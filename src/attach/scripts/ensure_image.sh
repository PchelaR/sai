base=$1
image=$2
dockerfile=$3
agent=$4
base_dockerfile=$5

if ! docker image inspect "$base" >/dev/null 2>&1; then
  echo "==> Building the base image with Python and Rust, this takes 15-30 minutes once"
  printf '%s' "$base_dockerfile" | base64 -d | docker_build -t "$base" --build-arg UID="$(id -u)" -
  docker images "${base%:*}" --format '{{.Repository}}:{{.Tag}}' \
    | grep -vxF "$base" \
    | xargs -r docker rmi >/dev/null 2>&1 || true
fi

if ! docker image inspect "$image" >/dev/null 2>&1; then
  echo "==> Building $agent image, the first time takes 2-5 minutes"
  printf '%s' "$dockerfile" | base64 -d | docker_build -t "$image" --build-arg BASE="$base" -
  docker builder prune -af >/dev/null 2>&1 || true
fi
