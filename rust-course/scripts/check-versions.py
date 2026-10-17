#!/usr/bin/env python3
"""Record dated publisher/registry evidence; never infer releases from a schedule."""
import argparse
import concurrent.futures
import datetime
import hashlib
import json
from pathlib import Path
import re
import tomllib
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def stable(version):
    match = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", version)
    return tuple(map(int, match.groups())) if match else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--as-of", default="2026-10-17")
    args = parser.parse_args()
    cutoff = datetime.date.fromisoformat(args.as_of).isoformat()
    output = ROOT / "verification" / ("versions-" + cutoff)
    output.mkdir(exist_ok=True)
    crates = {}
    for manifest in ROOT.rglob("Cargo.toml"):
        if set(manifest.relative_to(ROOT).parts) & {".local", "target", "node_modules", "learner-work"}:
            continue
        data = tomllib.loads(manifest.read_text())
        for table in (data, data.get("workspace", {})):
            for section in ("dependencies", "dev-dependencies", "build-dependencies"):
                for name, dep in table.get(section, {}).items():
                    version = dep if isinstance(dep, str) else dep.get("version")
                    if version:
                        crates.setdefault(name, set()).add(version)
    for name in ("cargo-audit", "cargo-fuzz", "sqlx-cli", "wasm-bindgen-cli"):
        crates.setdefault(name, set())
    package = json.loads((ROOT / "experiments/svelte-bridge/package.json").read_text())
    npm = {**package["dependencies"], **package["devDependencies"]}
    jobs = [("crate", name, sorted(pins)) for name, pins in crates.items()]
    jobs += [("npm", name, [pin]) for name, pin in npm.items()]
    jobs += [("node", "node", []), ("github", "jdx/mise", [])]

    def check(job):
        kind, name, pins = job
        url = {
            "crate": "https://crates.io/api/v1/crates/" + name,
            "npm": "https://registry.npmjs.org/" + urllib.parse.quote(name, safe=""),
            "node": "https://nodejs.org/dist/index.json",
            "github": "https://api.github.com/repos/" + name + "/releases?per_page=100",
        }[kind]
        request = urllib.request.Request(url, headers={"User-Agent": "rust-course-evidence-audit/1.0"})
        with urllib.request.urlopen(request, timeout=60) as response:
            raw = response.read()
        filename = kind + "-" + name.replace("/", "_").replace("@", "") + ".json"
        (output / filename).write_bytes(raw)
        data = json.loads(raw)
        if kind == "crate":
            releases = [{"version": v["num"], "published": v["created_at"], "rust_version": v.get("rust_version")}
                        for v in data["versions"] if not v["yanked"]]
        elif kind == "npm":
            releases = [{"version": v, "published": date, "engines": data["versions"].get(v, {}).get("engines"),
                         "peer_dependencies": data["versions"].get(v, {}).get("peerDependencies")}
                        for v, date in data["time"].items() if v in data["versions"]]
        elif kind == "node":
            releases = [{"version": v["version"], "published": v["date"], "lts": v["lts"]} for v in data]
        else:
            releases = [{"version": v["tag_name"], "published": v["published_at"], "release_url": v["html_url"]}
                        for v in data if not v["draft"] and not v["prerelease"]]
        candidates = [v for v in releases if stable(v["version"]) and v["published"][:10] <= cutoff]
        latest = max(candidates, key=lambda v: stable(v["version"]))
        record = {"kind": kind, "name": name, "declared_versions": pins, "latest_stable": latest,
                  "source": url, "snapshot": output.name + "/" + filename, "sha256": hashlib.sha256(raw).hexdigest()}
        if kind == "node":
            record["latest_lts"] = max((v for v in candidates if v["lts"]), key=lambda v: stable(v["version"]))
        print(name, latest["version"], latest["published"], flush=True)
        return record

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        records = list(pool.map(check, jobs))
    report = {"as_of": cutoff, "retrieved_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "policy": "Highest published stable version on or before cutoff; excludes prereleases and currently yanked crates. Registry snapshots cannot prove past yank status.",
              "packages": sorted(records, key=lambda x: (x["kind"], x["name"]))}
    (ROOT / "verification" / ("versions-" + cutoff + ".json")).write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
