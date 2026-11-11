docker_build() {
  local log
  log=$(mktemp)
  if ! docker build --progress=plain "$@" >"$log" 2>&1; then
    echo "docker build failed, last lines of its output:" >&2
    tail -n 50 "$log" >&2
    rm -f "$log"
    return 1
  fi
  rm -f "$log"
}
