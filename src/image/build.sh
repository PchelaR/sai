docker_build() {
  local log status progress=()
  if docker buildx version >/dev/null 2>&1; then
    progress=(--progress=plain)
  fi
  log=$(mktemp)
  set +e
  docker build "${progress[@]}" "$@" 2>&1 | tee "$log" | awk '
    function show(line) {
      if (length(line) > 100) line = substr(line, 1, 97) "..."
      print line
      fflush()
    }
    /^#[0-9]+ \[[0-9]+\/[0-9]+\] / && !seen[$0]++ {
      sub(/^#[0-9]+ /, "  ")
      show($0)
    }
    /^Step [0-9]+\/[0-9]+ : / {
      sub(/^Step /, "  [")
      sub(/ : /, "] ")
      show($0)
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
