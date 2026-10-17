#!/usr/bin/env python3
"""macOS process resource profile; Rust example itself is platform-neutral.

Build examples in release mode first. Writes new evidence; never overwrites a run.
"""
import argparse
import datetime
import hashlib
import json
import platform
import re
import statistics
import subprocess
import time
from pathlib import Path


def bounded(maximum):
    def parse(text):
        value = int(text)
        if not 1 <= value <= maximum:
            raise argparse.ArgumentTypeError(f"expected 1..{maximum}")
        return value
    return parse


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rows", type=bounded(1_000_000), default=750_000)
    parser.add_argument("--passes", type=bounded(200), default=80)
    parser.add_argument("--samples", type=bounded(31), default=7)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("this resource-counter driver requires macOS /usr/bin/time -l; run the Rust example or adapt the profiler on another OS")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("build the release example before profiling")
    output = args.output.resolve()
    if output.exists():
        parser.error("output already exists; choose a new run directory")
    output.mkdir(parents=True)
    records = []
    expected = None
    for sample in range(args.samples):
        variants = ["aos", "soa"] if sample % 2 == 0 else ["soa", "aos"]
        for position, mode in enumerate(variants):
            label = f"{sample + 1:02}-{position + 1}-{mode}"
            resource_file = output / f"{label}.resources.txt"
            command = ["/usr/bin/time", "-l", "-p", "-o", str(resource_file), str(binary), mode, str(args.rows), str(args.passes)]
            started = time.perf_counter_ns()
            result = subprocess.run(command, capture_output=True, text=True, timeout=60)
            elapsed = time.perf_counter_ns() - started
            (output / f"{label}.stdout.txt").write_text(result.stdout)
            (output / f"{label}.stderr.txt").write_text(result.stderr)
            if result.returncode:
                raise SystemExit(f"profile failed ({result.returncode}); inspect {label} logs; partial run is not complete evidence")
            fields = dict(part.split("=", 1) for part in result.stdout.strip().split("\t"))
            values = {key: int(value) for key, value in fields.items() if key != "layout"}
            if fields["layout"] != mode or values["rows"] != args.rows or values["passes"] != args.passes:
                raise SystemExit("profiler output does not match requested workload")
            if expected is None:
                expected = values["checksum"]
            if values["checksum"] != expected:
                raise SystemExit("cross-variant checksum differs; do not interpret timings")
            resource = resource_file.read_text()
            rss = re.search(r"^\s*(\d+)\s+maximum resident set size\s*$", resource, re.M)
            if rss is None:
                raise SystemExit("macOS maximum RSS counter absent; inspect resource logs")
            records.append({"sample": sample + 1, "position": position + 1, "layout": mode, "command": command, "exit_code": result.returncode, "process_elapsed_ns": elapsed, "max_rss_bytes": int(rss.group(1)), **values})
            print(f"{label}: scan {values['scan_ns'] / 1e6:.3f} ms, maximum RSS {int(rss.group(1)):,} bytes", flush=True)
    summaries = {}
    for mode in ["aos", "soa"]:
        summaries[mode] = {}
        for metric in ["generation_ns", "validation_ns", "scan_ns", "process_elapsed_ns", "max_rss_bytes"]:
            values = [row[metric] for row in records if row["layout"] == mode]
            summaries[mode][metric] = {"minimum": min(values), "median": statistics.median(values), "maximum": max(values)}
    hardware = {}
    for name in ["machdep.cpu.brand_string", "hw.logicalcpu", "hw.memsize"]:
        result = subprocess.run(["sysctl", "-n", name], capture_output=True, text=True, timeout=10)
        hardware[name] = {"exit_code": result.returncode, "value": result.stdout.strip(), "stderr": result.stderr.strip()}
    report = {"date_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "platform": platform.platform(), "hardware": hardware, "profile": "release; executable refuses debug_assertions", "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "rows": args.rows, "passes": args.passes, "samples_per_variant": args.samples, "within_process_warmups": 2, "method": "Separate process per sample; interleaved variant order. Full-record comparison and analytic oracle before two warm-ups. Scan timer excludes construction, validation, warm-ups and destruction. Process timer/RSS includes all of those plus startup/profiler overhead. RSS units verified in the installed macOS getrusage(2) manual; not GNU time units.", "records": records, "summaries": summaries, "limitations": ["Synthetic read-mostly analytics, not full replay, I/O or mutation workload.", "No hardware cache-miss counter or call-stack sample was collected; cache locality is a supported interpretation, not measured attribution.", "Shared development host; CPU affinity, background load, power and thermals were not controlled.", "Capacity payload bytes exclude allocator metadata and code; RSS is a process peak, not allocation counts.", "Counter driver is macOS-specific; Rust example is portable."], "independent_review": "pending"}
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summaries, indent=2))


if __name__ == "__main__":
    main()
