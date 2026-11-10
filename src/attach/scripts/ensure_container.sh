container=$1
workspace=$2
local_path=$3
image=$4
session=$5
spec=$6
policy=$7
policy_target=$8

if ! image_id=$(docker image inspect -f '{{ .Id }}' "$image" 2>/dev/null); then
  echo "agent image is not built yet, run \`sai update\`" >&2
  exit 1
fi

current=$(docker container inspect -f '{{ .Image }} {{ index .Config.Labels "sai.spec" }}' "$container" 2>/dev/null || true)
if [ -n "$current" ] && [ "$current" != "$image_id $spec" ]; then
  if docker exec "$container" tmux has-session -t "$session" 2>/dev/null; then
    echo "Agent container was updated, it will be applied after the current agent session ends"
  else
    echo "==> Updating agent container"
    docker rm -f "$container" >/dev/null
    current=
  fi
fi

if [ -n "$current" ]; then
  docker start "$container" >/dev/null
else
  mem_kb=$(awk '/^MemTotal/ { print $2 }' /proc/meminfo)
  docker run -d --init \
    --name "$container" \
    --hostname "${container#sai-}" \
    --label "sai.path=$local_path" \
    --label "sai.spec=$spec" \
    --network sai-net \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --memory "$(( mem_kb * 3 / 4 ))k" \
    --cpus "$(nproc)" \
    --pids-limit 512 \
    --restart unless-stopped \
    -v "$workspace:/workspace" \
    -v sai-agent-state:/state \
    -v "$policy:$policy_target:ro" \
    -e CARGO_HOME=/state/cargo \
    -e PIP_CACHE_DIR=/state/pip \
    "$image" >/dev/null
fi

docker images "${image%:*}" --format '{{.Repository}}:{{.Tag}}' \
  | grep -vxF "$image" \
  | xargs -r docker rmi >/dev/null 2>&1 || true
docker image prune -f >/dev/null 2>&1 || true
