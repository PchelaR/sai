# Services of a project (MariaDB, Redis, RabbitMQ) run in the network of its agent
# container: the agent reaches them on localhost, nothing else does
mode=$1
project=$2
shift 2

case "$project" in
  "" | *[!a-z0-9-]*) echo "invalid project $project" >&2; exit 1 ;;
esac
agent="sai-$project"
dir="$HOME/agents/$project/services"
fields=11

containers() {
  docker ps -aq --filter "label=sai.service-of=$project"
}

running() {
  [ "$(docker container inspect -f '{{ .State.Running }}' "$1" 2>/dev/null || true)" = true ]
}

remember_volume() {
  mkdir -p "$dir"
  touch "$dir/volumes"
  grep -qxF "$1" "$dir/volumes" || printf '%s\n' "$1" >> "$dir/volumes"
}

wait_for() {
  local name=$1 tries=90
  shift
  until docker exec "$name" "$@" >/dev/null 2>&1; do
    tries=$((tries - 1))
    if [ "$tries" -le 0 ] || ! running "$name"; then
      echo "warning: $name did not start, its log:"
      docker logs --tail 30 "$name" 2>&1 || true
      return 1
    fi
    sleep 2
  done
}

create() {
  local name=$1 kind=$2 image=$3 volume=$4 user=$5 password=$6 root=$7 first=$8 spec=$9
  local memory agent_id common
  if ! docker image inspect "$image" >/dev/null 2>&1; then
    echo "==> Downloading $image"
    docker pull -q "$image" >/dev/null
  fi
  memory=$(memory_service "$kind")
  agent_id=$(docker container inspect -f '{{ .Id }}' "$agent")
  common=(
    -d --name "$name"
    --network "container:$agent"
    --restart unless-stopped
    --security-opt no-new-privileges
    --cap-drop NET_RAW
    --pids-limit 512
    --memory "${memory}k"
    --memory-swap "$(memory_swap "$memory")k"
    --label "sai.service-of=$project"
    --label "sai.spec=$spec"
    --label "sai.agent-id=$agent_id"
    -v "$volume:$(data_dir "$kind")"
  )
  case "$kind" in
    mariadb)
      docker run "${common[@]}" \
        -e MARIADB_ROOT_PASSWORD="$root" -e MARIADB_USER="$user" \
        -e MARIADB_PASSWORD="$password" -e MARIADB_DATABASE="$first" \
        "$image" --character-set-server=utf8mb4 --collation-server=utf8mb4_unicode_ci >/dev/null
      ;;
    redis)
      local args=(redis-server --appendonly yes)
      if [ -n "$password" ]; then
        args+=(--requirepass "$password")
      fi
      docker run "${common[@]}" "$image" "${args[@]}" >/dev/null
      ;;
    rabbitmq)
      docker run "${common[@]}" \
        -e RABBITMQ_NODENAME=rabbit@localhost \
        -e RABBITMQ_DEFAULT_USER="$user" -e RABBITMQ_DEFAULT_PASS="$password" \
        -e RABBITMQ_DEFAULT_VHOST="$first" \
        "$image" >/dev/null
      ;;
  esac
}

data_dir() {
  case "$1" in
    mariadb) echo /var/lib/mysql ;;
    redis) echo /data ;;
    rabbitmq) echo /var/lib/rabbitmq ;;
  esac
}

# Until the service answers: the agent should find it working as soon as the chat opens
ready() {
  local name=$1 kind=$2
  case "$kind" in
    # Over TCP: while the image creates the database on first start, it runs a server
    # without network, and setting up users at that moment breaks its own setup
    mariadb) wait_for "$name" sh -c 'mariadb-admin ping -h 127.0.0.1 --protocol=tcp --silent || mysqladmin ping -h 127.0.0.1 --protocol=tcp --silent' ;;
    rabbitmq) wait_for "$name" rabbitmq-diagnostics -q check_running ;;
    redis) wait_for "$name" sh -c 'redis-cli ping 2>&1 | grep -qE "PONG|NOAUTH"' ;;
  esac
}

