. /etc/os-release
[ "${ID:-}" = ubuntu ] || fail "unsupported OS: ${PRETTY_NAME:-unknown}, only Ubuntu is supported"
[ "${VERSION_ID%%.*}" -ge 22 ] || fail "unsupported Ubuntu version: $VERSION_ID, 22.04 or newer is required"
