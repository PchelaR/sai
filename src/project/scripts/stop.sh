container=$1
if ! docker container inspect "$container" >/dev/null 2>&1; then
  echo "no project named ${container#sai-}" >&2
  exit 1
fi
docker ps -q --filter "label=sai.service-of=${container#sai-}" | xargs -r docker stop >/dev/null
docker stop "$container" >/dev/null
