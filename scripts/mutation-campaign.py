#!/usr/bin/env python3
"""Mutation run for ADR-003's provenance vocabulary.

Two rules this codebase learned the hard way and this script enforces:

  * CONFIRM THE MUTATION APPLIED. A sed that matched nothing is indistinguishable
    from a surviving mutant and reads as a weaker suite than you have. Here the
    files are UNTRACKED, so `git diff --quiet` would report nothing whatever
    happened — the applied-check is `cmp` against a pre-mutation snapshot.
  * RESTORE FROM THE SNAPSHOT, never `git checkout --`. That command reverts to
    the last COMMIT and has twice destroyed uncommitted work in this session; on
    an untracked file it errors out and leaves the mutation in place.
"""
import filecmp
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from edit_guard import restore  # noqa: E402  the shared applied-check

REPO = Path(__file__).resolve().parent.parent
SNAP = Path(tempfile.mkdtemp(prefix="nc-mutation-"))
PROV = REPO / "crates/neuralcompose-mobile-core/src/provenance.rs"
EEG = REPO / "crates/neuralcompose-hypnagogic/src/eeg.rs"
WM = REPO / "crates/neuralcompose-hypnagogic/src/worldmodel.rs"

# (name, file, old, new, why it should die)
MUTANTS = [
    ("evidence_mapping always NeverIngestible", PROV,
     '        Observed => EvidenceMapping::Ingestible("observed"),',
     '        Observed => EvidenceMapping::NeverIngestible,',
     "tests 2, 3"),
    ("heuristic mapped onto agentInference", PROV,
     "        HeuristicAnnotation => EvidenceMapping::NeverIngestible,",
     '        HeuristicAnnotation => EvidenceMapping::Ingestible("agentInference"),',
     "tests 2, 3"),
    ("comparable uses plain equality", PROV,
     "    match (&a.comparison_embedding_space, &b.comparison_embedding_space) {\n        (Some(x), Some(y)) => x == y,\n        _ => false,\n    }",
     "    a.comparison_embedding_space == b.comparison_embedding_space",
     "test 4, the (None,None) case only"),
    ("comparable always true", PROV,
     "    match (&a.comparison_embedding_space, &b.comparison_embedding_space) {\n        (Some(x), Some(y)) => x == y,\n        _ => false,\n    }",
     "    true",
     "test 4"),
    ("validate returns no defects", PROV,
     "    let mut out = Vec::new();\n\n    if env.schema_id != PROVENANCE_ENVELOPE_SCHEMA {",
     "    let mut out = Vec::new();\n    if true {\n        return out;\n    }\n\n    if env.schema_id != PROVENANCE_ENVELOPE_SCHEMA {",
     "test 7 every row, test 9"),
    ("derivation may name no inputs", PROV,
     "    if env.assertion_kind == AssertionKind::DerivedDeterministically && env.inputs.is_empty() {",
     "    if false {",
     "test 7 row 2, test 9"),
    ("confidence applicable to every kind", PROV,
     "        if !takes_confidence(env.assertion_kind) {",
     "        if false {",
     "test 7 row 4"),
    ("confidence upper bound made exclusive", PROV,
     "        } else if !(0.0..=1.0).contains(&c) {",
     "        } else if !(0.0..1.0).contains(&c) {",
     "test 10 via the confidence:1.0 valid fixture"),
    ("every kind requires a method", PROV,
     "fn requires_method(kind: AssertionKind) -> bool {\n    matches!(\n        kind,\n        AssertionKind::DerivedDeterministically\n            | AssertionKind::AgentInference\n            | AssertionKind::HeuristicAnnotation\n    )\n}",
     "fn requires_method(_kind: AssertionKind) -> bool {\n    true\n}",
     "test 9 negative half, test 10"),
    ("parameters digest unchecked", PROV,
     '            if !is_lower_hex(&m.parameters_digest, 64) {',
     '            if false {',
     "test 7 row 8"),
    ("a missing field silently defaults", PROV,
     '    #[serde(deserialize_with = "present_option")]\n    pub method: Option<MethodIdentity>,',
     "    pub method: Option<MethodIdentity>,",
     "test 5"),
    ("stale samples reported as current", EEG,
     "    if !matches!(phase, StreamPhase::Live) {\n        return None;\n    }",
     "    if false {\n        return None;\n    }",
     "a_stale_stream_reports_nothing / no_phase_but_live_reports_anything"),
    ("a partial montage reported short", EEG,
     "    if channels.len() != CHANNEL_COUNT || channels.iter().any(|c| c.is_empty()) {",
     "    if false {",
     "a_partial_montage_is_refused / a_channel_with_no_samples_yet"),
    ("NaN allowed into a turn line", EEG,
     "    if reports.iter().any(|r| !r.rms.is_finite()) {\n        return None;\n    }",
     "    if false {\n        return None;\n    }",
     "a_non_finite_sample_never_reaches_a_turn_line"),
    ("env speed clamp removed", WM,
     "    if speed > config.max_speed as f64 {",
     "    if false {",
     "env_conformance / the_speed_clamp_is_isotropic / never_leaves_the_arena"),
    ("env restitution is perfectly elastic", WM,
     "    restitution: 0.6,",
     "    restitution: 1.0,",
     "the_defaults_are_the_python_and_swift_values / a_wall_reverses_the_velocity"),
    ("env wall clamp dropped so position escapes", WM,
     "        if pos[i] > config.arena_half_extent {\n            pos[i] = config.arena_half_extent;",
     "        if pos[i] > config.arena_half_extent {\n            pos[i] = pos[i];",
     "env_conformance / the_particle_never_leaves_the_arena"),
    ("env clamps speed AFTER integrating position", WM,
     "    // Position integrates with the ALREADY-CLAMPED velocity (`env.py:63`).\n    for i in 0..2 {\n        pos[i] += vel[i] * config.dt;\n    }",
     "    for i in 0..2 {\n        pos[i] += (vel[i] / 1.0000001) * config.dt;\n    }",
     "env_conformance bit-exactness"),
    ("env NaN guard removed", WM,
     "    if !state.iter().all(|v| v.is_finite()) || !action.iter().all(|v| v.is_finite()) {\n        return None;\n    }",
     "    if false {\n        return None;\n    }",
     "a_non_finite_input_is_refused_rather_than_laundered"),
    ("goal tolerance loosened tenfold", WM,
     "pub const GOAL_TOLERANCE: f32 = 0.1;",
     "pub const GOAL_TOLERANCE: f32 = 1.0;",
     "an_episode_already_at_the_goal_takes_no_steps + the_mpc_defaults"),
]

