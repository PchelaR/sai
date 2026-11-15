root=$1
shift
case "$root" in
  "$HOME"/projects/*/*) ;;
  *) echo "refusing to remove files outside $HOME/projects: $root" >&2; exit 1 ;;
esac
for relative in "$@"; do
  case "/$relative/" in
    *"/../"* | *"/./"* | "//"*) echo "refusing to remove $relative" >&2; exit 1 ;;
  esac
  rm -rf -- "${root:?}/$relative"
done
