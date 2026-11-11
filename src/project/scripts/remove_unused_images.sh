docker images --format '{{.Repository}}:{{.Tag}}' \
  | grep '^sai-' | grep -v '^sai-base:' \
  | xargs -r docker rmi >/dev/null 2>&1 || true
docker image prune -f >/dev/null 2>&1 || true
