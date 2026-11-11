apt_get install fail2ban python3-systemd
cat > /etc/fail2ban/jail.d/zz-sai.local <<CONF
[sshd]
enabled = true
port = $SAI_PORT
backend = systemd
maxretry = 3
findtime = 10m
bantime = 1h
banaction = ufw
CONF
systemctl enable fail2ban >/dev/null 2>&1
systemctl restart fail2ban
for _ in $(seq 15); do
  fail2ban-client status sshd >/dev/null 2>&1 && exit 0
  sleep 1
done
fail "fail2ban sshd jail did not start"
