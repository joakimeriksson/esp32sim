# Exactness results (bin/exact-all.sh)

drift = instruction total vs the base build over the same guest seconds; "queued" = bytes already in queue/jobs.jsonl

| artifact | sha | EXACT | insns | drift vs base | frames | panics | jit failures | queued |
|---|---|---|---:|---:|---:|---:|---:|---|
| blockq-g64 | aeb2b0d71df8 | FAIL | 1700111278 | -16.4383% | 577 | 0 | 0 | no |
| calls-s1 | 09cc6fa1d503 | ok | 2034559010 | +0.0000% | 577 | 0 | 0 | no |
