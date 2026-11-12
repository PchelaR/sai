for git in "$@"; do
  if [ -d "$git" ] && [ ! -f "$git/config" ]; then
    printf '[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n' > "$git/config"
  fi
done
