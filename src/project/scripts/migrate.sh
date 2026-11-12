mode=$1
container=$2
workspace=$3
snapshots=$4
folder=$5

nest() {
  local dir=$1
  [ -d "$dir" ] || return 0
  [ -e "$dir/$folder" ] && [ ! -e "$dir/.sai-nest" ] && [ -z "$(find "$dir" -mindepth 1 -maxdepth 1 ! -name "$folder" -print -quit)" ] && return 0
  mkdir -p "$dir/.sai-nest"
  find "$dir" -mindepth 1 -maxdepth 1 ! -name .sai-nest -exec mv -t "$dir/.sai-nest" {} +
  mv -T "$dir/.sai-nest" "$dir/$folder"
}

case "$mode" in
  check)
    if docker exec "$container" tmux ls >/dev/null 2>&1; then
      echo "an agent is still running in ${container#sai-}, exit it with /exit and run sai again to finish the upgrade" >&2
      exit 1
    fi
    docker container inspect -f '{{ index .Config.Labels "sai.agent" }}' "$container" 2>/dev/null || true
    ;;
  move)
    docker rm -f "$container" >/dev/null 2>&1 || true
    nest "$workspace"
    for stamp in "$snapshots"/*/; do
      if [ -d "$stamp" ]; then
        nest "${stamp%/}"
      fi
    done
    ;;
esac
