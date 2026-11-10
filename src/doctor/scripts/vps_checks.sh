base=$1

report() {
  printf '%s\t%s\t%s\n' "$1" "$2" "${3:-}"
}

if docker info >/dev/null 2>&1; then
  report ok "docker"
else
  report fail "docker" "docker is not running"
fi

for service in ssh fail2ban ufw sai-firewall; do
  if systemctl is-active --quiet "$service"; then
    report ok "$service"
  else
    report fail "$service" "service is not running"
  fi
done

if docker network inspect sai-net >/dev/null 2>&1; then
  report ok "container network"
else
  report fail "container network" "sai-net is missing"
fi

if docker image inspect "$base" >/dev/null 2>&1; then
  report ok "base image"
else
  report fail "base image" "not built yet, run \`sai update\`"
fi

if [ -x "$HOME/.sai/bin/snapshots" ]; then
  export XDG_RUNTIME_DIR="/run/user/$(id -u)"
  if systemctl --user is-active --quiet sai-snapshot.timer; then
    report ok "snapshot timer"
  else
    report fail "snapshot timer" "not running, run \`sai chat\` to reinstall it"
  fi
else
  report warn "snapshot timer" "installed on the first \`sai chat\`"
fi

free_kb=$(df --output=avail -k "$HOME" | tail -n 1)
if [ "$free_kb" -ge 2097152 ]; then
  report ok "disk space ($((free_kb / 1048576)) GB free)"
else
  report fail "disk space" "less than 2 GB free, remove unused projects with \`sai rm --purge\` or run \`sai reset\`"
fi
