if [ -n "${SAI_KEEP_BUILD_CACHE:-}" ]; then
  install -d -o "$SAI_USER" -g "$SAI_USER" "$(dirname "$SAI_BUILD_CACHE_MARKER")"
  touch "$SAI_BUILD_CACHE_MARKER"
  chown "$SAI_USER:$SAI_USER" "$SAI_BUILD_CACHE_MARKER"
else
  rm -f "$SAI_BUILD_CACHE_MARKER"
fi
docker image inspect "$SAI_IMAGE" >/dev/null 2>&1 && exit 0
printf '%s' "$SAI_DOCKERFILE" | base64 -d \
  | docker_build -t "$SAI_IMAGE" --build-arg UID="$(id -u "$SAI_USER")" -
if [ ! -f "$SAI_BUILD_CACHE_MARKER" ]; then
  docker builder prune -af >/dev/null
fi
