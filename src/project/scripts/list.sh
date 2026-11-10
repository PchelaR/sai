docker ps -a --filter label=sai.path --format '{{.Names}}	{{.State}}	{{.Label "sai.path"}}'
