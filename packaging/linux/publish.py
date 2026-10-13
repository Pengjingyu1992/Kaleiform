#!/usr/bin/env python3
"""Publish only verified Linux files, retaining every existing release asset."""
import argparse
import hashlib
import json
import re
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath


SUFFIXES = ("tar.gz", "deb", "rpm", "AppImage", "AppImage.zsync")
PRIVATE_DATA = (
    rb"gh[pousr]_[A-Za-z0-9]{30,}",
    rb"github_pat_[A-Za-z0-9_]{50,}",
    rb"AKIA[0-9A-Z]{16}",
    rb"xox[baprs]-[A-Za-z0-9-]{20,}",
    rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
    rb"/workspace/",
    rb"/(?:Users|home)/[A-Za-z0-9_.-]+/",
)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_artifacts(directory, version):
    if not re.fullmatch(r"[0-9A-Za-z.+-]+", version):
        raise ValueError("Invalid version")
    base = f"vectorcraft-{version}-linux-x86_64"
    names = [f"{base}.{suffix}" for suffix in SUFFIXES]
    manifest = f"{base}.sha256"
    hashes = {}
    for line in (directory / manifest).read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
        if not match or match[2] not in names or match[2] in hashes:
            raise ValueError("Invalid Linux checksum manifest")
        hashes[match[2]] = match[1]
    if set(hashes) != set(names):
        raise ValueError("Linux manifest must cover all five package files")
    for name, expected in hashes.items():
        if sha256(directory / name) != expected:
            raise ValueError(f"Checksum mismatch: {name}")
    patterns = [re.compile(pattern) for pattern in PRIVATE_DATA]
    binaries = set()
    with tarfile.open(directory / f"{base}.tar.gz", "r:gz") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            if (path.is_absolute() or ".." in path.parts or not path.parts
                    or path.parts[0] != base):
                raise ValueError("Unsafe path in Linux archive")
            if (any(p in {".git", ".aws", ".ssh", ".env", "TEST-REPORT.md"} for p in path.parts)
                    or path.suffix == ".log"):
                raise ValueError("Local configuration or report present in Linux archive")
            if not member.isfile():
                continue
            with archive.extractfile(member) as stream:
                data = stream.read()
            if any(pattern.search(data) for pattern in patterns):
                # Do not print matched values or user paths.
                raise ValueError("Potential credential or private build path in Linux archive")
            if member.name in {f"{base}/bin/vectorcraft", f"{base}/bin/vectorcraft-cli"}:
                if not data.startswith(b"\x7fELF"):
                    raise ValueError("Expected a Linux ELF executable")
                binaries.add(path.name)
    if binaries != {"vectorcraft", "vectorcraft-cli"}:
        raise ValueError("Missing Linux executables")
    hashes[manifest] = sha256(directory / manifest)
    print("Linux package checksums and privacy scan passed")
    return hashes


def read_release(repo, tag):
    return json.loads(subprocess.check_output(
        ["gh", "api", f"repos/{repo}/releases/tags/{tag}"], text=True))


def pending_assets(release, directory, hashes):
    existing = {asset["name"]: asset for asset in release["assets"]}
    pending = []
    for name, digest in hashes.items():
        asset = existing.get(name)
        if asset is None:
            pending.append(directory / name)
        elif (asset.get("digest") != f"sha256:{digest}"
              or asset["size"] != (directory / name).stat().st_size
              or asset["state"] != "uploaded"):
            raise ValueError(f"Refusing to replace an existing asset: {name}")
    return pending


def verify_retained(before, after):
    current = {asset["id"]: asset for asset in after["assets"]}
    for original in before["assets"]:
        asset = current.get(original["id"])
        if asset is None or any(asset.get(key) != original.get(key)
                                for key in ("name", "size", "digest")):
            raise ValueError("An original release asset changed; investigate before continuing")


