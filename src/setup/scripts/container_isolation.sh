image=curlimages/curl:8.11.1
host_ip=$(ip -4 route get 1.1.1.1 | awk '{ for (i = 1; i < NF; i++) if ($i == "src") print $(i + 1) }')
gateway_ip=$(docker network inspect sai-net -f '{{ (index .IPAM.Config 0).Gateway }}')
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
