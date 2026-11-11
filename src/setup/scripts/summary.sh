row() {
  printf '%s\t%s\n' "$1" "$2"
}

ports=$(ss -ltnpH | awk '/"sshd"/ { sub(/.*:/, "", $4); print $4 }' | sort -un | xargs)
row "user" "$SAI_USER, no sudo, logs in with your ssh key"
row "ssh port" "$ports"

if [ -f /etc/ssh/sshd_config.d/00-sai.conf ] && grep -q '^PasswordAuthentication no' /etc/ssh/sshd_config.d/00-sai.conf; then
  row "ssh login" "only $SAI_USER with the key, passwords and root are disabled"
else
  row "ssh login" "unchanged"
fi

if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
  open=$(ufw status | awk '$2 == "ALLOW" { print $1 }' | sort -u | xargs)
  row "firewall (ufw)" "on, open: ${open:-nothing}, all other incoming traffic is blocked"
else
  row "firewall (ufw)" "not changed by sai"
fi

if fail2ban-client status sshd >/dev/null 2>&1; then
  retries=$(fail2ban-client get sshd maxretry)
  ban=$(fail2ban-client get sshd bantime)
  row "fail2ban" "on, $retries failed logins ban the IP for $((ban / 60)) min"
else
  row "fail2ban" "not changed by sai"
fi

row "docker" "$(docker --version | sed 's/^Docker version //; s/,.*//')"
row "agent network" "sai-net, agents cannot reach this VPS or local networks"
row "swap" "$(free -m | awk '/^Swap:/ { print $2 " MB" }')"
