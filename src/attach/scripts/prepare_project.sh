marker=$1
container=$2
workspace=$3
local_path=$4

if [ ! -f "$marker" ]; then
  echo "the VPS is not fully configured, run \`sai setup\` first" >&2
  exit 1
fi

owner=$(docker container inspect -f '{{ index .Config.Labels "sai.path" }}' "$container" 2>/dev/null || true)
if [ -n "$owner" ] && [ "$owner" != "$local_path" ]; then
  echo "project name ${container#sai-} is already used by $owner" >&2
  exit 1
fi

mkdir -p "$(dirname "$workspace")"
docker container inspect -f '{{ index .Config.Labels "sai.agent" }}' "$container" 2>/dev/null || true
