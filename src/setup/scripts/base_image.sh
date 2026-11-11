docker image inspect "$SAI_IMAGE" >/dev/null 2>&1 && exit 0
printf '%s' "$SAI_DOCKERFILE" | base64 -d \
  | docker_build -t "$SAI_IMAGE" --build-arg UID="$(id -u "$SAI_USER")" -
docker builder prune -af >/dev/null
