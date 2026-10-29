#!/usr/bin/env python3
"""Check the Windows portable payload before distributing either Windows package."""
import argparse
from pathlib import Path, PurePosixPath
import re
import zipfile


PATTERNS = [re.compile(pattern) for pattern in (
    rb"gh[pousr]_[A-Za-z0-9]{30,}", rb"github_pat_[A-Za-z0-9_]{50,}",
    rb"AKIA[0-9A-Z]{16}", rb"xox[baprs]-[A-Za-z0-9-]{20,}",
    rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
    rb"/workspace/", rb"/(?:home|Users)/[A-Za-z0-9_.-]+/",
)]
PATTERNS.append(re.compile(rb"[A-Z]:[\\/]+Users[\\/]+(?!Public[\\/]|Default[\\/])[^\\/\x00\r\n]{1,80}[\\/]", re.I))


def check(path):
    root = str(Path(__file__).resolve().parents[2])
    build_paths = [root.encode(), root.replace("\\", "/").encode()]
    executables = set()
    with zipfile.ZipFile(path) as archive:
        for entry in archive.infolist():
            name = PurePosixPath(entry.filename.replace("\\", "/"))
            if name.is_absolute() or re.match(r"^[A-Za-z]:", str(name)) or ".." in name.parts or any(p in {".git", ".env", ".ssh", ".aws", "__pycache__"} for p in name.parts):
                raise ValueError("Unsafe path or local configuration in Windows archive")
            if name.suffix.lower() in {".pdb", ".log", ".dmp", ".tmp"}:
                raise ValueError("Debug data or runtime files in Windows archive")
            if entry.is_dir():
                continue
            if entry.file_size > 512 * 1024 * 1024:
                raise ValueError("Unexpectedly large Windows payload file")
            data = archive.read(entry)
            if any(pattern.search(data) for pattern in PATTERNS) or any(p in data for p in build_paths):
                # Never print matched values or private paths.
                raise ValueError("Potential credential or private build path in Windows payload")
            if name.suffix.lower() == ".exe":
                if not data.startswith(b"MZ"):
                    raise ValueError("Invalid Windows executable")
                executables.add(name.name)
    if executables != {"vectorcraft.exe", "vectorcraft-cli.exe"}:
        raise ValueError("Portable payload does not contain the expected Windows executables")
    print("Windows payload privacy and file selection checks passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    check(parser.parse_args().archive)
