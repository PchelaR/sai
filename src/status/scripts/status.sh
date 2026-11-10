echo "VPS"
printf '  cpu      %s core(s), load %s\n' "$(nproc)" "$(cut -d' ' -f1-3 /proc/loadavg)"
free -m | awk '
  /^Mem:/  { printf "  memory   %d / %d MB\n", $3, $2 }
  /^Swap:/ { printf "  swap     %d / %d MB\n", $3, $2 }
'
df -h / | awk 'NR == 2 { printf "  disk     %s / %s (%s)\n", $3, $2, $5 }'

echo
echo "Agents"
ids=$(docker ps -q --filter label=sai.path)
if [ -z "$ids" ]; then
  echo "  none running"
else
  docker stats --no-stream --format '{{.Name}} {{.CPUPerc}} {{.MemUsage}}' $ids \
    | awk '{ name = $1; sub(/^sai-/, "", name); cpu = $2; $1 = $2 = ""; sub(/^ +/, ""); printf "  %-20s cpu %-8s memory %s\n", name, cpu, $0 }'
fi
