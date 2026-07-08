#!/bin/bash
# Remove the w3p-hwm service (and the legacy w3p_hwm one, if present),
# the installed binary and the assets. Idempotent: safe to re-run.
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
else
    systemctl daemon-reload
    systemctl reset-failed w3p-hwm.service w3p_hwm.service 2>/dev/null || true
fi

# Remove the artifacts create_service.sh installs (no-op when absent).
rm -f /usr/local/bin/w3p-hwm
rm -rf /usr/local/share/w3p-hwm
echo "Removed /usr/local/bin/w3p-hwm and /usr/local/share/w3p-hwm (if present)."
