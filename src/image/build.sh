docker_build() {
  local log status
  log=$(mktemp)
  set +e
  docker build --progress=plain "$@" 2>&1 | tee "$log" | awk '
    /^#[0-9]+ \[[0-9]+\/[0-9]+\] / && !seen[$0]++ {
      sub(/^#[0-9]+ /, "  ")
      if (length($0) > 100) $0 = substr($0, 1, 97) "..."
      print
      fflush()
    }'
  status=${PIPESTATUS[0]}
  set -e
  if [ "$status" -ne 0 ]; then
    echo "docker build failed, last lines of its output:" >&2
    tail -n 50 "$log" >&2
    rm -f "$log"
    return 1
  fi
  rm -f "$log"
}
