base=$1
image=$2
dockerfile=$3
agent=$4
base_dockerfile=$5
build_cache "${6:-}"

if ! docker image inspect "$base" >/dev/null 2>&1; then
  if [ -n "$(docker images -q "${base%:*}")" ]; then
    echo "==> sai was updated and its base image changed, rebuilding it once, this takes 15-30 minutes. Projects, files and chats stay as they are"
  else
    echo "==> Building the base image with Python and Rust, this takes 15-30 minutes once"
  fi
  printf '%s' "$base_dockerfile" | base64 -d | docker_build -t "$base" --build-arg UID="$(id -u)" -
  docker images "${base%:*}" --format '{{.Repository}}:{{.Tag}}' \
    | grep -vxF "$base" \
    | xargs -r docker rmi >/dev/null 2>&1 || true
fi

if ! docker image inspect "$image" >/dev/null 2>&1; then
  if [ -n "$(docker images -q "${image%:*}")" ]; then
    echo "==> sai was updated and the $agent image changed, rebuilding it once, this takes 2-5 minutes"
  else
    echo "==> Building $agent image, the first time takes 2-5 minutes"
  fi
  printf '%s' "$dockerfile" | base64 -d | docker_build -t "$image" --build-arg BASE="$base" -
  if [ ! -f "$HOME/.sai/keep-build-cache" ]; then
    docker builder prune -af >/dev/null 2>&1 || true
  fi
fi
