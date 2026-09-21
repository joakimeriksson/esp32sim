# Benchmark evidence retention

Keep enough evidence in the source tree to review a claim and reproduce its workload.
Commit a README describing the result and its limits, compact result/comparison JSON,
input and source hashes, environment details, and small reproduction scripts or summaries.
Keep individual evidence files below 50 KB; split results by run when useful, rather
than splitting raw captures merely to fit this limit.

Save raw event streams, per-block rows, CPU profiles, full console logs, screenshots
and videos under an ignored output directory such as `target/`. When a claim needs
those captures, attach them to a release in the contributor's fork or retain them in
a separate evidence repository. Link the artifact and record its SHA-256, byte size,
source revision and reproduction command in the committed summary. Verify access
before removing the only available copy. Test fixtures required by automated tests
belong with the tests and are outside this evidence policy.

Results should retain timings, pass/fail outcomes, instruction counts, run order and
measurement limitations. Remove embedded logs and frame/event arrays from compact
results; identify omitted fields and link the complete original. Screenshots can
support visual claims without being committed to the source repository.

Existing captures already pushed in the browser JIT stack remain accessible at the
immutable revisions listed in the [capture archive](ARCHIVE.md). This is a transition
for existing evidence, not the storage location for future captures. No captures have
been uploaded to a release by this cleanup. Removing files reduces the current tree;
it does not reclaim their bytes from existing Git history.

## Privacy and curation

Retain negative results in [the experiment catalog](../experiments.md). Keep only
material needed to reproduce or challenge the conclusion.

Before committing a capture:

- Replace personal home-directory labels with `/Users/alice` or `/home/alice`.
- Remove personal hostnames, logins, email addresses, device identifiers and
  session details. Harnesses must accept remote targets from the caller.
- Remove unrelated application names, process inventories and command lines.
  Preserve load averages or anonymous CPU samples if they explain noise. Label
  partial process samples as partial; their sum is not total machine utilization.
- Inspect compressed profiles, structured records, screenshots and binary
  metadata as well as plain text. Retain a sanitized summary instead when the
  raw capture adds no reproducibility value.
- Record what was removed and whether that limits the conclusion. For existing
  receipts, retain a manifest of original and sanitized file hashes. Historical
  artifact hashes still identify the original measured artifacts; do not silently
  replace them with hashes of edited receipts.

Run `node tools/check-evidence-privacy.mjs` from the repository root. CI runs the
same check on tracked text and gzip evidence. It detects configured patterns and
does not establish that arbitrary text, images or binaries contain no personal data.
Manual review remains necessary. Keep private raw captures outside Git only when
there is a specific need for them.

[September 21 redactions](privacy-review-2026-09-21/redactions.json) map the existing
capture bytes to their sanitized replacements.
