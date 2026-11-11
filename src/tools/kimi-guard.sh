#!/bin/sh
input=$(cat)
sep='(^|[;&|({`]|\$\(|\\n|"command"[[:space:]]*:[[:space:]]*")'
prefix='[[:space:]]*(([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*|sudo|env|command|exec|nohup|time|nice|builtin|xargs)[[:space:]]+(-[^[:space:]]*[[:space:]]+)*)*'
end='([[:space:];&|)`"\\]|$)'
delete='([^[:space:];&|"]*/)?(rm|rmdir|unlink|shred)'
git='git([[:space:]]+[^[:space:];&|"]+)*[[:space:]]+(clean|commit|push)'
find="(-exec|-execdir|-ok|-okdir)[[:space:]]+([^[:space:]]*/)?(rm|rmdir|unlink|shred)$end|[[:space:]]-delete$end"
if printf '%s' "$input" | grep -Eq -e "$sep$prefix($delete|$git)$end" -e "$find"; then
  echo "sai: deleting files, git clean, git commit and git push are not allowed" >&2
  exit 2
fi
