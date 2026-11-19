mode=$1
old=$2
new=$3

case "$mode" in
  check)
    if docker exec "sai-$old" tmux ls >/dev/null 2>&1; then
      echo "an agent is running in $old, exit it with /exit first" >&2
      exit 1
    fi
    if [ -e "$HOME/projects/$new" ] || [ -e "$HOME/snapshots/$new" ] || [ -e "$HOME/agents/$new" ]; then
      echo "files of a removed project named $new are still on the VPS, delete them with \`sai rm $new --purge\` or pick another name" >&2
      exit 1
    fi
    ;;
  move)
    docker ps -aq --filter "label=sai.service-of=$old" | xargs -r docker rm -f >/dev/null
    docker rm -f "sai-$old" >/dev/null 2>&1 || true
    for root in "$HOME/projects" "$HOME/snapshots" "$HOME/agents"; do
      if [ -d "$root/$old" ]; then
        mv -T "$root/$old" "$root/$new"
      fi
    done
    ;;
esac
