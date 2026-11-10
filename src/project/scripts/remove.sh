container=$1
workspace=$2
mode=$3
if docker container inspect "$container" >/dev/null 2>&1 || [ -d "$workspace" ]; then
  echo found
fi
if docker container inspect "$container" >/dev/null 2>&1; then
  docker rm -f "$container" >/dev/null
fi
if [ "$mode" = purge ]; then
  rm -rf "$workspace"
fi
