#!/bin/bash
set -euo pipefail

tools=$(dirname "$(dirname "$(readlink -f "$0")")")
projects=$HOME/projects
snapshots=$HOME/snapshots
excludes=$tools/snapshot-excludes
watched_list=$tools/watched-files
all_seconds=3600
hourly_seconds=86400
daily_seconds=604800
min_free_kb=2097152

fail() {
  echo "$*" >&2
  exit 1
}

stamps() {
  local path
  for path in "$snapshots/$1"/*; do
    [ -e "$path" ] || continue
    path=${path##*/}
    [[ $path == *.partial ]] || printf '%s\n' "$path"
  done | sort
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
    watched_changes "$stamp" <<<"$changes" >> "$root/.meta/watched"
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

watched_changes() {
  awk -v stamp="$1" -v list="$watched_list" '
    BEGIN {
      while ((getline pattern < list) > 0) {
        if (pattern ~ /\/$/) dirs[++d] = pattern
        else if (pattern ~ /^\*/) suffixes[++s] = substr(pattern, 2)
        else names[pattern] = 1
      }
    }
    {
      path = substr($0, 13)
      if (path ~ /\/$/) next
      if ($0 ~ /^\*deleting/) kind = "deleted"
      else if ($0 ~ /^>f\+\+\+/) kind = "added"
      else if ($0 ~ /^>f/ || $0 ~ /^cL/) kind = "changed"
      else next
      count = split(path, parts, "/")
      base = parts[count]
      hit = (base in names)
      for (i = 1; !hit && i <= s; i++) {
        hit = length(base) > length(suffixes[i]) && substr(base, length(base) - length(suffixes[i]) + 1) == suffixes[i]
      }
      for (i = 1; !hit && i <= d; i++) {
        hit = index("/" path, "/" dirs[i]) > 0
      }
      if (hit) printf "%s\t%s\t%s\n", stamp, kind, path
    }'
}

forget_uploaded() {
  local name=$2 since=$3 file
  shift 3
  file=$snapshots/$name/.meta/watched
  [ -f "$file" ] || return 0
  awk -F '\t' -v since="$since" -v folders="$*" '
    BEGIN { count = split(folders, list, " ") }
    {
      if ($1 >= since) {
        for (i = 1; i <= count; i++) if (index($3, list[i] "/") == 1) next
      }
      print
    }' "$file" > "$file.tmp"
  mv "$file.tmp" "$file"
}

unreviewed() {
  local file=$snapshots/$1/.meta/watched seen
  [ -f "$file" ] || return 0
  seen=$(cat "$snapshots/$1/.meta/watched.seen" 2>/dev/null || true)
  awk -F '\t' -v seen="$seen" '$1 > seen' "$file"
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

# sai checks what it passes, and this checks again: these end up in rm -rf and rsync --delete
check_name() {
  [[ "$1" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] || fail "invalid project name: $1"
}

check_stamp() {
  [[ "$1" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] || fail "snapshot $1 not found, see \`sai snapshots\`"
}

check_path() {
  case "/$1/" in
    //* | */../* | */./*) fail "path must be relative to the project folder: $1" ;;
  esac
}

case "${1:-}" in
  take | list | watched | reviewed | purge) check_name "${2:-}" ;;
  restore)
    check_name "${2:-}"
    check_stamp "${3:-}"
    for item in "${@:4}"; do
      if [ -n "$item" ]; then
        check_path "$item"
      fi
    done
    ;;
esac

mkdir -p "$snapshots"
exec 9>"$tools/snapshots.lock"
flock 9

case "${1:-}" in
  take)
    take "$2"
    if [ "$#" -gt 3 ]; then
      forget_uploaded "$@"
    fi
    ;;
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
  watched) unreviewed "$2" ;;
  reviewed)
    if [ -f "$snapshots/$2/.meta/watched" ]; then
      tail -n 1 "$snapshots/$2/.meta/watched" | cut -f 1 > "$snapshots/$2/.meta/watched.seen"
    fi
    ;;
  restore) shift; restore "$@" ;;
  purge) rm -rf "${snapshots:?}/$2" ;;
  *) fail "usage: snapshots take|take-all|list|restore|purge" ;;
esac