def publish(args, hashes):
    if args.tag != f"v{args.version}" or not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo or ""):
        raise ValueError("Release tag must match the package version; specify owner/repository")
    before = read_release(args.repo, args.tag)
    if before["draft"]:
        raise ValueError("This workflow appends Linux assets to an already published release")
    pending = pending_assets(before, args.directory, hashes)
    if args.check_release:
        print(f"Release preflight passed; {len(pending)} Linux assets can be appended")
        return
    if not re.fullmatch(r"[0-9a-f]{40}", args.source_sha or ""):
        raise ValueError("Publication requires the full build source commit")
    if pending:
        # No --clobber: concurrent additions cannot overwrite any existing asset.
        subprocess.run(["gh", "release", "upload", args.tag, "--repo", args.repo,
                        "--", *(str(path) for path in pending)], check=True)
    after = read_release(args.repo, args.tag)
    verify_retained(before, after)
    assets = {asset["name"]: asset for asset in after["assets"]}
    for name, digest in hashes.items():
        asset = assets.get(name)
        if asset is None or asset["state"] != "uploaded" or asset["size"] != (args.directory / name).stat().st_size:
            raise ValueError(f"Uploaded asset is missing or incomplete: {name}")
        if asset.get("digest"):
            if asset["digest"] != f"sha256:{digest}":
                raise ValueError(f"Remote SHA-256 mismatch: {name}")
        else:
            with tempfile.TemporaryDirectory() as temp:
                subprocess.run(["gh", "release", "download", args.tag, "--repo", args.repo,
                                "--pattern", name, "--dir", temp], check=True)
                if sha256(Path(temp) / name) != digest:
                    raise ValueError(f"Downloaded SHA-256 mismatch: {name}")
    marker = "<!-- kaleiform-linux-x86_64 -->"
    if marker not in (after.get("body") or ""):
        base = f"vectorcraft-{args.version}-linux-x86_64"
        section = (
            f"{marker}\n## Linux x86_64 补充发布\n\n"
            "本 Release 已追加 Linux 预览包；下方原始发布说明记录 macOS 版的范围。\n"
            "原 macOS 文件、源码包、BUILD-INFO.json 和 SHA256SUMS.txt 均保留。\n\n"
            f"- Debian/Ubuntu 选择 `{base}.deb`；Fedora 系选择 `.rpm`；其他发行版可用 `.AppImage` 或 `.tar.gz`。\n"
            "- 架构为 x86_64，最低 glibc 2.35（Ubuntu 22.04 及兼容的新发行版）。\n"
            f"- Linux 校验清单为 `{base}.sha256`，与原 Mac 校验清单分开。\n"
            f"- Linux 来源：[完整构建提交](https://github.com/{args.repo}/commit/{args.source_sha})；"
            f"[构建、安装运行、AppImage 和隐私检查]({args.run_url})通过。\n"
            f"- [隐私审查](https://github.com/{args.repo}/blob/{args.source_sha}/docs/privacy-linux-release-2026-10-13.md)："
            "未检出凭据、私人构建路径或用户文件；无新增遥测与作品自动上传。\n"
            "- 修复 JPEG 打开/显示问题；大文档性能仍有已知缺口，RPM 安装、ARM64、真实显卡/输入法尚未完整验收。\n\n---\n\n"
        )
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as body:
            json.dump({"body": section + (after.get("body") or "")}, body, ensure_ascii=False)
            body.flush()
            subprocess.run(["gh", "api", "--method", "PATCH",
                            f"repos/{args.repo}/releases/{after['id']}",
                            "--input", body.name, "--silent"], check=True)
    final = read_release(args.repo, args.tag)
    verify_retained(before, final)
    print(f"Published {len(hashes)} verified Linux assets; all {len(before['assets'])} original assets retained")
    print(final["html_url"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--check-files", action="store_true", help="Local checks only; no network or writes")
    parser.add_argument("--check-release", action="store_true", help="Read-only release preflight")
    parser.add_argument("--repo")
    parser.add_argument("--tag")
    parser.add_argument("--source-sha")
    parser.add_argument("--run-url", default="")
    args = parser.parse_args()
    hashes = verify_artifacts(args.directory, args.version)
    if not args.check_files:
        publish(args, hashes)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        raise SystemExit(f"Linux publication stopped: {exc}") from exc
