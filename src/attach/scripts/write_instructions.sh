file=$1
content=$2
case "$file" in
  "$HOME"/agents/*/instructions.md) ;;
  *) echo "refusing to write $file" >&2; exit 1 ;;
esac
mkdir -p "$(dirname "$file")"
tmp=$(mktemp)
printf '%s' "$content" | base64 -d > "$tmp"
# In place: the running container sees this file through a bind mount of its inode
if ! cmp -s "$tmp" "$file"; then
  cat "$tmp" > "$file"
fi
chmod 644 "$file"
rm -f "$tmp"
