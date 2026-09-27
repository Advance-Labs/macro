# Runner health line plus a compile-job cap for the 8 vCPU / 16 GiB rust-ci
# profile. A cache-miss `cargo test` that forks one rustc per core swaps that
# machine for hours: the test step goes quiet after the last "Compiling" line,
# then reports a multi-hour `Finished` (observed 232m compile, 4m of tests).
# Check is the same compiler on the same profile, without Postgres.

ci_probe() {
  local mem_kb nproc reserve_kb per_job_kb cargo_jobs
  mem_kb="$(awk '/MemAvailable/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"
  nproc="$(nproc 2>/dev/null || echo 1)"
  # Leave ~2 GiB for Postgres, sccache, and the OS. ~2.5 GiB per rustc.
  reserve_kb=2000000
  per_job_kb=2500000
  if [ "${mem_kb:-0}" -gt "$reserve_kb" ]; then
    cargo_jobs=$(( (mem_kb - reserve_kb) / per_job_kb ))
  else
    cargo_jobs=1
  fi
  if [ "$cargo_jobs" -lt 1 ]; then
    cargo_jobs=1
  fi
  if [ "$cargo_jobs" -gt "$nproc" ]; then
    cargo_jobs="$nproc"
  fi
  export CARGO_BUILD_JOBS="$cargo_jobs"

  if [ -z "${RUSTC_WRAPPER:-}" ] && command -v sccache >/dev/null 2>&1; then
    RUSTC_WRAPPER="$(command -v sccache)"
    export RUSTC_WRAPPER
  fi

  echo "ci-probe nproc=${nproc} cargo_jobs=${CARGO_BUILD_JOBS} mem_available_kb=${mem_kb:-unknown} rustc_wrapper=${RUSTC_WRAPPER:-unset}"
  if command -v sccache >/dev/null 2>&1; then
    sccache --stop-server >/dev/null 2>&1 || true
    sccache --start-server
  fi

  # A frozen runner stops these lines too. A quiet cargo compile keeps printing
  # them once a minute, with load and free memory.
  (
    while true; do
      sleep 60
      echo "ci-heartbeat $(date -u +%H:%M:%S) load=$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo unknown) mem_available_kb=$(awk '/MemAvailable/ {print $2}' /proc/meminfo 2>/dev/null || echo unknown)"
    done
  ) &
  echo $! > /tmp/ci-heartbeat.pid
}

ci_probe_stop() {
  if [ -f /tmp/ci-heartbeat.pid ]; then
    kill "$(cat /tmp/ci-heartbeat.pid)" >/dev/null 2>&1 || true
    rm -f /tmp/ci-heartbeat.pid
  fi
}
