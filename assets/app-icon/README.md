# Kaleiform logo and app icons

The Kaleiform (虹构) logo is the maintainer-supplied illustration of a smiling red puppy on a
sky-blue rounded tile. The approved artwork is kept on a square 1:1 canvas, including its white
outer border. Platform variants resize the supplied picture without redrawing it.

`kaleiform-1024.png` is the approved square 1:1 master used by the interface, About dialog,
README, and platform icon generators. `packaging/icons.sh` regenerates the 512 px UI texture and
platform variants. macOS uses a separate 1:1 canvas with 9% transparent margins and a rounded tile
so Finder and Dock icons match native icon sizing. Its artwork is resized, not redrawn.
The existing Linux icon ID and platform package IDs remain `ai.storyteller.vectorcraft`
for compatibility; they do not change the displayed name.

| File | Use |
|---|---|
| `kaleiform-1024.png` | Square 1:1 master for all app logo uses |
| `kaleiform-runtime.png` | 512x512 About logo texture |
| `kaleiform-macos-1024.png` | 1024x1024 rounded macOS tile with transparent margins |
| `kaleiform-macos-512.png` | Runtime macOS window and Dock icon |
| `kaleiform.icns` | macOS application bundle |
| `kaleiform.ico` | Windows executable and installer |
| `hicolor/<size>/apps/ai.storyteller.vectorcraft.png` | Linux theme icons |

Run `packaging/icons.sh` on macOS to regenerate the square runtime logo and platform files. It uses `sips`,
`iconutil`, `cargo xtask macos-icon` and `cargo xtask ico`. The logo and icon variants are licensed under MIT OR
Apache-2.0; see `LICENSE.txt`.
