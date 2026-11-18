# Rings the bell of the agent's tmux pane, unless the user is looking at the window.
# tmux passes the bell through ssh to the user's terminal, which flashes the taskbar
# (Windows Terminal) or bounces the dock icon (macOS)
session=$(tmux display-message -p -t "$TMUX_PANE" '#{session_name}' 2>/dev/null) || exit 0
if tmux list-clients -t "$session" -F '#{client_flags}' 2>/dev/null | grep -qw focused; then
  exit 0
fi
tty=$(tmux display-message -p -t "$TMUX_PANE" '#{pane_tty}' 2>/dev/null) && [ -c "$tty" ] && printf '\a' > "$tty"
exit 0
