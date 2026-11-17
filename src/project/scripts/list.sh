mkdir -p "$HOME/.sai"
if [ ! -s "$HOME/.sai/id" ]; then
  od -An -N16 -tx1 /dev/urandom | tr -d ' \n' > "$HOME/.sai/id"
fi
printf 'vps\t%s\n' "$(cat "$HOME/.sai/id")"
docker ps -a --filter label=sai.project --format '{{.Label "sai.project"}}	{{.State}}	{{.Label "sai.agent"}}' \
  | while IFS=$'\t' read -r project state agent; do
      if [ "$state" = running ] && [ -n "$agent" ]; then
        state=$(agent_state "sai-$project" "$agent")
      fi
      printf 'container\t%s\t%s\n' "$project" "$state"
    done
for file in "$HOME"/snapshots/*/.meta/watched; do
  if [ -f "$file" ]; then
    meta=$(dirname "$file")
    seen=$(cat "$meta/watched.seen" 2>/dev/null || true)
    count=$(awk -F '\t' -v seen="$seen" '$1 > seen { print $3 }' "$file" | sort -u | wc -l)
    if [ "$count" -gt 0 ]; then
      printf 'review\t%s\t%s\n' "$(basename "$(dirname "$meta")")" "$count"
    fi
  fi
done
for dir in "$HOME"/projects/*/; do
  if [ -d "$dir" ]; then
    printf 'files\t%s\n' "$(basename "$dir")"
  fi
done
