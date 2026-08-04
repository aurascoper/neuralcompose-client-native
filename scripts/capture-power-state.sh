#!/usr/bin/env bash
# Capture GPU/CPU power state for an evidence directory.
#
# Why this exists: the Radeon 890M power-gates between benchmark samples, so a
# measurement's power source, governor and thermal state change the number by
# more than the thing usually being measured. Two prior results were distorted
# by it — a battery-vs-AC prompt-throughput pair that disagreed with no recorded
# reason, and a reducer benchmark whose headline ratio was mostly clock-ramp
# artifact. Both were unresolvable afterwards because the state was not captured.
#
# Redirect into the run's evidence directory and hash it with the rest:
#   scripts/capture-power-state.sh > docs/hardware/<topic>.evidence/power-state.txt
#
# ponytail: reads sysfs directly, no rocm-smi/upower dependency. If a field is
# needed that only a vendor tool reports, add it here rather than switching.

set -u

say() { printf '%s\n' "$*"; }
read_or() { [ -r "$1" ] && cat "$1" 2>/dev/null || printf 'UNREADABLE\n'; }

say "# power-state capture"
say "captured_at: $(date -Is)"
say "host: $(uname -n)"
say "kernel: $(uname -r)"
say ""

say "## power supply"
for ps in /sys/class/power_supply/*/; do
  [ -d "$ps" ] || continue
  name=$(basename "$ps")
  say "$name.type: $(read_or "$ps/type")"
  [ -r "$ps/online" ] && say "$name.online: $(read_or "$ps/online")"
  [ -r "$ps/status" ] && say "$name.status: $(read_or "$ps/status")"
  [ -r "$ps/capacity" ] && say "$name.capacity: $(read_or "$ps/capacity")"
done
say ""

say "## cpu"
say "governor: $(read_or /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)"
say "driver: $(read_or /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver)"
[ -r /sys/firmware/acpi/platform_profile ] &&
  say "platform_profile: $(read_or /sys/firmware/acpi/platform_profile)"
say ""

say "## gpu"
for dev in /sys/class/drm/card*/device; do
  [ -d "$dev" ] || continue
  # Skip non-GPU DRM nodes: no dpm knob means nothing to report here.
  [ -r "$dev/power_dpm_force_performance_level" ] || continue
  card=$(basename "$(dirname "$dev")")
  say "$card.dpm_force_performance_level: $(read_or "$dev/power_dpm_force_performance_level")"
  say "$card.dpm_state: $(read_or "$dev/power_dpm_state")"
  for hw in "$dev"/hwmon/hwmon*/; do
    [ -d "$hw" ] || continue
    # Millidegrees C and microwatts, as the kernel reports them. Not converted
    # here: a capture records what was read, and conversion is a place to be wrong.
    [ -r "$hw/temp1_input" ] && say "$card.temp1_input_millicelsius: $(read_or "$hw/temp1_input")"
    [ -r "$hw/power1_average" ] && say "$card.power1_average_microwatts: $(read_or "$hw/power1_average")"
    [ -r "$hw/freq1_input" ] && say "$card.freq1_input_hz: $(read_or "$hw/freq1_input")"
  done
done
