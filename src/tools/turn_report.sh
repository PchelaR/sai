#!/bin/bash
# What the agent changed in its last answer: files, lines added and removed.
#   start   the user sent a message: remember the project's files
#   report  the agent finished: compare and show the user a summary
# Runs as a hook of the agent, so any trouble ends it quietly instead of failing a turn
set -u
mode=${1:-}
workspace=${SAI_TURN_WORKSPACE:-/workspace}
dir=${SAI_TURN_STATE:-/state/sai-turn}
base=$dir/base
excludes=${SAI_TURN_EXCLUDES:-/opt/sai/excludes}
shown=30

cat >/dev/null 2>&1 || true

mirror() {
  rsync -a --delete --exclude=.git/ --exclude-from="$excludes" "$@" "$workspace/" "$base/"
}

case "$mode" in
  start)
    mkdir -p "$base" || exit 0
    rm -f "$dir/ready"
    if mirror >/dev/null 2>&1; then
      touch "$dir/ready"
    fi
    exit 0
    ;;
  report) ;;
  *) exit 0 ;;
esac

[ -f "$dir/ready" ] || exit 0
changes=$(mirror --dry-run --out-format=$'%i\t%n' 2>/dev/null) || exit 0

files=0
added_total=0
removed_total=0
listed=""
while IFS=$'\t' read -r item path; do
  case "$path" in
    "" | */) continue ;;
  esac
  case "$item" in
    '*deleting'*) kind=deleted; old=$base/$path; new=/dev/null ;;
    '>f+++++++'*) kind=new; old=/dev/null; new=$workspace/$path ;;
    '>f'*) kind=changed; old=$base/$path; new=$workspace/$path ;;
    *) continue ;;
  esac
  stat=$(git diff --no-index --numstat -- "$old" "$new" 2>/dev/null | head -n 1)
  # Only the time or the mode changed: not a change of the content
  [ -n "$stat" ] || continue
  added=${stat%%$'\t'*}
  rest=${stat#*$'\t'}
  removed=${rest%%$'\t'*}
  files=$((files + 1))
  if [ "$added" = - ]; then
    counts="binary"
  else
    added_total=$((added_total + added))
    removed_total=$((removed_total + removed))
    case "$kind" in
      new) counts="+$added" ;;
      deleted) counts="-$removed" ;;
      *) counts="+$added -$removed" ;;
    esac
  fi
  case "$kind" in
    new) counts="new, $counts" ;;
    deleted) counts="deleted, $counts" ;;
  esac
  if [ "$files" -le "$shown" ]; then
    listed+=$'\n'"  $path  $counts"
  fi
done <<< "$changes"

[ "$files" -gt 0 ] || exit 0
if [ "$files" -gt "$shown" ]; then
  listed+=$'\n'"  and $((files - shown)) more"
fi
if [ "$files" -eq 1 ]; then noun=file; else noun=files; fi
text="Changed in this answer: $files $noun, +$added_total -$removed_total$listed"

escape() {
  local value
  value=$(printf '%s' "$1" | tr -d '\000-\010\013\014\016-\037')
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  value=${value//$'\t'/\\t}
  value=${value//$'\r'/\\r}
  value=${value//$'\n'/\\n}
  printf '%s' "$value"
}
printf '{"systemMessage":"%s"}\n' "$(escape "$text")"
