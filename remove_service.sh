#!/bin/bash
# Remove the w3p-hwm service (and the legacy w3p_hwm one, if present).
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "This script must be run with sudo."
    exit 1
fi

removed=0
for name in w3p-hwm w3p_hwm; do
    service_file="/etc/systemd/system/$name.service"
    if [[ -f "$service_file" ]]; then
        systemctl stop "$name.service" 2>/dev/null || true
        systemctl disable "$name.service" 2>/dev/null || true
        rm -f "$service_file"
        echo "Service $name removed."
        removed=1
    fi
done

if [[ $removed -eq 0 ]]; then
    echo "No w3p-hwm/w3p_hwm service found."
    exit 1
fi

systemctl daemon-reload
