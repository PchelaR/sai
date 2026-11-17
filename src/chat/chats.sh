command=$1
container=$2
agent=$3
project=$4
shift 4

state_dir="$HOME/agents/$project/$agent"
case "$state_dir" in
  "$HOME"/agents/*/*) ;;
  *) echo "refusing to use $state_dir" >&2; exit 1 ;;
esac

list_chats() {
  if ! command -v python3 >/dev/null; then
    echo "python3 is missing on the VPS, install it as root: apt install python3" >&2
    exit 1
  fi
  python3 -I - "$agent" "$state_dir" <<'PY'
import glob, json, os, sys

agent, root = sys.argv[1], sys.argv[2]

def text_of(content):
    if isinstance(content, str):
        return content
    for part in content if isinstance(content, list) else []:
        if isinstance(part, dict) and part.get("type") in ("text", "input_text"):
            return part.get("text", "")
    return ""

def entries(path):
    with open(path, errors="replace") as lines:
        for line in lines:
            try:
                yield json.loads(line)
            except ValueError:
                continue

def claude():
    for path in glob.glob(os.path.join(root, "projects", "*", "*.jsonl")):
        title = first = None
        for entry in entries(path):
            kind = entry.get("type")
            if kind in ("summary", "custom-title", "ai-title"):
                title = entry.get("customTitle") or entry.get("aiTitle") or entry.get("summary") or title
            elif kind == "user" and first is None and not entry.get("isMeta") and not entry.get("isSidechain"):
                text = text_of(entry.get("message", {}).get("content"))
                if text and not text.startswith("<"):
                    first = text
        yield os.path.basename(path)[:-6], os.path.getmtime(path), title or first

def codex():
    for path in glob.glob(os.path.join(root, "sessions", "**", "*.jsonl"), recursive=True):
        chat = first = None
        for entry in entries(path):
            payload = entry.get("payload") or {}
            if entry.get("type") == "session_meta":
                chat = payload.get("id") or chat
            elif first is None and payload.get("type") == "user_message":
                text = payload.get("message", "")
                if text and not text.startswith("<"):
                    first = text
        if chat:
            yield chat, os.path.getmtime(path), first

chats = claude() if agent == "claude" else codex()
for chat, modified, title in sorted(chats, key=lambda row: -row[1]):
    title = " ".join((title or "empty chat").split())[:80]
    print(f"chat\t{chat}\t{int(modified)}\t{title}")
PY
}

show_chat() {
  local chat=$1
  case "$chat" in
    *[!A-Za-z0-9-]* | "") echo "invalid chat id" >&2; exit 1 ;;
  esac
  python3 -I - "$agent" "$state_dir" "$chat" <<'PY'
import glob, json, os, sys

agent, root, chat = sys.argv[1], sys.argv[2], sys.argv[3]
name = "Claude" if agent == "claude" else "Codex"

def entries(path):
    with open(path, errors="replace") as lines:
        for line in lines:
            try:
                yield json.loads(line)
            except ValueError:
                continue

def parts(content):
    if isinstance(content, str):
        yield "text", content
        return
    for part in content if isinstance(content, list) else []:
        if not isinstance(part, dict):
            continue
        kind = part.get("type")
        if kind in ("text", "input_text", "output_text"):
            yield "text", part.get("text", "")
        elif kind == "tool_use":
            detail = json.dumps(part.get("input", {}), ensure_ascii=False)
            yield "tool", f"{part.get('name', 'tool')} {detail[:160]}"

def say(who, text):
    text = text.strip()
    if text:
        print(f"{who}:")
        for line in text.splitlines():
            print(f"  {line}")
        print()

if agent == "claude":
    paths = glob.glob(os.path.join(root, "projects", "*", chat + ".jsonl"))
    for path in paths[:1]:
        for entry in entries(path):
            if entry.get("isMeta") or entry.get("isSidechain"):
                continue
            kind = entry.get("type")
            if kind not in ("user", "assistant"):
                continue
            for part_kind, text in parts(entry.get("message", {}).get("content")):
                if part_kind == "tool":
                    print(f"  [{text}]")
                elif kind == "user" and not text.startswith("<"):
                    say("You", text)
                elif kind == "assistant":
                    say(name, text)
else:
    for path in glob.glob(os.path.join(root, "sessions", "**", f"*{chat}.jsonl"), recursive=True)[:1]:
        for entry in entries(path):
            payload = entry.get("payload") or {}
            if payload.get("type") == "user_message" and not payload.get("message", "").startswith("<"):
                say("You", payload.get("message", ""))
            elif payload.get("type") == "agent_message":
                say(name, payload.get("message", ""))
PY
}

running() {
  docker exec "$container" tmux has-session -t "$agent" >/dev/null 2>&1
}

stop_if_idle() {
  case "$(agent_state "$container" "$agent")" in
    working)
      echo "the agent is working on a task, wait until it finishes" >&2
      exit 1
      ;;
    asking)
      echo "the agent is waiting for your answer, answer it or let it finish first" >&2
      exit 1
      ;;
  esac
  docker exec "$container" tmux kill-session -t "$agent" >/dev/null 2>&1 || true
}

remove_chat() {
  local chat=$1 newest
  case "$chat" in
    *[!A-Za-z0-9-]* | "") echo "invalid chat id" >&2; exit 1 ;;
  esac
  newest=$(list_chats | head -n 1 | cut -f 2)
  if [ "$chat" = "$newest" ] && running; then
    stop_if_idle
  fi
  if [ "$agent" = claude ]; then
    rm -rf -- "$state_dir"/projects/*/"$chat".jsonl "$state_dir"/projects/*/"$chat" \
      "$state_dir"/todos/"$chat"-*.json "$state_dir/file-history/$chat" "$state_dir/session-env/$chat"
  else
    find "$state_dir/sessions" -name "*$chat.jsonl" -delete 2>/dev/null || true
  fi
  if [ -z "$(list_chats)" ]; then
    rm -rf -- "$state_dir/history.jsonl"
  fi
}

print_state() {
  if running; then
    printf 'state\t%s\n' "$(agent_state "$container" "$agent")"
    printf 'clients\t%s\n' "$(docker exec "$container" tmux list-clients -t "$agent" 2>/dev/null | wc -l)"
  else
    printf 'state\tidle\n'
  fi
}

case "$command" in
  state) print_state ;;
  list)
    print_state
    list_chats
    ;;
  show) show_chat "$1" ;;
  switch) stop_if_idle ;;
  remove) remove_chat "$1" ;;
  *) echo "usage: chats list|switch|remove" >&2; exit 1 ;;
esac
