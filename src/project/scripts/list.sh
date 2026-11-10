docker ps -a --filter label=sai.path --format '{{.Names}}	{{.Label "sai.agent"}}	{{.State}}	{{.Label "sai.path"}}'
