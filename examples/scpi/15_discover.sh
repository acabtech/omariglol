#!/usr/bin/env sh
# 15_discover.sh — find LXI instruments on the network.
#
# Does not need an address: it scans the local subnet and browses mDNS.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

echo "== discover"
"$RIGLOL_BIN" discover --mdns --timeout 3
