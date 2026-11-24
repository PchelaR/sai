image=curlimages/curl:8.11.1
host_ip=$(ip -4 route get 1.1.1.1 | awk '{ for (i = 1; i < NF; i++) if ($i == "src") print $(i + 1) }')
# The address of the VPS on the agents' bridge. Newer Docker leaves the gateway out of the
# network's settings when it was not given, so it is read from the bridge itself
gateway_ip=$(ip -4 -o addr show br-sai | awk '{ sub(/\/.*/, "", $4); print $4; exit }')
[ -n "$host_ip" ] || fail "cannot find the address of this VPS"
[ -n "$gateway_ip" ] || fail "cannot find the address of the agents' network on this VPS"
docker pull -q "$image" >/dev/null
result=0
docker run --rm -i --network sai-net --entrypoint sh "$image" -s "$SAI_PORT" "$host_ip" "$gateway_ip" <<'CHECK' || result=$?
port=$1
shift
curl -sS -o /dev/null --max-time 15 https://api.anthropic.com || { echo "containers have no internet access" >&2; exit 1; }
for ip in "$@"; do
  code=0
  curl -s -o /dev/null --connect-timeout 3 "http://$ip:$port" || code=$?
  [ "$code" -eq 28 ] || { echo "containers can reach the host at $ip" >&2; exit 1; }
done
CHECK
docker image rm "$image" >/dev/null
exit "$result"
