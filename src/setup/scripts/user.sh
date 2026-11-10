id "$SAI_USER" >/dev/null 2>&1 || useradd --create-home --shell /bin/bash "$SAI_USER"
home=$(getent passwd "$SAI_USER" | cut -d: -f6)
install -d -m 700 -o "$SAI_USER" -g "$SAI_USER" "$home/.ssh"
printf '%s\n' "$SAI_PUBKEY" > "$home/.ssh/authorized_keys"
chown "$SAI_USER:$SAI_USER" "$home/.ssh/authorized_keys"
chmod 600 "$home/.ssh/authorized_keys"
loginctl enable-linger "$SAI_USER"
