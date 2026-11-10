if [ -z "$(swapon --show --noheadings)" ]; then
  ram_mb=$(awk '/^MemTotal/ { print int($2 / 1024) }' /proc/meminfo)
  swap_mb=$(( ram_mb < 2048 ? ram_mb : 2048 ))
  rm -f /swapfile
  fallocate -l "${swap_mb}M" /swapfile
  chmod 600 /swapfile
  mkswap /swapfile >/dev/null
  swapon /swapfile
  grep -q '^/swapfile ' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi
echo 'vm.swappiness=10' > /etc/sysctl.d/99-sai.conf
sysctl -q -p /etc/sysctl.d/99-sai.conf
mkdir -p /etc/systemd/journald.conf.d
printf '[Journal]\nSystemMaxUse=100M\n' > /etc/systemd/journald.conf.d/sai.conf
systemctl restart systemd-journald
