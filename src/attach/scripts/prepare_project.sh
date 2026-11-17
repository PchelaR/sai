marker=$1
container=$2
workspace=$3

if [ ! -f "$marker" ]; then
  echo "the VPS is not fully configured, run \`sai setup\` first" >&2
  exit 1
fi

mkdir -p "$HOME/.sai"
if [ ! -s "$HOME/.sai/id" ]; then
  od -An -N16 -tx1 /dev/urandom | tr -d ' \n' > "$HOME/.sai/id"
fi
printf 'vps\t%s\n' "$(cat "$HOME/.sai/id")"
printf 'now\t%s\n' "$(date -u +%Y%m%dT%H%M%SZ)"
if [ -d "$workspace" ]; then
  printf 'existed\n'
fi
mkdir -p "$workspace"
printf 'agent\t%s\n' "$(docker container inspect -f '{{ index .Config.Labels "sai.agent" }}' "$container" 2>/dev/null || true)"
