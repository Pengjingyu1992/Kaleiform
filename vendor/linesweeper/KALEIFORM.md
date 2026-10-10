# Local patch

Source: https://github.com/jneem/linesweeper, crates.io linesweeper 0.4.0.
Original crates.io checksum: `9c19728333c060c6569a53c9a0e56c4be0df52cb4e6e07a8fbe16084cecce769`.
Copyright Joe Neeman and contributors, MIT OR Apache-2.0 (license texts retained).

Only the Rust library source and its license/readme are vendored. The manifest is reduced to
library dependencies; source formatting follows the workspace. Kaleiform adds an opt-in thread-scoped `budget` API and checkpoints in
curve subdivision, sweep events/scans, and topology construction. Unscoped calls keep the
upstream behavior. Callers discard stopped partial topologies immediately.

When updating, rebase these hooks and run cancellation/stress tests as well as geometry CI.
