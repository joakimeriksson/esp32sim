# Profile harness reproduction

Start with `tools/browser-benchmark/` at source revision
`74b87d24ec5c45b5953087b338af33d90c23b26a`, then apply
[harness.patch](harness.patch) from that checkout root with `git apply`.
The patch preserves the two modified capture files; the other 26 files matched
that revision exactly. See the [evidence retention policy](../README.md) for future captures.
