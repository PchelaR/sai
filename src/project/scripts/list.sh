docker ps -a --filter label=sai.project --format 'container	{{.Label "sai.project"}}	{{.State}}'
for dir in "$HOME"/projects/*/; do
  if [ -d "$dir" ]; then
    printf 'files\t%s\n' "$(basename "$dir")"
  fi
done
