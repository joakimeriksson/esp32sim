#!/bin/bash
# Override M3_HOST with a current IP if mDNS is unavailable; preserve host-key verification.
exec ssh -o BatchMode=yes -o ConnectTimeout=8 "${M3_HOST:?Set M3_HOST to your benchmark SSH target}" "$@"
