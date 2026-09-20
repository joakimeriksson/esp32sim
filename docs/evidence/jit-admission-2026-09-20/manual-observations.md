# Manual observations after local adoption

User report on September 20, 2026, after the browser test copies were updated to the [selected admission build](selected-build.json). Browser versions, timing modes and exact workload state were not specified; the loaded artifact was not independently verified on each device. These approximate observations are not controlled benchmark results.

| Machine | Workload | Reported behavior |
|---|---|---|
| M1 Pro | Pocket Tank | Around 50–60% realtime depending on conditions; user's representative estimate is 57%. |
| M3 Pro | Pocket Tank | With `feed 6` and `shadow`, drops to approximately 69% at the low point. After those effects are gone, returns to 77–78%. |
| M1 Pro and M3 Pro | TinyDraw | Reported realtime. Drawing on the display is offset slightly from the cursor; the same issue occurs on M3 Pro. |

The cursor-to-drawing offset is a separate correctness/interaction issue. Its cause has not been diagnosed, and this report does not establish whether it predates the admission change. Preserve it for a separate investigation rather than treating it as a throughput result.

The workload-sensitive Pocket Tank figures suggest including `feed 6` and `shadow` in a future interactive stress scenario. They do not establish an additional gain or regression relative to the paired benchmark campaign.

Linux demo boot was reported at approximately 10% realtime after its missing firmware asset was supplied. Device and browser were not specified in that report; no controlled comparison was performed. It remains a lower-priority workload. iPhone Pocket Tank observations and deferred investigation are recorded in [the separate phone evidence](../iphone-browser-2026-09-20/README.md).
