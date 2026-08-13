#!/usr/bin/env python3
"""Emit the ParticleNavigatorEnv conformance fixture from the Python original.

The Rust port of `step()` is a *claim*. This is what makes it checkable. Nothing
pins that function against Python in any language today — the Swift twin's tests
are four hand-computed literals, and no fixture exists on either side.

REQUIRES the sibling NeuralCompose checkout and numpy. It does NOT degrade to a
synthetic stand-in when either is missing: a fixture generated from a
reimplementation would agree with the Rust port by construction and prove
exactly nothing. Absent input is a hard error.

    ~/src/NeuralCompose/.venv-muse/bin/python \
        tools/worldmodel-fixture/generate.py \
        > crates/neuralcompose-hypnagogic/tests/fixtures/env_v1.json

That interpreter is the one `band_power`'s 12-significant-figure reference
numbers came from, so the numeric provenance is the same as the existing
precedent.

FLOATS ARE EMITTED EXACTLY. Every value is a float32 widened to float64 before
serialization, which is lossless, and Python's repr round-trips float64. The
Rust side parses to f64 and narrows back to f32, recovering the identical bits —
so the conformance test can assert bit-equality rather than a tolerance, and a
tolerance cannot quietly absorb a real divergence.

`speed` and the per-step flags are recorded as DIAGNOSTICS, not as things the
port must reproduce. They exist so that a mismatch says *which* operation
diverged instead of only that the state did.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


def _git_commit(source_file: Path) -> str | None:
    """The commit `source_file` was read at, or None if that file is modified.

    Scoped to the FILE, not the tree. The pin claims "env.py as of this commit",
    so unrelated edits elsewhere in the checkout do not invalidate it — but an
    edit to env.py itself does, and then no commit is claimed at all. One
    `git status --porcelain=v2 --branch -- <file>` call, so the hash and the
    dirty state describe the same moment.

    `fileSha256` is the authority regardless; the commit is a convenience for
    finding the source, not the thing that verifies it.
    """
    try:
        out = subprocess.run(
            ["git", "status", "--porcelain=v2", "--branch", "--", source_file.name],
            cwd=source_file.parent, capture_output=True, text=True, check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    oid = None
    for line in out.splitlines():
        if line.startswith("# branch.oid "):
            oid = line.split()[2]
        elif not line.startswith("#"):
            return None  # env.py itself is modified
    return oid


def _today() -> str:
    from datetime import date, timezone, datetime

    return datetime.now(timezone.utc).date().isoformat()

SCHEMA = "neuralcompose.hypnagogic.env-conformance.v1"


def resolve_env_module() -> Path:
    root = Path(
        os.environ.get("NEURALCOMPOSE_DIR", Path.home() / "src" / "NeuralCompose")
    )
    world = root / "WorldModel"
    if not (world / "env.py").is_file():
        sys.exit(
            f"error: {world/'env.py'} not found.\n"
            "  The fixture is generated FROM the Python original; there is no\n"
            "  fallback, because a fixture built from a reimplementation would\n"
            "  agree with the port by construction.\n"
            "  Set NEURALCOMPOSE_DIR to the checkout, or clone it."
        )
    return world


sys.path.insert(0, str(resolve_env_module()))

try:
    import numpy as np
except ModuleNotFoundError:
    sys.exit(
        "error: numpy is required.\n"
        "  Run under the sibling repo's venv:\n"
        "    ~/src/NeuralCompose/.venv-muse/bin/python tools/worldmodel-fixture/generate.py"
    )

from env import EnvConfig, ParticleNavigatorEnv  # noqa: E402


def f(x) -> float:
    """float32 -> float64, exactly. Never narrows, never rounds."""
    return float(np.float32(x))


def action_script() -> list[tuple[float, float]]:
    """A deterministic action sequence chosen to REACH the interesting states.

    Coverage is not left to chance: a trajectory that never bounces would let a
    port with no wall handling pass, and one that never clamps would let a port
    with no speed limit pass. Each phase exists to make one of those impossible.
    """
    script: list[tuple[float, float]] = []
    # Phase 1 — full diagonal thrust: exceeds max_speed, then corners into (+,+).
    script += [(1.0, 1.0)] * 40
    # Phase 2 — full reverse diagonal: crosses the arena, corners into (-,-).
    script += [(-1.0, -1.0)] * 60
    # Phase 3 — x only: bounces the x wall while y drifts. Single-axis bounce.
    script += [(1.0, 0.0)] * 40
    # Phase 4 — y only, the other single-axis bounce.
    script += [(0.0, 1.0)] * 40
    # Phase 5 — small accelerations well under the clamp, so at least one step
    # exercises the path where the speed limit does NOT fire.
    script += [(0.05, -0.05)] * 40
    # Phase 6 — out-of-range actions, to pin the accel clip itself.
    script += [(5.0, -5.0)] * 20
    script += [(-3.0, 2.5)] * 20
    return script


def main() -> int:
    config = EnvConfig()
    env = ParticleNavigatorEnv(config)

    # A fixed start, not reset(). reset() consumes an RNG, and NumPy's generator
    # has no cross-language equivalent — seeding it would make the fixture
    # unreproducible from Rust for reasons that have nothing to do with step().
    state = np.array([-0.25, 0.4, 0.3, -0.15], dtype=np.float32)

    steps = []
    bounced_x = bounced_y = corner = clamped = unclamped = 0

    for ax, ay in action_script():
        action = np.array([ax, ay], dtype=np.float32)

        # Recompute the intermediates the way env.step does, purely to record
        # them. This mirrors env.py:57-63 and must be kept in step with it.
        vel_pre = state[2:] + np.clip(action, -config.max_accel, config.max_accel) * config.dt
        speed = float(np.linalg.norm(vel_pre))
        did_clamp = speed > config.max_speed

        nxt = env.step(state, action)

        hit_x = abs(float(nxt[0])) >= config.arena_half_extent
        hit_y = abs(float(nxt[1])) >= config.arena_half_extent
        bounced_x += hit_x
        bounced_y += hit_y
        corner += hit_x and hit_y
        clamped += did_clamp
        unclamped += not did_clamp

        steps.append(
            {
                "action": [f(ax), f(ay)],
                "next": [f(v) for v in nxt],
                "speedBeforeClamp": speed,
                "clamped": bool(did_clamp),
                "hitX": bool(hit_x),
                "hitY": bool(hit_y),
            }
        )
        state = nxt

    # Pin the SOURCE, not just the output. The fixture is generated FROM env.py,
    # so Python is the reference by construction rather than by test — and with
    # three conformant implementations, a stale fixture is LESS likely to be
    # noticed, not more: Rust and Swift would both keep passing against an
    # obsolete reference with nothing detecting it. Same convention as
    # contracts/provenance/fixtures/evidence-class-names.json.
    env_path = resolve_env_module() / "env.py"
    source = {
        "repo": "NeuralCompose",
        "path": "WorldModel/env.py",
        "fileSha256": hashlib.sha256(env_path.read_bytes()).hexdigest(),
        "commit": _git_commit(env_path),
        "readOn": _today(),
    }

    doc = {
        "schemaId": SCHEMA,
        "source": source,
        "note": (
            "Generated by tools/worldmodel-fixture/generate.py from "
            "WorldModel/env.py. Floats are float32 widened to float64 and are "
            "exact; the port is asserted bit-for-bit, not to a tolerance. "
            "speedBeforeClamp and the flags are diagnostics."
        ),
        "config": {
            "arenaHalfExtent": config.arena_half_extent,
            "dt": config.dt,
            "maxAccel": config.max_accel,
            "maxSpeed": config.max_speed,
            "restitution": config.restitution,
        },
        "initialState": [f(v) for v in [-0.25, 0.4, 0.3, -0.15]],
        "coverage": {
            "steps": len(steps),
            "bouncedX": bounced_x,
            "bouncedY": bounced_y,
            "cornerHits": corner,
            "clampedSteps": clamped,
            "unclampedSteps": unclamped,
        },
        "steps": steps,
    }

    # Refuse to emit a fixture that cannot discriminate. Every one of these is a
    # port defect the trajectory would otherwise pass over in silence.
    cov = doc["coverage"]
    missing = [k for k in ("bouncedX", "bouncedY", "cornerHits", "clampedSteps", "unclampedSteps") if cov[k] == 0]
    if missing or cov["steps"] < 200:
        sys.exit(f"error: fixture does not cover {missing or 'enough steps'}: {cov}")

    json.dump(doc, sys.stdout, indent=1)
    sys.stdout.write("\n")
    print(f"ok: {cov}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
