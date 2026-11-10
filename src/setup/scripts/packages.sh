if command -v cloud-init >/dev/null; then
  cloud-init status --wait >/dev/null || true
fi
apt_get update
apt_get upgrade
apt_get install ufw fail2ban python3-systemd unattended-upgrades rsync curl ca-certificates
apt_get clean
cat > /etc/apt/apt.conf.d/20auto-upgrades <<'CONF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
CONF
