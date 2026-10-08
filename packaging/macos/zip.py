#!/usr/bin/env python3
"""Create a portable ZIP with UTF-8 names, Unix modes and no Finder metadata."""

import os
from pathlib import Path
import stat
import sys
import zipfile


def archive(source: Path, output: Path) -> None:
    source = source.resolve()
    output = output.resolve()
    if output.is_relative_to(source):
        raise ValueError("output must be outside the packaged directory")
    temporary = output.with_suffix(output.suffix + ".tmp")
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
        for path in [source, *sorted(source.rglob("*"))]:
            name = path.relative_to(source.parent).as_posix()
            if path.is_symlink():
                entry = zipfile.ZipInfo(name)
                entry.create_system = 3
                entry.external_attr = (stat.S_IFLNK | 0o777) << 16
                target.writestr(entry, os.readlink(path).encode("utf-8"))
            else:
                target.write(path, name)
    temporary.replace(output)


if __name__ == "__main__":
    archive(Path(sys.argv[1]), Path(sys.argv[2]))
