set -euo pipefail
exec </dev/null
export DEBIAN_FRONTEND=noninteractive
apt_get() { apt-get -yq -o DPkg::Lock::Timeout=300 -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold "$@"; }
fail() { echo "$*" >&2; exit 1; }

