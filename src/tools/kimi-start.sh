#!/bin/sh
set -eu
config="$KIMI_CODE_HOME/config.toml"
rules=/etc/sai/kimi-rules.toml
touch "$config"
updated=$(mktemp)
awk -v rules="$rules" '
function norm(s) { gsub(/[ \t]/, "", s); return s }
function insert() {
  if (inserted) return
  while ((getline line < rules) > 0) print line
  close(rules)
  print ""
  inserted = 1
}
function flush(   i) {
  if (!drop) for (i = 1; i <= n; i++) print buf[i]
  n = 0
  drop = 0
}
BEGIN {
  while ((getline line < rules) > 0)
    if (line ~ /^(pattern|command)[ \t]*=/) ours[norm(line)] = 1
  close(rules)
}
/^# (BEGIN|END) sai$/ { next }
/^[ \t]*\[/ {
  flush()
  insert()
  table = norm($0)
  buf[++n] = $0
  next
}
{
  if (!n) { print; next }
  buf[++n] = $0
  if ((table == "[[permission.rules]]" || table == "[[hooks]]") && (norm($0) in ours)) drop = 1
}
END { flush(); insert() }
' "$config" > "$updated"
cat "$updated" > "$config"
rm -f "$updated"
exec kimi "$@"
