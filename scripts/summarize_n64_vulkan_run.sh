#!/usr/bin/env bash
set -euo pipefail

LOG_FILE="${1:-recentRun.log}"
if [[ ! -f "$LOG_FILE" ]]; then
  echo "missing log file: $LOG_FILE" >&2
  exit 1
fi

TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

# Strip ANSI escape sequences so regexes are stable.
perl -pe 's/\e\[[0-9;]*[A-Za-z]//g' "$LOG_FILE" > "$TMP_FILE"

count_rg() {
  local pattern="$1"
  rg -c -- "$pattern" "$TMP_FILE" 2>/dev/null || echo 0
}

last_number_after_equals() {
  local pattern="$1"
  local value
  value="$(rg -o -- "$pattern" "$TMP_FILE" | tail -n 1 | sed -E 's/.*=([0-9]+).*/\1/' || true)"
  if [[ -n "$value" ]]; then
    echo "$value"
  else
    echo 0
  fi
}

dynarec_count="$(count_rg 'Starting R4300 emulator: Dynamic Recompiler')"
cached_count="$(count_rg 'Starting R4300 emulator: Cached Interpreter')"

queue_present_success_logs="$(count_rg 'queue_present completed')"
queue_present_fail_logs="$(count_rg 'failed presenting Vulkan swapchain image')"
queue_present_attempts_est="$((queue_present_success_logs + queue_present_fail_logs))"
queue_present_attempts_metrics="$(last_number_after_equals 'queue_present_attempts=[0-9]+')"
queue_present_success_metrics="$(last_number_after_equals 'queue_present_successes=[0-9]+')"

external_present_deliveries_metrics="$(last_number_after_equals 'external_present_deliveries=[0-9]+')"
external_present_perf_max="$(rg -o 'external_present=[0-9]+' "$TMP_FILE" | sed -E 's/.*=([0-9]+).*/\1/' | sort -n | tail -n 1 || true)"
external_present_perf_max="${external_present_perf_max:-0}"
vulkan_metrics_final_logs="$(count_rg 'vulkan_present_metrics.*phase=\"?final\"?|phase=\"?final\"?.*vulkan_present_metrics')"
video_refresh_raw_logs="$(count_rg 'handoff trace: retro_video_refresh raw')"
video_refresh_hw_logs="$(count_rg 'handoff trace: retro_video_refresh hw idx=')"
video_refresh_hw_tiny_logs="$(count_rg 'handoff trace: retro_video_refresh hw tiny')"
non_tiny_source_frame_seen="no"
if rg -q 'non_tiny_source_frame_seen=true' "$TMP_FILE"; then
  non_tiny_source_frame_seen="yes"
fi
max_consecutive_tiny_source_frames="$(last_number_after_equals 'max_consecutive_tiny_source_frames=[0-9]+')"

fallback_1x1_count="$(count_rg 'pending=1x1')"

source_non_black_seen="no"
if rg -q 'source image readback .*non_black_samples=([1-9][0-9]*)/64|source_non_black_seen=true' "$TMP_FILE"; then
  source_non_black_seen="yes"
fi

swapchain_non_black_seen="no"
if rg -q 'present readback .*non_black_samples=([1-9][0-9]*)/64|swapchain_non_black_seen=true' "$TMP_FILE"; then
  swapchain_non_black_seen="yes"
fi

scanout_total="$(count_rg '\[PARALLEL_SCANOUT\]')"
scanout_has_image="$(count_rg '\[PARALLEL_SCANOUT\].*has_image=1')"
scanout_non_tiny="$(awk '
  /\[PARALLEL_SCANOUT\]/ {
    if (match($0, /width=[0-9]+/)) {
      w = substr($0, RSTART + 6, RLENGTH - 6) + 0
    } else {
      w = 0
    }
    if (match($0, /height=[0-9]+/)) {
      h = substr($0, RSTART + 7, RLENGTH - 7) + 0
    } else {
      h = 0
    }
    if (w > 1 && h > 1) {
      c++
    }
  }
  END { print c + 0 }
