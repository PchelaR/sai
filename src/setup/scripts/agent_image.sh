docker image inspect "$SAI_IMAGE" >/dev/null 2>&1 && exit 0
printf '%s' "$SAI_DOCKERFILE" | base64 -d \
  | docker build -q -t "$SAI_IMAGE" --build-arg UID="$(id -u "$SAI_USER")" - >/dev/null
docker builder prune -af >/dev/null