# Users, databases and vhosts are set up on every change, so editing them in sai works
# on a database that already has data
configure() {
  local name=$1 kind=$2 user=$3 password=$4 root=$5 names=$6 sql item
  case "$kind" in
    mariadb)
      ready "$name" "$kind" || return 1
      sql="CREATE USER IF NOT EXISTS '$user'@'%' IDENTIFIED BY '$password'; ALTER USER '$user'@'%' IDENTIFIED BY '$password';"
      for item in ${names//,/ }; do
        sql+=" CREATE DATABASE IF NOT EXISTS \`$item\` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
        sql+=" GRANT ALL PRIVILEGES ON \`$item\`.* TO '$user'@'%';"
      done
      sql+=" FLUSH PRIVILEGES;"
      local tries=10
      until docker exec -e MYSQL_PWD="$root" "$name" \
        sh -c 'if command -v mariadb >/dev/null; then exec mariadb -uroot -e "$1"; else exec mysql -uroot -e "$1"; fi' sh "$sql"; do
        tries=$((tries - 1))
        [ "$tries" -gt 0 ] || return 1
        sleep 3
      done
      ;;
    rabbitmq)
      ready "$name" "$kind" || return 1
      docker exec "$name" rabbitmqctl -q add_user "$user" "$password" >/dev/null 2>&1 \
        || docker exec "$name" rabbitmqctl -q change_password "$user" "$password" >/dev/null
      docker exec "$name" rabbitmqctl -q set_user_tags "$user" administrator >/dev/null
      for item in ${names//,/ }; do
        docker exec "$name" rabbitmqctl -q add_vhost "$item" >/dev/null 2>&1 || true
        docker exec "$name" rabbitmqctl -q set_permissions -p "$item" "$user" '.*' '.*' '.*' >/dev/null
      done
      ;;
  esac
}

