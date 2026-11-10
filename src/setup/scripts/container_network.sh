docker network inspect sai-net >/dev/null 2>&1 || docker network create \
  --driver bridge \
  --subnet 172.30.0.0/16 \
  -o com.docker.network.bridge.name=br-sai \
  -o com.docker.network.bridge.enable_icc=false \
  sai-net >/dev/null

cat > /usr/local/sbin/sai-firewall <<'RULES'
#!/bin/sh
set -e
rule() { iptables -C "$@" 2>/dev/null || iptables -I "$@"; }
rule INPUT -i br-sai -j DROP
for net in 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 169.254.0.0/16 100.64.0.0/10; do
  rule DOCKER-USER -i br-sai -d "$net" -j DROP
done
RULES
chmod 755 /usr/local/sbin/sai-firewall

cat > /etc/systemd/system/sai-firewall.service <<'UNIT'
[Unit]
Description=sai container firewall
Requires=docker.service
After=docker.service ufw.service
PartOf=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/local/sbin/sai-firewall

[Install]
WantedBy=docker.service
UNIT
systemctl daemon-reload
systemctl enable sai-firewall.service >/dev/null 2>&1
systemctl restart sai-firewall.service
