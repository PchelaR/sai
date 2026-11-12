workspace=$1
shift
names=()
for name in "$@"; do
  names+=(-o -name "$name")
done
find "$workspace" -mindepth 1 \( -name .git -prune \) -o \( \( "${names[@]:1}" \) -prune -exec rm -rf {} + \)
