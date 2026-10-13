#!/usr/bin/env python3
"""Reject mismatched Linux release binaries; print their required glibc version."""
import argparse
import os
import re
import subprocess
import sys
from pathlib import Path


def inspect(binary, arch, version):
    env = dict(os.environ, LC_ALL="C")
    header = subprocess.check_output(["readelf", "-h", str(binary)], text=True, env=env)
    machine = {"x86_64": "Advanced Micro Devices X86-64", "aarch64": "AArch64"}[arch]
    if not re.search(r"Class:\s+ELF64", header) or not re.search(r"Machine:\s+" + re.escape(machine) + r"\s*$", header, re.M):
        raise ValueError(f"{binary}: expected a Linux {arch} ELF64 executable")
    actual = subprocess.check_output([str(binary), "--version"], text=True, env=env).strip()
    if actual != f"{binary.name} {version}":
        raise ValueError(f"{binary}: expected version {version}, got {actual!r}; rebuild before packaging")
    symbols = subprocess.check_output(["readelf", "--version-info", str(binary)], text=True, env=env)
    versions = [tuple(map(int, v.split("."))) for v in re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", symbols)]
    if not versions:
        raise ValueError(f"{binary}: no glibc symbol requirements found; this packager expects GNU/Linux binaries")
    return max(versions)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--arch", choices=("x86_64", "aarch64"), required=True)
    p.add_argument("--version", required=True)
    p.add_argument("binaries", type=Path, nargs="+")
    args = p.parse_args()
    # Ubuntu 22.04 is the supported baseline, even when an ELF uses older symbols.
    floor = max((2, 35), *(inspect(b, args.arch, args.version) for b in args.binaries))
    print(".".join(map(str, floor)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)
