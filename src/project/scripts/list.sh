docker ps -a --filter label=sai.project --format '{{.Label "sai.project"}}	{{.State}}'
