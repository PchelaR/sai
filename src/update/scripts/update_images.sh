base=$1
base_dockerfile=$2
shift 2

printf '%s' "$base_dockerfile" | base64 -d \
  | docker_build --pull --no-cache -t "$base" --build-arg UID="$(id -u)" -

while [ $# -gt 0 ]; do
  image=$1
  dockerfile=$2
  shift 2
  if [ -n "$(docker images -q "${image%:*}")" ]; then
    echo "==> Rebuilding ${image%:*}"
    printf '%s' "$dockerfile" | base64 -d \
      | docker_build --no-cache -t "$image" --build-arg BASE="$base" -
  fi
done

docker images sai-base --format '{{.Repository}}:{{.Tag}}' \
  | grep -vxF "$base" \
  | xargs -r docker rmi >/dev/null 2>&1 || true
docker image prune -f >/dev/null
docker builder prune -af >/dev/null
