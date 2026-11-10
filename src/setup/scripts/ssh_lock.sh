cat > /etc/ssh/sshd_config.d/00-sai.conf <<CONF
Port $SAI_PORT
PermitRootLogin no
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
AuthenticationMethods publickey
AllowUsers $SAI_USER
MaxAuthTries 3
LoginGraceTime 30
X11Forwarding no
AllowAgentForwarding no
AllowTcpForwarding no
PermitTunnel no
CONF
sshd -t
systemctl restart ssh.service

ssh_ports() { ss -ltnpH | awk '/"sshd"/ { sub(/.*:/, "", $4); print $4 }' | sort -u | xargs; }
for _ in $(seq 10); do
  [ "$(ssh_ports)" = "$SAI_PORT" ] && break
  sleep 1
done
[ "$(ssh_ports)" = "$SAI_PORT" ] || fail "sshd listens on unexpected ports: $(ssh_ports)"

offered=$(ssh -p "$SAI_PORT" -o BatchMode=yes -o PreferredAuthentications=none \
  -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "$SAI_USER@127.0.0.1" true 2>&1 || true)
case "$offered" in
  *"Permission denied (publickey)"*) ;;
  *) fail "sshd still offers other authentication methods: $offered" ;;
esac

ufw delete allow 22/tcp >/dev/null
passwd -l root >/dev/null
mkdir -p "$(dirname "$SAI_SETUP_MARKER")"
touch "$SAI_SETUP_MARKER"
