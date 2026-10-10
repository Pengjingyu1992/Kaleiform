# Offset Path safety

Implementation plan (2026-10-10):

1. Keep the current curve precision. Add cooperative work/cancellation checks to the pinned
   MIT OR Apache-2.0 linesweeper dependency, including the intersection-event loop.
2. Compute offsets from an immutable document/selection snapshot. Apply successful results as
   one command/undo step only while that snapshot is still current.
3. Run desktop offsets on a worker, show a cancellable status, and discard cancelled, stale,
   oversized or failed results. Keep synchronous engine/wasm commands bounded too.
4. Exercise dense synthetic intersections, mid-computation cancellation, rollback, journal
   replay and stale results; run the full quality gates and inspect the installed macOS app.

The earlier interrupted random test's seed/input was not recorded. Its exact trigger remains
unknown; a bounded synthetic stress case is not evidence that every geometric defect is fixed.

## Behavior and limits

- Desktop direct offsets and dialog previews use a worker. The status bar's cancel button and
  `geometry.cancel` signal the same shared budget; closing the dialog cancels its preview.
- Commands from CLI/MCP, actions/batches and wasm stay synchronous, with the same deterministic
  work limit. Wasm has no native wall-clock deadline or desktop worker.
- Per command: 1,000,000 weighted work units and 10 seconds on native targets, shared across
  selected objects. Each source path is capped at 50,000 anchors and generated stroke input at
  100,000 elements. These are safety ceilings, not claims about a particular machine's speed.
- Checks run during sweep events, segment scans, curve subdivision, topology construction,
  contour walks and refitting. These are cooperative limits, not a hard operating-system memory
  limit or a real-time guarantee for every single library call (stroke fitting, for example).
- The original geometric tolerances are unchanged. Errors propagate instead of silently applying
  partial geometry. The engine verifies document identity, revision and selection before commit.
- `KALEIFORM_MODEL_TRACE=/absolute/local/path.json` records the synthetic document, selection,
  command and random operation sequence before model tests enter an offset. It is opt-in and
  test-only; do not publish diagnostic inputs containing private documents.

The local dependency patch is documented in `vendor/linesweeper/KALEIFORM.md`; both license texts
are included in macOS packages. No published release is implied by a local build.

## Automated validation (2026-10-10)

- `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1 cargo xtask ci`: all seven gates passed;
  4,644 tests passed, including 1,494 engine and 834 UI tests. The random command-sequence
  property test passed without fixing its seed.
- Synthetic crossing paths exercise deterministic exhaustion, cancellation after the sweep
  starts, and an expired native deadline. A later ordinary operation still succeeds.
- Engine/UI coverage checks unchanged artwork/history after failure or cancellation, stale
  selection/document rejection, one undo/journal entry, replay, and preview replacement/commit.
  Editing while a preview is pending keeps the new edit.
- An earlier parallel test run hit the existing native-menu cache assertion; that test passed
  in isolation. The complete rerun was serialized because language and plug-in state are shared
  across tests. No menu assertion was removed or weakened.

These checks are not a substitute for native application interaction or a clean-machine
performance comparison. The baseline performance run warned of high system load; its zoom
timing was over budget, so it must not be used to claim an offset speed improvement.
