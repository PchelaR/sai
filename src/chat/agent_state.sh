agent_state() {
  local screen
  if ! screen=$(docker exec "$1" tmux capture-pane -p -t "$2" 2>/dev/null); then
    echo idle
    return 0
  fi
  screen=$(printf '%s\n' "$screen" | grep -v '^[[:space:]]*$' | tail -n 15 || true)
  if printf '%s' "$screen" | grep -qiE 'do you want to|would you like to|allow command'; then
    echo asking
  elif printf '%s' "$screen" | grep -qi 'esc to interrupt'; then
    echo working
  else
    echo waiting
  fi
}
