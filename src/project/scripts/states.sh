docker ps --filter label=sai.project --format '{{.Label "sai.project"}}	{{.Label "sai.agent"}}' \
  | while IFS=$'\t' read -r project agent; do
      if [ -n "$agent" ]; then
        printf 'container\t%s\t%s\n' "$project" "$(agent_state "sai-$project" "$agent")"
      fi
    done
