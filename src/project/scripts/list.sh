docker ps -a --filter label=sai.project --format 'container	{{.Label "sai.project"}}	{{.State}}'
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
