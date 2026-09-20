#!/bin/bash
# Override M3_HOST with a current IP if mDNS is unavailable; preserve host-key verification.
exec ssh -o BatchMode=yes -o ConnectTimeout=8 -o HostKeyAlias=alice-m3p.local "${M3_HOST:-alice@alice-m3p.local}" "$@"
