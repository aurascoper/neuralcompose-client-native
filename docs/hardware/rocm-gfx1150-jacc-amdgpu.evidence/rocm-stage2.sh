#!/bin/bash
# Stage 2: full rocm metapackage install, logged.
LOG=~aurascoper/outputs/rocm-gfx1150-probes-20260804/apt-stage2-install.txt
apt-get install -y rocm > "$LOG" 2>&1
echo "exit=$? ; log at $LOG ; tail:"
tail -5 "$LOG"
