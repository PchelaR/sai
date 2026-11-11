mkdir -p "$(dirname "$SAI_SETUP_MARKER")"
printf '%s\n' "$SAI_MINIMAL" > "$SAI_SETUP_MARKER"
