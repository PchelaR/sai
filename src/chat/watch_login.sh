login_file=$1
container=$2
session=$3
if [ -s "$HOME/agents/.login/$session/$login_file" ]; then
  echo logged-in
  exit 0
fi
docker exec "$container" tmux capture-pane -p -J -t "$session" 2>/dev/null || true
