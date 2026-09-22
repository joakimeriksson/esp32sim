# Shared production/test compiler policy for the parent WebAssembly module.
# EX154: validated on the combined runtime with Rust 1.98.1. An explicitly
# supplied RUSTFLAGS (including an empty value) overrides this default.
# -inline-threshold is an internal LLVM option. New stable LLVM versions may
# reject it; fail the build loudly rather than silently change the artifact.
RUSTFLAGS="${RUSTFLAGS--Cllvm-args=-inline-threshold=2000}"; export RUSTFLAGS
# EX120: omit DWARF explicitly, retaining the name section for profiles.
# Sourcing exports this policy into the caller's shell, including later native builds.
# Empty values use defaults. Explicit debug=1/2 keeps DWARF unless strip is set.
CARGO_PROFILE_RELEASE_DEBUG="${CARGO_PROFILE_RELEASE_DEBUG:-0}"; export CARGO_PROFILE_RELEASE_DEBUG
case "$CARGO_PROFILE_RELEASE_DEBUG" in
    0|false|none) CARGO_PROFILE_RELEASE_STRIP="${CARGO_PROFILE_RELEASE_STRIP:-debuginfo}" ;;
    *) CARGO_PROFILE_RELEASE_STRIP="${CARGO_PROFILE_RELEASE_STRIP:-none}" ;;
esac
export CARGO_PROFILE_RELEASE_STRIP
