// Both fresh runs must expose a successful JIT counter; absent counters are not zero.
export function matchesReference(got, ref) {
  return got.panics === 0 && ref.panics === 0 && got.jitFailures === 0 && ref.jitFailures === 0 &&
    ['insns', 'consoleSha256', 'frames', 'framesSha256'].every(field => got[field] !== undefined && got[field] === ref[field]);
}
