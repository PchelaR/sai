mode=$1

containers=$( (docker ps -aq --filter label=sai.project; docker ps -aq --filter label=sai.path; docker ps -aq --filter label=sai.service-of) | sort -u)
if [ -n "$containers" ]; then
  docker rm -f $containers >/dev/null
fi

volumes=$( (docker volume ls -q --filter name=^sai-state-; docker volume ls -q --filter name=^sai-svc-) | sort -u)
if [ -n "$volumes" ]; then
  docker volume rm $volumes >/dev/null
fi

images=$(docker images --format '{{.Repository}}:{{.Tag}}' | grep '^sai-' || true)
if [ "$mode" != all ]; then
  images=$(printf '%s\n' "$images" | grep -v '^sai-base:' || true)
fi
if [ -n "$images" ]; then
  docker rmi -f $images >/dev/null
fi

mkdir -p "$HOME/.sai"
exec 9>"$HOME/.sai/snapshots.lock"
flock 9
rm -rf "$HOME/projects" "$HOME/snapshots" "$HOME/agents"

docker image prune -f >/dev/null
docker builder prune -af >/dev/null
