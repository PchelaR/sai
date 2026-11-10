grep -qE '^\s*Include\s+/etc/ssh/sshd_config\.d/\*\.conf' /etc/ssh/sshd_config \
  || fail "/etc/ssh/sshd_config does not include /etc/ssh/sshd_config.d/*.conf"
for file in /etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf; do
  if [ -f "$file" ] && [ "$file" != /etc/ssh/sshd_config.d/00-sai.conf ]; then
    sed -i -E 's/^\s*Port\s/#&/' "$file"
  fi
done
printf 'Port 22\nPort %s\n' "$SAI_PORT" > /etc/ssh/sshd_config.d/00-sai.conf
mkdir -p /run/sshd
sshd -t

if systemctl is-enabled --quiet ssh.socket 2>/dev/null; then
  systemctl disable --now ssh.socket >/dev/null 2>&1
  rm -f /etc/systemd/system/ssh.service.d/00-socket.conf /etc/systemd/system/ssh.socket.d/addresses.conf
  systemctl daemon-reload
fi
systemctl enable ssh.service >/dev/null 2>&1
systemctl restart ssh.service

for _ in $(seq 10); do
  [ -n "$(ss -ltnH "sport = :$SAI_PORT")" ] && exit 0
  sleep 1
done
fail "sshd is not listening on port $SAI_PORT"
