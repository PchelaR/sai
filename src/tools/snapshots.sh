#!/bin/bash
set -euo pipefail

tools=$(dirname "$(dirname "$(readlink -f "$0")")")
projects=$HOME/projects
snapshots=$HOME/snapshots
excludes=$tools/snapshot-excludes
all_seconds=3600
hourly_seconds=86400
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
  filter_args=(--exclude-from="$excludes" --filter=':- .saiignore')
}

take() {
  local name=$1
  local source=$projects/$name
  local root=$snapshots/$name
  [ -d "$source" ] || return 0
  mkdir -p "$root"
  filters "$name"
  local previous changes=
  previous=$(stamps "$name" | tail -n 1)
  if [ -n "$previous" ]; then
    changes=$(rsync -a --dry-run --itemize-changes --delete "${filter_args[@]}" "$source/" "$root/$previous/")
    [ -n "$changes" ] || return 0
  fi
  local stamp
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  [ -e "$root/$stamp" ] && return 0
  rm -rf "$root/$stamp.partial"
  rsync -a --delete "${filter_args[@]}" ${previous:+--link-dest="$root/$previous"} "$source/" "$root/$stamp.partial/"
  mv "$root/$stamp.partial" "$root/$stamp"
  mkdir -p "$root/.meta"
  if [ -n "$previous" ]; then
    summarize <<<"$changes" > "$root/.meta/$stamp"
  else
    echo "first snapshot" > "$root/.meta/$stamp"
  fi
  prune "$name"
  free_space
}

summarize() {
  awk '
    /^\*deleting / { if ($2 !~ /\/$/) deleted++; next }
    /^>f\+\+\+/ { added++; next }
    /^>f/ { changed++ }
    END {
      if (deleted) parts = parts sep "deleted " deleted; if (deleted) sep = ", "
      if (changed) parts = parts sep "changed " changed; if (changed) sep = ", "
      if (added) parts = parts sep "added " added
      print (parts == "" ? "small changes" : parts)
    }'
}

drop() {
  rm -rf "${snapshots:?}/$1/$2" "${snapshots:?}/$1/.meta/$2"
}

prune() {
  local now newest=1 stamp age bucket last_bucket=
  now=$(date -u +%s)
  for stamp in $(stamps "$1" | sort -r); do
    age=$((now - $(epoch "$stamp")))
    if [ -n "$newest" ] || [ "$age" -lt "$all_seconds" ]; then
      newest=
      continue
    elif [ "$age" -lt "$hourly_seconds" ]; then
      bucket=${stamp:0:11}
    elif [ "$age" -lt "$daily_seconds" ]; then
      bucket=${stamp:0:8}
    else
      drop "$1" "$stamp"
      continue
    fi
    if [ "$bucket" = "$last_bucket" ]; then
      drop "$1" "$stamp"
    else
      last_bucket=$bucket
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
    drop "$(basename "$(dirname "$oldest")")" "$(basename "$oldest")"
  done
}

list() {
  local stamp summary
  for stamp in $(stamps "$1" | sort -r); do
    summary=$(cat "$snapshots/$1/.meta/$stamp" 2>/dev/null || true)
    printf '%s\t%s\n' "$stamp" "$summary"
  done
}

restore() {
  local name=$1 stamp=$2 path=$3
  shift 3
  local source=$snapshots/$name/$stamp
  local target=$projects/$name
  local folder
  if [[ "$stamp" == *.partial ]] || [ ! -d "$source" ]; then
    fail "snapshot $stamp not found, see \`sai snapshots\`"
  fi
  take "$name"
  filters "$name"
  if [ -z "$path" ]; then
    for folder in "$@"; do
      if [ -d "$source/$folder" ]; then
        mkdir -p "$target/$folder"
        rsync -a --checksum --delete "${filter_args[@]}" "$source/$folder/" "$target/$folder/"
      else
        echo "$folder is not in snapshot $stamp, left as is"
      fi
    done
  elif [ -d "$source/$path" ]; then
    mkdir -p "$target/$path"
    rsync -a --checksum --delete "${filter_args[@]}" "$source/$path/" "$target/$path/"
  elif [ -e "$source/$path" ]; then
    mkdir -p "$(dirname "$target/$path")"
    rsync -a --checksum "$source/$path" "$target/$path"
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
    for dir in "$snapshots"/*/; do
      if [ -d "$dir" ]; then
        prune "$(basename "$dir")"
      fi
    done
    free_space
    ;;
  list) list "$2" ;;
  restore) shift; restore "$@" ;;
  purge) rm -rf "${snapshots:?}/$2" ;;
  *) fail "usage: snapshots take|take-all|list|restore|purge" ;;
esac
