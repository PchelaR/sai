rm -f /etc/ssh/sshd_config.d/00-sai.conf "$SAI_SETUP_MARKER"
ufw allow 22/tcp >/dev/null
usermod -U root 2>/dev/null || true
systemctl restart ssh.service
