workspace=$1
shift
names=()
for name in "$@"; do
  names+=(-o -name "$name")
done
find "$workspace" -mindepth 1 \( "${names[@]:1}" \) -prune -exec rm -rf {} +
