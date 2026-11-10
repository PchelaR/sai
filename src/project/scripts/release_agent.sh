container=$1
agent=$2
current=$(docker container inspect -f '{{ index .Config.Labels "sai.agent" }}' "$container" 2>/dev/null || true)
if [ -z "$current" ] || [ "$current" = "$agent" ]; then
  exit 0
fi
if docker exec "$container" tmux ls >/dev/null 2>&1; then
  echo "$current is still running in ${container#sai-}, exit it with /exit first" >&2
  exit 1
fi
docker rm -f "$container" >/dev/null
