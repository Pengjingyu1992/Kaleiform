#!/usr/bin/env bash
# Regenerate platform icons and the runtime logo from the square Kaleiform master.
#
# Needs: sips and iconutil (both included with macOS). The outputs are committed, so builds do not
# need these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
MASTER="$DIR/kaleiform-1024.png"
MACOS_MASTER="$DIR/kaleiform-macos-1024.png"
RUNTIME_LOGO="$DIR/kaleiform-runtime.png"
ID="ai.storyteller.vectorcraft"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v sips >/dev/null || { echo "error: sips not found (run this script on macOS)" >&2; exit 1; }
command -v iconutil >/dev/null || { echo "error: iconutil not found (run this script on macOS)" >&2; exit 1; }
test -f "$MASTER" || { echo "error: missing $MASTER" >&2; exit 1; }

# Every app-logo use is square; generate its UI texture from the same approved master.
sips -Z 512 "$MASTER" --out "$RUNTIME_LOGO" >/dev/null

render() { sips -s format png -z "$2" "$2" "$1" --out "$3" >/dev/null; }

# Finder and the running app must use the same padded, rounded tile.
(cd "$ROOT" && cargo run -q -p xtask -- macos-icon "$MASTER" "$MACOS_MASTER")
render "$MACOS_MASTER" 512 "$DIR/kaleiform-macos-512.png"

# Linux hicolor theme (the 256 one is also the runtime icon on Windows and Linux).
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$MASTER" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
rm -f "$DIR/hicolor/scalable/apps/$ID.svg"

# Windows .ico.
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$MASTER" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/kaleiform.ico" "${ICO_PNGS[@]}")

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/kaleiform.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MACOS_MASTER" "$s" "$SET/icon_${s}x${s}.png"
    render "$MACOS_MASTER" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/kaleiform.icns" "$SET"
fi
echo "icons written to $DIR"
