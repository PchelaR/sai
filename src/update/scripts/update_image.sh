image=$1
dockerfile=$2
printf '%s' "$dockerfile" | base64 -d | docker build -q --pull --no-cache -t "$image" --build-arg UID="$(id -u)" - >/dev/null
docker image prune -f >/dev/null
docker builder prune -af >/dev/null
