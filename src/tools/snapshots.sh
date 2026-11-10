#!/bin/bash
set -euo pipefail

tools=$(dirname "$(dirname "$(readlink -f "$0")")")
projects=$HOME/projects
snapshots=$HOME/snapshots
excludes=$tools/snapshot-excludes
recent_seconds=86400
recent_limit=48
daily_seconds=604800
min_free_kb=2097152

fail() {
  echo "$*" >&2
  exit 1
}

stamps() {
  ls -1 "$snapshots/$1" 2>/dev/null | grep -v '\.partial$' | sort || true
}

epoch() {
  date -u -d "${1:0:4}-${1:4:2}-${1:6:2} ${1:9:2}:${1:11:2}:${1:13:2}" +%s
}

filters() {
  filter_args=(--exclude-from="$excludes")
  if [ -f "$projects/$1/.saiignore" ]; then
    filter_args+=(--exclude-from="$projects/$1/.saiignore")
  fi
}

take() {
  local name=$1
  local source=$projects/$name
  local root=$snapshots/$name
  [ -d "$source" ] || return 0
  mkdir -p "$root"
  filters "$name"
  local previous
  previous=$(stamps "$name" | tail -n 1)
  if [ -n "$previous" ] && [ -z "$(rsync -a --dry-run --itemize-changes --delete "${filter_args[@]}" "$source/" "$root/$previous/")" ]; then
    return 0
  fi
  local stamp
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  [ -e "$root/$stamp" ] && return 0
  rm -rf "$root/$stamp.partial"
  rsync -a --delete "${filter_args[@]}" ${previous:+--link-dest="$root/$previous"} "$source/" "$root/$stamp.partial/"
  mv "$root/$stamp.partial" "$root/$stamp"
  prune "$name"
  free_space
}

prune() {
  local root=$snapshots/$1
  local now recent=0 last_day= stamp age day
  now=$(date -u +%s)
  for stamp in $(stamps "$1" | sort -r); do
    age=$((now - $(epoch "$stamp")))
    day=${stamp:0:8}
    if [ "$age" -lt "$recent_seconds" ] && [ "$recent" -lt "$recent_limit" ]; then
      recent=$((recent + 1))
    elif [ "$age" -lt "$daily_seconds" ] && [ "$day" != "$last_day" ]; then
      last_day=$day
    else
      rm -rf "${root:?}/$stamp"
    fi
  done
}

free_space() {
  local oldest root
  while [ "$(df --output=avail -k "$HOME" | tail -n 1)" -lt "$min_free_kb" ]; do
    oldest=$(
      for root in "$snapshots"/*/; do
        stamps "$(basename "$root")" | head -n -1 | sed "s|^|$root|"
      done | awk -F/ '{ print $NF " " $0 }' | sort | head -n 1 | cut -d' ' -f2
    )
    [ -n "$oldest" ] || return 0
    rm -rf "$oldest"
  done
}

list() {
  local stamp found=
  for stamp in $(stamps "$1" | sort -r); do
    printf '%s  %s\n' "$stamp" "$(date -u -d "@$(epoch "$stamp")" '+%Y-%m-%d %H:%M:%S UTC')"
    found=1
  done
  [ -n "$found" ] || echo "No snapshots yet"
}

restore() {
  local name=$1 stamp=$2 path=${3:-}
  local source=$snapshots/$name/$stamp
  local target=$projects/$name
  if [[ "$stamp" == *.partial ]] || [ ! -d "$source" ]; then
    fail "snapshot $stamp not found, see \`sai snapshots\`"
  fi
  take "$name"
  filters "$name"
  if [ -z "$path" ]; then
    rsync -a --delete "${filter_args[@]}" "$source/" "$target/"
  elif [ -d "$source/$path" ]; then
    mkdir -p "$target/$path"
    rsync -a --delete "${filter_args[@]}" "$source/$path/" "$target/$path/"
  elif [ -e "$source/$path" ]; then
    mkdir -p "$(dirname "$target/$path")"
    rsync -a "$source/$path" "$target/$path"
  else
    fail "$path is not in snapshot $stamp"
  fi
}

mkdir -p "$snapshots"
exec 9>"$tools/snapshots.lock"
flock 9

case "${1:-}" in
  take) take "$2" ;;
  take-all)
    for dir in "$projects"/*/; do
      if [ -d "$dir" ]; then
        take "$(basename "$dir")"
      fi
    done
    ;;
  list) list "$2" ;;
  restore) restore "$2" "$3" "${4:-}" ;;
  *) fail "usage: snapshots take|take-all|list|restore" ;;
esac
