"""Verify the source-only, pinned CS Demo Manager baseline byte for byte.

This never installs or executes upstream dependencies or bundled binaries.
"""
import csv
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = ROOT / "upstream" / "cs-demo-manager"
LEDGER = ROOT / "docs" / "MIGRATION_LEDGER.csv"
ROOT_FILES = {
    "LICENSE", "package.json", "pnpm-lock.yaml", "pnpm-workspace.yaml",
    "tsconfig.json", "vite.config.ts", "postcss.config.mjs", "lingui.config.ts",
    "scripts/electron-vendors.mjs",
}
EXCLUDED = {
    "src/node/os/get-running-process-exit-code/build/Release/get_running_process_exit_code.node",
}


def git_blob_sha(data: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def main() -> None:
    with LEDGER.open(newline="", encoding="utf-8") as source:
        rows = list(csv.DictReader(source))
    expected = {
        row["upstream_path"]: row["git_object_sha"]
        for row in rows
        if (row["upstream_path"].startswith("src/") or row["upstream_path"] in ROOT_FILES)
        and row["upstream_path"] not in EXCLUDED
    }
    assert len(expected) == 2827, f"unexpected manifest count: {len(expected)}"
    actual = {path.relative_to(SNAPSHOT).as_posix() for path in SNAPSHOT.rglob("*") if path.is_file()}
    actual.remove("LICENSE.inter.txt")
    assert actual == set(expected), f"snapshot paths differ: missing={set(expected)-actual}, extra={actual-set(expected)}"
    for relative, sha in expected.items():
        assert git_blob_sha((SNAPSHOT / relative).read_bytes()) == sha, f"blob changed: {relative}"
    for relative in EXCLUDED:
        assert not (SNAPSHOT / relative).exists(), f"excluded payload present: {relative}"
    assert (SNAPSHOT / "LICENSE").read_text(encoding="utf-8").startswith("The MIT License (MIT)")
    inter_license = (SNAPSHOT / "LICENSE.inter.txt").read_bytes()
    assert git_blob_sha(inter_license) == "9b2ca37b3ffc77391d8b2ebef4a974ef32bf46ea"
    print(f"PASS: {len(expected)} pinned upstream files match ledger Git blobs; native payload absent; Inter OFL notice verified")


if __name__ == "__main__":
    main()
