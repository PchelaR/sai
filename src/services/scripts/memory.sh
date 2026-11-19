# Memory of a project on the VPS, in KiB. The system keeps a share, so ssh and Docker
# answer under any load. Services get fixed shares, the agent gets the rest. Swap is
# capped at half of a limit: a runaway process hits its limit and is killed, instead of
# swapping the whole VPS to a halt
memory_agent_minimum=1048576

memory_total() {
  awk '/^MemTotal/ { print $2 }' /proc/meminfo
}

memory_system() {
  local reserve
  reserve=$(( $(memory_total) * 15 / 100 ))
  if [ "$reserve" -lt 524288 ]; then
    reserve=524288
  fi
  echo "$reserve"
}

memory_service() {
  local share minimum
  case "$1" in
    redis) share=$(( $(memory_total) / 16 )); minimum=65536 ;;
    *) share=$(( $(memory_total) / 8 )); minimum=262144 ;;
  esac
  if [ "$share" -lt "$minimum" ]; then
    share=$minimum
  fi
  echo "$share"
}

# What is left for the agent next to the given services, before the minimum is applied
memory_left() {
  local left kind
  left=$(( $(memory_total) - $(memory_system) ))
  for kind in "$@"; do
    left=$(( left - $(memory_service "$kind") ))
  done
  echo "$left"
}

memory_agent() {
  local left
  left=$(memory_left "$@")
  if [ "$left" -lt "$memory_agent_minimum" ]; then
    left=$memory_agent_minimum
  fi
  echo "$left"
}

# The value of --memory-swap: the limit plus at most half of it in swap
memory_swap() {
  echo $(( $1 + $1 / 2 ))
}
