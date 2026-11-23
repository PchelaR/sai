container=$1
workspace=$2
mode=$3
# Only a project folder and its own container: a mistake here deletes on the VPS
name=${workspace#"$HOME"/projects/}
case "$name" in
  "" | "$workspace" | */* | . | ..) echo "refusing to remove $workspace" >&2; exit 1 ;;
esac
if [ "$container" != "sai-$name" ]; then
  echo "refusing to remove $container with $workspace" >&2
  exit 1
fi
if docker container inspect "$container" >/dev/null 2>&1 || [ -d "$workspace" ]; then
  echo found
fi
docker ps -aq --filter "label=sai.service-of=${container#sai-}" | xargs -r docker rm -f >/dev/null
if docker container inspect "$container" >/dev/null 2>&1; then
  docker rm -f "$container" >/dev/null
fi
if [ "$mode" = purge ]; then
  rm -rf -- "$workspace" "$HOME/agents/$name"
fi
