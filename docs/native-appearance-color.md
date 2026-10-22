# Native macOS appearance and display colour

The macOS preview uses AppKit semantic colours for window chrome, controls, labels,
separators and the area around the canvas. These are resolved in the selected Aqua/Dark Aqua
appearance and converted to sRGB before painting in egui. Installed SF/PingFang fonts remain
runtime system resources. Tool selection uses the system accent with contrasting text;
document tabs use a neutral rounded surface. Narrow-window inspectors have an inset rounded
material surface, while docked inspectors retain the available canvas space. Reduce Transparency
and Reduce Motion continue to be honoured. Other platforms keep their existing theme.

## Display path

The renderer produces encoded sRGB pixels. wgpu 30's Metal SDR configuration clears
`CAMetalLayer.colorspace`. A nil layer colour space bypasses display colour matching and can
make sRGB red/cyan appear too saturated on a wide-gamut monitor. A pinned, vendored wgpu-hal patch explicitly tags SDR Metal output as sRGB
in the surface configuration itself. It covers resize-end configurations that occur after the last
repaint, as well as surface recreation, without changing transfer functions,
pixel values, the monitor profile, or document colour settings.

Apple documents the nil behaviour here:
<https://developer.apple.com/documentation/quartzcore/cametallayer/colorspace>.

## Imported images

PNG and RGB/gray JPEG display decoding honours embedded ICC profiles through the existing
pure-Rust CMS and converts the editing/display proxy to sRGB. Invalid or absent profiles retain the existing sRGB fallback.
The strict JPEG decoder and the separate CMYK ink rendering path remain in use. Alpha is preserved exactly, and
conversion uses bounded batches rather than allocating another full-resolution image.

Original encoded images are unchanged. The previous PNG interchange, saved original resources,
placement metadata and undo behaviour remain in place. This work does not add wide-gamut or
16-bit editing: colours beyond the current sRGB working gamut are mapped to that gamut, while
unmodified PNG transfers still preserve the original profile and bit depth.

## Validation

Generated test inputs are defined in code. No user image, system ICC file or system font is
added to the repository. Unit tests compare P3 RGB samples against independently generated
CoreGraphics relative-colorimetric references, and cover PNG/JPEG decoding, malformed profiles,
alpha, retained originals and a conversion spanning multiple batches. A real Metal-surface
probe checks four reconfigurations with no application repaint. Actual window screenshots
are compared in a common colour space, respecting the screenshots' embedded display profile.

### Local acceptance, 2026-10-22

- `RUST_TEST_THREADS=1 cargo xtask ci`: all 7 gates passed; 4,795 tests passed,
  zero failed, 5 ignored. The macOS release build used the patched backend above.
- Native Metal probe: four real surface configurations retained sRGB without repaint.
- Installed-window validation: the supplied sRGB JPEG remained colour-matched immediately
  after window zoom/restore, without clicking the canvas. Mean absolute RGB channel error
  against a CoreGraphics sRGB reference was 1.152/255 at narrow width and 1.069/255 at wide
  width (before repair: 17.885/255). These measurements respect the screenshot monitor ICC
  profile and sample low-gradient interior pixels; they are not Delta E or a zero-error claim.
- Generated P3 PNG: six opaque samples, one half-alpha sample over white and one zero-alpha
  sample matched CoreGraphics' conversion into the actual monitor ICC space within one
  8-bit channel level. Alpha/original-byte checks also passed in the unit tests.
- Actual Delete/Command-Z removed and restored the image in both installed editors. Earlier
  real-mouse cross-window drag acceptance is retained; this task did not repeat that manual
  acceptance or change the drag protocol.
- Performance: an initial run near build activity exceeded the 400% zoom budget (17.28 ms).
  After builds finished, paired old/new runs at the same reported load passed every budget:
  400% zoom 15.68/15.19 ms, fit render 58.16/57.46 ms. No thresholds were changed.
- Final staged/installed app passed strict deep signing and executable-hash equality checks;
  no literal personal build-home path was found. Original installed bundles are retained for
  rollback outside the checkout. This is a local `native-srgb-local` build, not a public release.
