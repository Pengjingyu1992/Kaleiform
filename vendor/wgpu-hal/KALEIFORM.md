# Kaleiform Metal SDR colour-matching patch

Source: wgpu-hal 30.0.1 from crates.io, checksum
`b6b7fb58561a792bc237628ba0792e332de418fefe145f13b5ed8201e6d52f58`.
Upstream: https://github.com/gfx-rs/wgpu . Licence: MIT OR Apache-2.0;
original licence texts and crate metadata are retained.
The crate's standalone `Cargo.lock` is omitted; builds use the workspace's pinned
top-level `Cargo.lock` instead.

The only code change is in `src/metal/surface.rs`: map `SurfaceColorSpace::Srgb`
to `kCGColorSpaceSRGB` instead of nil. Apple documents that a nil CAMetalLayer
colour space disables colour matching:
https://developer.apple.com/documentation/quartzcore/cametallayer/colorspace .

This corrects every surface configuration, including eframe's resize-end
configuration that happens after the last repaint. No HDR or other backend path
is changed. Compare against the pinned upstream crate when updating this patch.
The workspace intentionally excludes this third-party crate from its own lints/tests.
The application runs a real Metal-surface configuration probe via
`cargo run -p vectorcraft --example check_native_color`.