CMD = ["cargo", "+1.97.1", "test", "-j4",
       "-p", "neuralcompose-mobile-core", "-p", "neuralcompose-hypnagogic"]


def run_suite():
    r = subprocess.run(CMD, cwd=REPO, capture_output=True, text=True)
    return r.returncode == 0


def main():
    originals = {}
    for _, f, _, _, _ in MUTANTS:
        dst = SNAP / f.name
        if f not in originals:
            shutil.copy2(f, dst)
            originals[f] = dst

    print("baseline (unmutated) ...", flush=True)
    if not run_suite():
        sys.exit("baseline suite is RED — fix that before mutating")
    print("  baseline GREEN")

    # ---- CANARY: verify the HARNESS before trusting any verdict ----
    #
    # Three campaigns, three harness failures, every one the verifying tool
    # being itself unverified: `git diff` blind to an untracked file, a fixture
    # keyed on call order that desynced, and copy2 preserving mtime so a stale
    # binary answered for fresh source. The last is the worst, because a stale
    # binary is wrong in EITHER direction — a phantom KILLED is the same bug
    # wearing a comfortable face.
    #
    # So: one edit that MUST be detected, one that MUST NOT be. If either
    # disagrees, the harness is lying and no verdict below means anything.
    # All three past failures break this canary.
    f = PROV
    snap = originals[f]
    print("canary ...", flush=True)

    detectable = ("    match (&a.comparison_embedding_space, &b.comparison_embedding_space) {\n        (Some(x), Some(y)) => x == y,\n        _ => false,\n    }", "    true")
    text = f.read_text()
    assert text.count(detectable[0]) == 1, "canary pattern is stale"
    f.write_text(text.replace(*detectable, 1))
    if run_suite():
        restore(f, snap)
        sys.exit("CANARY FAILED: a known-detectable mutation was not detected. "
                 "The harness cannot see its own edits — stale artifact, wrong "
                 "target, or a suite that never runs. Every verdict would be a lie.")
    restore(f, snap)

    inert = "//! CANARY: inert comment, must not change any verdict.\n"
    f.write_text(inert + snap.read_text())
    if not run_suite():
        restore(f, snap)
        sys.exit("CANARY FAILED: an inert comment turned the suite RED. The "
                 "baseline is not reproducible, so a KILLED verdict cannot be "
                 "attributed to the mutation.")
    restore(f, snap)
    assert filecmp.cmp(f, snap, shallow=False), "canary restore failed"
    print("  canary GREEN (detectable edit seen, inert edit ignored)\n")

    survivors = []
    for name, f, old, new, why in MUTANTS:
        text = f.read_text()
        if text.count(old) != 1:
            print(f"SKIPPED  {name}\n         pattern matched {text.count(old)}x, not 1 — "
                  f"the mutation is stale, NOT a survivor")
            survivors.append((name, "stale pattern"))
            continue
        f.write_text(text.replace(old, new, 1))

        # The applied-check. Untracked file: cmp, not `git diff --quiet`.
        if filecmp.cmp(f, originals[f], shallow=False):
            print(f"SKIPPED  {name}\n         file unchanged after edit — not a survivor")
            survivors.append((name, "did not apply"))
            restore(f, originals[f])
            continue

        killed = not run_suite()
        restore(f, originals[f])
        assert filecmp.cmp(f, originals[f], shallow=False), "restore failed"

        print(f"{'KILLED ' if killed else 'SURVIVED'}  {name}   ({why})", flush=True)
        if not killed:
            survivors.append((name, "SURVIVED"))

    # ---- POST-CAMPAIGN: the restore is part of the harness too ----
    #
    # This is where the mtime bug actually bit: the final copy2 restore left a
    # stale artifact, and the NEXT session's baseline came back red for code
    # that was correct. The source matching the snapshot is necessary but not
    # sufficient — the build has to agree.
    print("\nfinal restore ...", flush=True)
    for f2, snap2 in originals.items():
        assert filecmp.cmp(f2, snap2, shallow=False), f"{f2} does not match its snapshot"
    if not run_suite():
        sys.exit("HARNESS FAULT: every source file matches its pre-campaign snapshot, "
                 "yet the suite is RED. The tree is clean, so this is the build "
                 "reusing an artifact from a mutant — not a code defect. Touch the "
                 "sources or cargo clean before believing any verdict above.")
    print("  restored tree is GREEN")

    print()
    if survivors:
        print("NOT KILLED:")
        for n, r in survivors:
            print(f"  {n}: {r}")
        sys.exit(1)
    print(f"all {len(MUTANTS)} mutants killed")


if __name__ == "__main__":
    main()
