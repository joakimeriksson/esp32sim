#!/usr/bin/env node
// Profiling entry point using the shared firmware loader and runner. Accepts the same
// manifests and environment as wasm-test.mjs, including FW_DIR for local manifests.
// Census mode skips the synthetic handoff test and prints the per-manifest profile.
process.env.CENSUS_STDOUT = '1';
await import('./wasm-test.mjs');
