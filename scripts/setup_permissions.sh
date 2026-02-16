#!/bin/bash
set -e

echo "🔒 Configuring Safe Permissions for Auto-Brightness..."

# 1. Ensure Video Group Exists
if ! getent group video > /dev/null; then
    echo "Creating 'video' group..."
    sudo groupadd -f video
fi

# 2. Add Current User to Video Group
if ! groups $USER | grep &>/dev/null '\bvideo\b'; then
    echo "Adding user $USER to 'video' group..."
    sudo usermod -aG video $USER
    echo "⚠️  You must log out and log back in for group changes to take effect!"
else
    echo "✅ User $USER is already in 'video' group."
fi

# 3. Install Udev Rules
echo "Installing Udev rules..."
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
UDEV_FILE="$PROJECT_ROOT/udev/90-backlight.rules"

if [ ! -f "$UDEV_FILE" ]; then
    echo "❌ Error: Could not find udev rules at $UDEV_FILE"
    exit 1
fi

sudo cp "$UDEV_FILE" /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=backlight --action=change

echo "✅ Safe permissions applied. Backlight devices should now be writable by the 'video' group."
echo "👉 If it still doesn't work, try rebooting."
