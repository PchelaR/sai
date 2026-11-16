if ! command -v docker >/dev/null; then
  . /etc/os-release
  install -m 0755 -d /etc/apt/keyrings
  curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
  chmod a+r /etc/apt/keyrings/docker.asc
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu ${UBUNTU_CODENAME:-$VERSION_CODENAME} stable" \
    > /etc/apt/sources.list.d/docker.list
  apt_get update
  apt_get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin
  apt_get clean
fi
if ! docker buildx version >/dev/null 2>&1; then
  apt_get install docker-buildx-plugin >/dev/null 2>&1 \
    || apt_get install docker-buildx >/dev/null 2>&1 \
    || echo "warning: docker buildx is not available, images are built with the older builder" >&2
fi
mkdir -p /etc/docker
cat > /etc/docker/daemon.json <<'CONF'
{
  "log-driver": "json-file",
  "log-opts": { "max-size": "10m", "max-file": "3" },
  "dns": ["1.1.1.1", "8.8.8.8"]
}
CONF
systemctl enable docker >/dev/null 2>&1
systemctl restart docker
usermod -aG docker "$SAI_USER"
