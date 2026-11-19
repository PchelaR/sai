container=$1
workspace=$2
mode=$3
if docker container inspect "$container" >/dev/null 2>&1 || [ -d "$workspace" ]; then
  echo found
fi
docker ps -aq --filter "label=sai.service-of=${container#sai-}" | xargs -r docker rm -f >/dev/null
if docker container inspect "$container" >/dev/null 2>&1; then
  docker rm -f "$container" >/dev/null
fi
if [ "$mode" = purge ]; then
  rm -rf "$workspace" "$HOME/agents/$(basename "$workspace")"
fi
