# Chats of sai before 0.1.9 live in one Docker volume per agent, shared by all projects
mode=$1
image=$2
agent=$3
project=$4

volume="sai-state-$agent"
case "$agent" in
  *[!a-z0-9-]*) echo "invalid agent $agent" >&2; exit 1 ;;
esac

count() {
  docker run --rm -v "$1:/old:ro" --entrypoint sh "$image" \
    -c 'find /old/projects /old/sessions -name "*.jsonl" 2>/dev/null | wc -l'
}

case "$mode" in
  list)
    for found in $(docker volume ls -q --filter 'name=^sai-state-'); do
      printf 'volume\t%s\t%s\n' "${found#sai-state-}" "$(count "$found" | tr -d ' ')"
    done
    ;;
  move)
    state="$HOME/agents/$project/$agent"
    case "$project" in
      "" | *[!a-z0-9-]*) echo "invalid project $project" >&2; exit 1 ;;
    esac
    mkdir -p "$state"
    docker run --rm -v "$volume:/old:ro" -v "$state:/new" --entrypoint sh "$image" -c '
      for dir in projects sessions; do
        if [ -d "/old/$dir" ]; then
          mkdir -p "/new/$dir" && cp -R --update=none "/old/$dir/." "/new/$dir/"
        fi
      done'
    docker volume rm "$volume" >/dev/null 2>&1 \
      || echo "the old chats were copied, but $volume could not be removed: docker volume rm $volume" >&2
    ;;
  delete)
    docker volume rm "$volume" >/dev/null
    ;;
  *) echo "usage: old_chats list|move|delete" >&2; exit 1 ;;
esac
