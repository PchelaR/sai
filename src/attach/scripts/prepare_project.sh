marker=$1
container=$2
workspace=$3

if [ ! -f "$marker" ]; then
  echo "the VPS is not fully configured, run \`sai setup\` first" >&2
  exit 1
fi

mkdir -p "$workspace"
docker container inspect -f '{{ index .Config.Labels "sai.agent" }}' "$container" 2>/dev/null || true
