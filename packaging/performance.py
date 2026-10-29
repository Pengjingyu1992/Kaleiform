#!/usr/bin/env python3
"""Record unchanged CLI performance budgets; budget failures remain failures."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [str(args.cli.resolve()), "perf"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=300,
    )
    report = result.stdout + result.stderr
    (args.output / "performance.log").write_text(report, encoding="utf-8")
    rows = []
    for name, measured, budget, status in re.findall(
        r"^\s*(.+?)\s+(\d+\.\d+) ms\s+(\d+) ms\s+(ok|OVER)\s*$", report, re.M
    ):
        rows.append(
            {"operation": name, "measured_ms": float(measured), "budget_ms": float(budget), "status": status}
        )
    failed = result.returncode != 0 or len(rows) != 10 or any(row["status"] == "OVER" for row in rows)
    evidence = {
        "system": platform.system(),
        "architecture": platform.machine(),
        "source_sha": os.environ.get("VECTORCRAFT_BUILD_SHA"),
        "returncode": result.returncode,
        "status": "failed" if failed else "passed",
        "busy": "WARNING: the machine is busy" in report,
        "rows": rows,
    }
    (args.output / "performance.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print("::notice::Performance environment: " + report.splitlines()[0], flush=True)
        for row in rows:
            print(
                f"::notice::Performance {row['operation']}: {row['measured_ms']} ms / {row['budget_ms']} ms ({row['status']})",
                flush=True,
            )
        if failed:
            print(
                "::warning::Performance acceptance failed; see the recorded original budgets. Functional checks are separate.",
                flush=True,
            )
    print(report, end="", flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
