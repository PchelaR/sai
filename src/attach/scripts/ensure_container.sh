container=$1
workspace=$2
project=$3
image=$4
spec=$5
agent=$6
shift 6

mounts=()
state_dirs=()
for mount in "$@"; do
  mounts+=(-v "$mount:ro")
  target=${mount#*:}
  case "$target" in
    /state/*) state_dirs+=("$(dirname "$target")") ;;
  esac
done

image_id=$(docker image inspect -f '{{ .Id }}' "$image")

current=$(docker container inspect -f '{{ .Image }} {{ index .Config.Labels "sai.spec" }}' "$container" 2>/dev/null || true)
if [ -n "$current" ] && [ "$current" != "$image_id $spec" ]; then
  if docker exec "$container" tmux ls >/dev/null 2>&1; then
    echo "Agent container was updated, it will be applied after all agent sessions end"
  else
    echo "==> Updating agent container"
    docker rm -f "$container" >/dev/null
    current=
  fi
fi

if [ -n "$current" ]; then
  docker start "$container" >/dev/null
else
  docker run --rm -v "sai-state-$agent:/state" "$image" mkdir -p "${state_dirs[@]}"
  mem_kb=$(awk '/^MemTotal/ { print $2 }' /proc/meminfo)
  docker run -d --init \
    --name "$container" \
    --hostname "${container#sai-}" \
    --label "sai.project=$project" \
    --label "sai.spec=$spec" \
    --label "sai.agent=$agent" \
    --network sai-net \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --memory "$(( mem_kb * 3 / 4 ))k" \
    --cpus "$(nproc)" \
    --pids-limit 512 \
    --restart unless-stopped \
    -v "$workspace:/workspace" \
    -v "sai-state-$agent:/state" \
    "${mounts[@]}" \
    -e CARGO_HOME=/state/cargo \
    -e PIP_CACHE_DIR=/state/pip \
    "$image" >/dev/null
fi

docker images "${image%:*}" --format '{{.Repository}}:{{.Tag}}' \
  | grep -vxF "$image" \
  | xargs -r docker rmi >/dev/null 2>&1 || true
docker image prune -f >/dev/null 2>&1 || true
