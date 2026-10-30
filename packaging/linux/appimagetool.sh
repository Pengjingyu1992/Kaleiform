# shellcheck shell=bash
# Official stable release assets, cross-checked with GitHub's published SHA-256 digests.
APPIMAGETOOL_VERSION=1.9.1

fetch_appimagetool() (
  set -euo pipefail
  tool="$1"
  arch="$2"
  case "$arch" in
    x86_64) digest=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 ;;
    aarch64) digest=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158 ;;
    *) echo "unsupported AppImage tool architecture: $arch" >&2; exit 2 ;;
  esac
  if [ ! -f "$tool" ]; then
    download="$(mktemp "$tool.download.XXXXXX")"
    trap 'rm -f "$download"' EXIT
    curl --fail --location --proto '=https' --tlsv1.2 --output "$download" \
      "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-$arch.AppImage"
    printf '%s  %s\n' "$digest" "$download" | sha256sum --check
    chmod +x "$download"
    /bin/mv -f "$download" "$tool"
  fi
  # A cached tool is also untrusted until its bytes match the pinned release.
  printf '%s  %s\n' "$digest" "$tool" | sha256sum --check
  chmod +x "$tool"
)
