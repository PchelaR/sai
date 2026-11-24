changed=
while [ $# -gt 0 ]; do
  path=$1
  mode=$2
  content=$3
  shift 3
  mkdir -p "$(dirname "$path")"
  tmp=$(mktemp)
  printf '%s' "$content" | base64 -d > "$tmp"
  if cmp -s "$tmp" "$path"; then
    rm -f "$tmp"
    continue
  fi
  if [ -e "$path" ]; then
    cat "$tmp" > "$path"
    chmod "$mode" "$path"
  else
    install -m "$mode" "$tmp" "$path"
  fi
  rm -f "$tmp"
  changed=1
done

XDG_RUNTIME_DIR="/run/user/$(id -u)"
export XDG_RUNTIME_DIR
if [ -n "$changed" ]; then
  systemctl --user daemon-reload
  systemctl --user restart sai-snapshot.timer >/dev/null 2>&1 || true
fi
if ! systemctl --user is-enabled --quiet sai-snapshot.timer; then
  systemctl --user enable --now sai-snapshot.timer >/dev/null 2>&1
fi
