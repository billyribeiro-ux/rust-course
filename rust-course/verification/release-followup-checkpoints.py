#!/usr/bin/env python3
"""Read-only package audit; reconstructed sources/metadata stay in the outer cache."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import tomllib
import zipfile

OUTER = Path(__file__).resolve().parents[2]
COURSE = OUTER / "rust-course"
ARCHIVE = OUTER / "rust-course-review.zip"
TARGET = OUTER / ".course-cache/native"
WORK = Path(tempfile.mkdtemp(prefix="release-followup-portability-", dir=OUTER / ".course-cache"))
REPORT = COURSE / "verification/release-followup-checkpoints.json"
LOG = COURSE / "verification/release-followup-checkpoints.log"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(command, log, output=None):
    log.write("$ " + shlex.join(command) + "\n")
    result = subprocess.run(command, cwd=OUTER, capture_output=True, text=True, check=False)
    if output:
        output.write_text(result.stdout)
        log.write(f"stdout saved to {output}; sha256={sha(result.stdout.encode())}\n")
    else:
        log.write(result.stdout)
    log.write(result.stderr)
    log.write(f"exit_code={result.returncode}\n")
    log.flush()
    if result.returncode:
        raise RuntimeError(f"Command failed: {shlex.join(command)}")
    return result.stdout


def main():
    result = {"date": "2026-10-09", "scope": "Exact review archive; no compilation or browser interaction",
              "archive": str(ARCHIVE), "archive_sha256": sha(ARCHIVE.read_bytes()),
              "archive_bytes": ARCHIVE.stat().st_size, "work_directory": str(WORK),
              "target_directory": str(TARGET), "independent_review": "pending", "checks": []}
    try:
        with zipfile.ZipFile(ARCHIVE) as archive:
            result["archive_members"] = len(archive.infolist())
            assert archive.testzip() is None, "Corrupt ZIP member"
            for member in archive.infolist():
                resolved = (WORK / "extracted" / member.filename).resolve()
                assert resolved.is_relative_to(WORK / "extracted/rust-course"), member.filename
                assert not (member.external_attr >> 16) & 0o170000 == 0o120000, member.filename
                parts = Path(member.filename).parts
                assert not {"target", ".local", "learner-work", "node_modules", "pkg", ".git"}.intersection(parts), member.filename
                assert not any(p == ".env" or p.startswith(".env.") for p in parts), member.filename
            archive.extractall(WORK / "extracted")
        root = WORK / "extracted/rust-course"
        status = json.loads((root / "course-status.json").read_text())
        result["lesson_count"] = len(status["lessons"])
        for lesson in status["lessons"]:
            assert (root / lesson["path"]).is_file(), lesson["path"]
        baseline = {}
        for lock in (root / "projects/Cargo.lock", root / "projects/ui-console/Cargo.lock"):
            baseline[lock] = {(p["name"], p["version"], p.get("source"), p.get("checksum"))
                              for p in tomllib.loads(lock.read_text())["package"] if p.get("source")}
        manifests = sorted((root / "projects/checkpoints").glob("*/checkpoint.json"))
        result["checkpoint_project_count"] = len(manifests)
        result["checkpoint_count"] = sum(len(json.loads(p.read_text())["checkpoints"]) for p in manifests)
        with LOG.open("w") as log:
            log.write(f"archive_sha256={result['archive_sha256']}\nwork_directory={WORK}\n")
            for manifest in manifests:
                specification = json.loads(manifest.read_text())
                assert specification["schema_version"] == 1
                for checkpoint in specification["checkpoints"]:
                    project, checkpoint_id = specification["project"], checkpoint["id"]
                    source = (root / checkpoint["source"]).resolve()
                    assert source.is_relative_to(root) and (source / "Cargo.toml").is_file()
                    lock = (root / checkpoint["lockfile"]) if checkpoint.get("lockfile") else source / "Cargo.lock"
                    assert lock.is_file(), f"Missing canonical lock: {project}/{checkpoint_id}"
                    destination = WORK / "reconstructed" / project / checkpoint_id
                    helper_command = ["python3", str(root / "scripts/start-checkpoint.py"), project, checkpoint_id,
                                      "--destination", str(destination)]
                    run(helper_command, log)
                    assert (destination / "Cargo.lock").read_bytes() == lock.read_bytes()
                    metadata_file = WORK / f"{project}-{checkpoint_id}-metadata.json"
                    command = ["mise", "exec", "--", "cargo", "metadata", "--offline", "--locked",
                               "--format-version", "1", "--manifest-path", str(destination / "Cargo.toml")]
                    # cargo metadata does not build; env still points at the one shared native target.
                    previous_target = os.environ.get("CARGO_TARGET_DIR")
                    os.environ["CARGO_TARGET_DIR"] = str(TARGET)
                    try:
                        metadata = json.loads(run(command, log, metadata_file))
                    finally:
                        if previous_target is None:
                            os.environ.pop("CARGO_TARGET_DIR", None)
                        else:
                            os.environ["CARGO_TARGET_DIR"] = previous_target
                    assert Path(metadata["target_directory"]) == TARGET
                    assert (destination / "Cargo.lock").read_bytes() == lock.read_bytes(), "Lock changed"
                    local = [p for p in metadata["packages"] if p["source"] is None]
                    assert all(Path(p["manifest_path"]).resolve().is_relative_to(destination) for p in local)
                    assert not list(destination.rglob("target")), "A copied target appeared"
                    reference_lock = root / ("projects/ui-console/Cargo.lock" if project == "p11-console" else "projects/Cargo.lock")
                    external = {(p["name"], p["version"], p.get("source"), p.get("checksum"))
                                for p in tomllib.loads(lock.read_text())["package"] if p.get("source")}
                    assert external <= baseline[reference_lock], f"Dependency drift in {project}/{checkpoint_id}"
                    entry = {"project": project, "checkpoint": checkpoint_id, "source": checkpoint["source"],
                             "lockfile": str(lock.relative_to(root)), "lock_sha256": sha(lock.read_bytes()),
                             "source_manifest_sha256": sha((source / "Cargo.toml").read_bytes()),
                             "helper_command": helper_command, "metadata_command": command,
                             "metadata_stdout": str(metadata_file), "resolved_packages": len(metadata["packages"]),
                             "local_packages": [p["name"] for p in local], "path_dependencies_contained": True,
                             "lock_unchanged": True, "reference_dependency_versions_match": True, "result": "passed"}
                    result["checks"].append(entry)
                    log.write(f"PASS {project}/{checkpoint_id}: packages={len(metadata['packages'])}; local={entry['local_packages']}\n")
        result["result"] = "passed"
        result["limitations"] = ["Metadata proves locked resolution and path containment, not compilation or runtime behavior.",
                                 "The toolchain was invoked through the already configured outer mise environment; no new global trust configuration was written.",
                                 "Archive hash identifies the exact input snapshot; later source/test/evidence edits require repackaging.",
                                 "Source and raw metadata copies remain in the reported outer cache; no build output was duplicated."]
    except (AssertionError, OSError, ValueError, RuntimeError, KeyError) as error:
        result["result"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        REPORT.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: result[k] for k in ("result", "archive_sha256", "archive_members", "lesson_count", "checkpoint_count", "work_directory")}, indent=2))


if __name__ == "__main__":
    main()
