#!/usr/bin/env python3
"""Programmatic source edits that cannot silently do nothing.

**Three failures, one family.** A mutation harness reported SURVIVED for an edit
that never applied (`git diff --quiet` is blind to an untracked file). A restore
that preserved mtime left the build testing a stale binary against correct
source. And a string replace with no count assertion matched nothing, because
`cargo fmt` had reflowed the lines it was looking for — the run printed the old
output and looked fine.

Each was found in the tooling built to catch exactly that class, which is the
point: **the checking tool has its own always-passing mode.** The guard belongs
here rather than inside any one harness, because an unguarded edit is the same
silent no-op whichever script performs it.

    from edit_guard import replace_once, snapshot, restore

    snap = snapshot(path)
    replace_once(path, old, new)   # raises unless exactly one match AND the file changed
    ...
    restore(path, snap)            # rewrites contents, so the mtime is NOW

Run `python3 scripts/edit_guard.py` to self-check.
"""

from __future__ import annotations

import filecmp
import shutil
import tempfile
from pathlib import Path


class EditNotApplied(RuntimeError):
    """Raised when an edit did not change the file it claimed to change.

    Deliberately an exception rather than a return code: the whole failure mode
    is a caller carrying on as though the edit had worked.
    """


def replace_once(path: Path, old: str, new: str) -> None:
    """Replace `old` with `new`, exactly once, verifying it took effect.

    Raises `EditNotApplied` if `old` appears any number of times other than one,
    or if the file content is unchanged afterwards. Both checks matter: the count
    catches a stale or ambiguous pattern, and the content check catches an edit
    that "succeeded" because `old == new`.
    """
    path = Path(path)
    before = path.read_text()
    n = before.count(old)
    if n != 1:
        head = old.splitlines()[0][:70] if old else ""
        raise EditNotApplied(
            f"{path}: pattern matched {n} times, expected exactly 1.\n"
            f"  pattern began: {head!r}\n"
            "  A pattern that matches nothing is indistinguishable from an edit that\n"
            "  did nothing. Re-read the file — a formatter may have reflowed it."
        )
    after = before.replace(old, new, 1)
    if after == before:
        raise EditNotApplied(f"{path}: replacement is identical to the original")
    path.write_text(after)


def snapshot(path: Path) -> Path:
    """Copy `path` somewhere temporary and return the copy."""
    path = Path(path)
    dst = Path(tempfile.mkdtemp(prefix="edit-guard-")) / path.name
    shutil.copy2(path, dst)
    return dst


def restore(path: Path, snap: Path) -> None:
    """Put `snap`'s contents back, with a CURRENT mtime.

    Not `shutil.copy2` and not `cp -p`: those preserve the snapshot's timestamp,
    which is older than whatever the build produced from the edited source. The
    build then skips its rebuild and the next run tests the edited binary against
    restored source — a wrong verdict in *either* direction, which is worse than
    an obviously broken one.

    Not `git checkout --` either: that reverts to the last *commit*, which has
    destroyed uncommitted work here twice, and on an untracked file it errors and
    leaves the edit in place.
    """
    path, snap = Path(path), Path(snap)
    path.write_text(snap.read_text())
    if not filecmp.cmp(path, snap, shallow=False):
        raise EditNotApplied(f"{path}: restore did not reproduce the snapshot")


def _self_check() -> int:
    """Prove each guard fires. A guard nobody has seen fail is a guess."""
    import sys

    tmp = Path(tempfile.mkdtemp(prefix="edit-guard-selfcheck-"))
    f = tmp / "sample.txt"
    original = "alpha\nbeta\ngamma\nbeta\n"
    ok = True

    def expect_raise(label: str, fn) -> None:
        nonlocal ok
        try:
            fn()
        except EditNotApplied:
            print(f"  ok    {label}")
            return
        print(f"  FAIL  {label}: no exception")
        ok = False

    f.write_text(original)
    expect_raise("pattern matching zero times", lambda: replace_once(f, "delta", "x"))
    expect_raise("pattern matching twice", lambda: replace_once(f, "beta", "x"))
    expect_raise("replacement identical to original", lambda: replace_once(f, "alpha", "alpha"))
    assert f.read_text() == original, "a failed edit must not modify the file"
    print("  ok    a failed edit leaves the file untouched")

    replace_once(f, "gamma", "GAMMA")
    if "GAMMA" in f.read_text():
        print("  ok    a valid edit applies")
    else:
        print("  FAIL  a valid edit did not apply")
        ok = False

    # The mtime property the restore exists for.
    snap = snapshot(f)
    import os
    import time

    old_time = os.stat(snap).st_mtime
    time.sleep(0.01)
    f.write_text("mutated\n")
    restore(f, snap)
    if f.read_text() != snap.read_text():
        print("  FAIL  restore did not reproduce the contents")
        ok = False
    elif os.stat(f).st_mtime <= old_time:
        print("  FAIL  restore preserved the old mtime — a build would skip its rebuild")
        ok = False
    else:
        print("  ok    restore rewrites contents and advances the mtime")

    print("edit-guard self-check: " + ("ok" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(_self_check())