' "$TMP_FILE")"
scanout_reuse="$(count_rg '\[PARALLEL_SCANOUT_REUSE\]')"
scanout_null_total="$(count_rg '\[PARALLEL_SCANOUT_NULL\]')"
scanout_repair_total="$(count_rg '\[PARALLEL_SCANOUT_REPAIR\]')"

null_reasons="$(awk '
  /\[PARALLEL_SCANOUT_NULL\]/ {
    reason = "unknown"
    if (match($0, /reason=[^ ]+/)) {
      reason = substr($0, RSTART + 7, RLENGTH - 7)
    }
    count[reason]++
  }
  END {
    for (k in count) {
      printf "%s=%d\n", k, count[k]
    }
  }
' "$TMP_FILE" | sort)"

printf '\nN64 Vulkan Dynarec Summary (%s)\n' "$LOG_FILE"
printf '  dynarec_starts: %s\n' "$dynarec_count"
printf '  cached_interpreter_starts: %s\n' "$cached_count"
printf '  queue_present_success_logs: %s\n' "$queue_present_success_logs"
printf '  queue_present_fail_logs: %s\n' "$queue_present_fail_logs"
printf '  queue_present_attempts_est: %s\n' "$queue_present_attempts_est"
printf '  queue_present_attempts_metrics_last: %s\n' "$queue_present_attempts_metrics"
printf '  queue_present_success_metrics_last: %s\n' "$queue_present_success_metrics"
printf '  external_present_deliveries_metrics_last: %s\n' "$external_present_deliveries_metrics"
printf '  external_present_perf_max: %s\n' "$external_present_perf_max"
printf '  vulkan_metrics_final_logs: %s\n' "$vulkan_metrics_final_logs"
printf '  video_refresh_raw_logs: %s\n' "$video_refresh_raw_logs"
printf '  video_refresh_hw_logs: %s\n' "$video_refresh_hw_logs"
printf '  video_refresh_hw_tiny_logs: %s\n' "$video_refresh_hw_tiny_logs"
printf '  fallback_pending_1x1_count: %s\n' "$fallback_1x1_count"
printf '  source_non_black_seen: %s\n' "$source_non_black_seen"
printf '  swapchain_non_black_seen: %s\n' "$swapchain_non_black_seen"
printf '  non_tiny_source_frame_seen: %s\n' "$non_tiny_source_frame_seen"
printf '  max_consecutive_tiny_source_frames_last: %s\n' "$max_consecutive_tiny_source_frames"
printf '  scanout_total: %s\n' "$scanout_total"
printf '  scanout_has_image: %s\n' "$scanout_has_image"
printf '  scanout_non_tiny: %s\n' "$scanout_non_tiny"
printf '  scanout_reuse_last_non_tiny: %s\n' "$scanout_reuse"
printf '  scanout_null_total: %s\n' "$scanout_null_total"
printf '  scanout_repair_total: %s\n' "$scanout_repair_total"

if [[ -n "$null_reasons" ]]; then
  printf '  scanout_null_reasons:\n'
  while IFS= read -r line; do
    printf '    - %s\n' "$line"
  done <<< "$null_reasons"
fi

cat <<'PROFILE'

Standard diagnostic profile for each iteration:
  export ARCADE_MACOS_EXPERIMENTAL_VULKAN=1
  export ARCADE_PARALLEL_N64_CPUCORE=dynamic_recompiler
  export ARCADE_VULKAN_TEST_METRICS=1
  export ARCADE_PARALLEL_RDP_SAFE_DIAG=1
  export ARCADE_PARALLEL_RDP_REPAIR_VI_GEOMETRY=1
  export ARCADE_VULKAN_DISABLE_FAIL_FAST=1
PROFILE
