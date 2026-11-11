#!/bin/sh
set -eu
config="$KIMI_CODE_HOME/config.toml"
rules=/etc/sai/kimi-rules.toml
touch "$config"
kept=$(mktemp)
awk '/^# BEGIN sai$/ { skip = 1; next } /^# END sai$/ { skip = 0; next } !skip' "$config" > "$kept"
{
  cat "$kept"
  echo "# BEGIN sai"
  cat "$rules"
  echo "# END sai"
} > "$config"
rm -f "$kept"
exec kimi "$@"
