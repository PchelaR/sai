folder=$1
case "$folder/" in
  */../* | */./* | *//*) echo "refusing to remove $folder" >&2; exit 1 ;;
esac
case "$folder" in
  "$HOME"/projects/*/*) ;;
  *) echo "refusing to remove $folder" >&2; exit 1 ;;
esac
rm -rf -- "$folder"
