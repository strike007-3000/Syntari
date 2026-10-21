#!/usr/bin/env bash
# Lists the network sockets opened by the running Syntari process. Run while a session is active.
# Expect only local (127.0.0.1) sockets after models are cached; anything else is Foundry runtime behaviour to review.
set -euo pipefail
pids=$(pgrep -x syntari || pgrep -f "target/.*/syntari" || true)
[ -n "$pids" ] || { echo "Syntari is not running"; exit 1; }
lsof -nP -i -a -p "$(echo "$pids" | paste -sd, -)"
