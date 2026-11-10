base=$1
image=$2
dockerfile=$3
agent=$4

if ! docker image inspect "$base" >/dev/null 2>&1; then
  echo "base image is not built yet, run \`sai update\`" >&2
  exit 1
fi

if ! docker image inspect "$image" >/dev/null 2>&1; then
  echo "==> Building $agent image"
  printf '%s' "$dockerfile" | base64 -d | docker build -q -t "$image" --build-arg BASE="$base" - >/dev/null
  docker builder prune -af >/dev/null 2>&1 || true
fi