# The agent gets what the running services leave. Applied live, so the limit of an agent
# started by an older sai, or before a service was added, follows too
limit_agent() {
  local args=("$@") kinds=() i memory left
  for ((i = 0; i < ${#args[@]} / fields; i++)); do
    if [ "${args[i * fields + 9]}" = yes ]; then
      kinds+=("${args[i * fields + 1]}")
    fi
  done
  memory=$(memory_agent "${kinds[@]}")
  left=$(memory_left "${kinds[@]}")
  if [ "$left" -lt "$memory_agent_minimum" ]; then
    echo "warning: the VPS has $(( $(memory_total) / 1024 )) MB of memory, next to the services of $project the agent gets $(( memory / 1024 )) MB and the VPS may run short. Stop a service you do not need or use a bigger VPS"
  fi
  if ! docker update --memory "${memory}k" --memory-swap "$(memory_swap "$memory")k" "$agent" >/dev/null 2>&1; then
    echo "warning: could not change the memory limit of the agent of $project"
  fi
}

ensure() {
  if ! running "$agent"; then
    echo "the agent of $project is not running, services start with the next \`sai chat $project\`"
    return 0
  fi
  local wanted=() agent_id agent_started
  agent_id=$(docker container inspect -f '{{ .Id }}' "$agent")
  agent_started=$(docker container inspect -f '{{ .State.StartedAt }}' "$agent")
  local args=("$@")
  local count=$(( ${#args[@]} / fields )) i
  for ((i = 0; i < count; i++)); do
    wanted+=("${args[i * fields]}")
  done
  for id in $(containers); do
    local found
    found=$(docker container inspect -f '{{ .Name }}' "$id")
    found=${found#/}
    if [[ " ${wanted[*]} " != *" $found "* ]]; then
      docker rm -f "$id" >/dev/null
    fi
  done
  mkdir -p "$dir"
  limit_agent "${args[@]}"
  for ((i = 0; i < count; i++)); do
    local name=${args[i * fields]} kind=${args[i * fields + 1]} image=${args[i * fields + 2]}
    local prefix=${args[i * fields + 3]} volume=${args[i * fields + 4]} user=${args[i * fields + 5]}
    local password=${args[i * fields + 6]} root=${args[i * fields + 7]} names=${args[i * fields + 8]}
    local enabled=${args[i * fields + 9]} spec=${args[i * fields + 10]}
    remember_volume "$prefix"
    if [ "$enabled" != yes ]; then
      if running "$name"; then
        docker stop "$name" >/dev/null
      fi
      continue
    fi
    local current fresh=
    current=$(docker container inspect -f '{{ index .Config.Labels "sai.spec" }} {{ index .Config.Labels "sai.agent-id" }}' "$name" 2>/dev/null || true)
    if [ "$current" != "$spec $agent_id" ]; then
      docker rm -f "$name" >/dev/null 2>&1 || true
      echo "==> Starting $name"
      # A service that fails to start must not keep the agent from starting
      if ! create "$name" "$kind" "$image" "$volume" "$user" "$password" "$root" "${names%%,*}" "$spec"; then
        echo "warning: could not start $name, it is retried on the next \`sai chat $project\`"
        continue
      fi
      fresh=1
    else
      local started=
      if ! running "$name"; then
        echo "==> Starting $name"
        docker start "$name" >/dev/null
        started=1
      elif [[ "$(docker container inspect -f '{{ .State.StartedAt }}' "$name")" < "$agent_started" ]]; then
        # The agent container restarted after the service, which then lost its network
        echo "==> Restarting $name"
        docker restart "$name" >/dev/null
        started=1
      fi
      # Started before was not waited for: the chat opened while the database still booted
      if [ -n "$started" ] && ! ready "$name" "$kind"; then
        echo "warning: $name does not answer yet, the agent may have to wait for it"
      fi
    fi
    local setup
    setup=$(printf '%s\n' "$kind" "$user" "$password" "$names" | sha256sum | cut -d ' ' -f 1)
    if [ -n "$fresh" ] || [ "$(cat "$dir/$name.setup" 2>/dev/null || true)" != "$setup" ]; then
      if configure "$name" "$kind" "$user" "$password" "$root" "$names"; then
        printf '%s\n' "$setup" > "$dir/$name.setup"
      else
        echo "warning: could not set up $name, it is retried on the next \`sai chat $project\`"
      fi
    fi
  done
}

status() {
  local id
  for id in $(containers); do
    docker container inspect -f '{{ .Name }}	{{ .State.Status }}' "$id" | sed 's|^/||'
  done
}

remove() {
  local name=$1 prefix=$2 found
  case "$name" in
    "sai-$project-"*) ;;
    *) echo "refusing to remove $name" >&2; exit 1 ;;
  esac
  case "$prefix" in
    sai-svc-*) ;;
    *) echo "refusing to remove volumes $prefix" >&2; exit 1 ;;
  esac
  docker rm -f "$name" >/dev/null 2>&1 || true
  rm -f "$dir/$name.setup"
  for found in $(docker volume ls -q --filter "name=^$prefix"); do
    case "$found" in
      "$prefix" | "$prefix"-*) docker volume rm "$found" >/dev/null ;;
    esac
  done
  if [ -f "$dir/volumes" ]; then
    grep -vxF "$prefix" "$dir/volumes" > "$dir/volumes.tmp" || true
    mv "$dir/volumes.tmp" "$dir/volumes"
  fi
}

purge() {
  local id prefix found
  for id in $(containers); do
    docker rm -f "$id" >/dev/null
  done
  if [ -f "$dir/volumes" ]; then
    while IFS= read -r prefix; do
      case "$prefix" in
        sai-svc-*) ;;
        *) continue ;;
      esac
      for found in $(docker volume ls -q --filter "name=^$prefix"); do
        case "$found" in
          "$prefix" | "$prefix"-*) docker volume rm "$found" >/dev/null 2>&1 || true ;;
        esac
      done
    done < "$dir/volumes"
  fi
}

case "$mode" in
  ensure) ensure "$@" ;;
  status) status ;;
  remove) remove "$@" ;;
  restart)
    case "$1" in
      "sai-$project-"*) docker restart "$1" >/dev/null ;;
      *) echo "refusing to restart $1" >&2; exit 1 ;;
    esac
    ;;
  purge) purge ;;
  *) echo "usage: services ensure|status|restart|remove|purge <project>" >&2; exit 1 ;;
esac
