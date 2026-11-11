if command -v cloud-init >/dev/null; then
  cloud-init status --wait >/dev/null || true
fi
apt_get update
apt_get install rsync curl ca-certificates
apt_get clean
