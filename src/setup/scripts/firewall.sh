ufw default deny incoming >/dev/null
ufw default allow outgoing >/dev/null
ufw allow 22/tcp >/dev/null
ufw allow "$SAI_PORT/tcp" >/dev/null
ufw --force enable >/dev/null
systemctl restart sai-firewall.service
