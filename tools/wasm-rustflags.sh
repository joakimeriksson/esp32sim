# Shared production/test compiler policy for the parent WebAssembly module.
# EX154: validated on the combined runtime with Rust 1.98.1. An explicitly
# supplied RUSTFLAGS (including an empty value) overrides this default.
# -inline-threshold is an internal LLVM option. New stable LLVM versions may
# reject it; fail the build loudly rather than silently change the artifact.
RUSTFLAGS="${RUSTFLAGS--Cllvm-args=-inline-threshold=4000}"; export RUSTFLAGS
