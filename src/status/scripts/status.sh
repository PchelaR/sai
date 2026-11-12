printf 'cores\t%s\n' "$(nproc)"
printf 'load\t%s\n' "$(cut -d' ' -f1-3 /proc/loadavg)"
free -m | awk '
  /^Mem:/  { printf "memory\t%d / %d MB\n", $3, $2 }
  /^Swap:/ { printf "swap\t%d / %d MB\n", $3, $2 }
'
df -h / | awk 'NR == 2 { printf "disk\t%s / %s (%s)\n", $3, $2, $5 }'

ids=$(docker ps -q --filter label=sai.project)
if [ -n "$ids" ]; then
  docker stats --no-stream --format 'agent	{{.Name}}	{{.CPUPerc}}	{{.MemUsage}}' $ids
fi
