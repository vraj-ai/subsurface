# Tauri, not Electron

Subsurface's hot path is git: walking history, following renames, blaming
regions, and building an index over a whole repository. That work is CPU- and
IO-bound and wants a native language, so the backend is Rust and the shell is
Tauri.

The cost is a two-language codebase and a smaller ecosystem than Electron's, and
Tauri's webview is the OS webview rather than a pinned Chromium, so rendering
differs per platform. We accept both. Electron would keep everything in
TypeScript but put the indexer in the wrong runtime, and the indexer is the
product.
