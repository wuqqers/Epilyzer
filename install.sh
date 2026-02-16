#!/bin/bash
set -e

echo "🌞 Installing Auto-Brightness (Epilepsy-Safe)..."

# Detect OS
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$ID
    OS_LIKE=$ID_LIKE
else
    OS=$(uname -s)
fi

echo "🔍 Detected OS: $PRETTY_NAME"

# 1. Install Dependencies
install_deps() {
    case "$OS" in
        arch|cachyos)
            echo "📦 Installing System Dependencies (Arch/CachyOS)..."
            sudo pacman -S --needed --noconfirm gammastep libadwaita gtk4
            ;;
        debian|ubuntu|pardus|kali|linuxmint)
            echo "📦 Installing System Dependencies (Debian/Pardus)..."
            sudo apt update
            sudo apt install -y gammastep libgtk-4-1 libadwaita-1-0 pkg-config
            ;;
        *)
            if [[ "$OS_LIKE" == *"debian"* ]]; then
                 echo "📦 Installing System Dependencies (Debian-like)..."
                 sudo apt update
                 sudo apt install -y gammastep libgtk-4-1 libadwaita-1-0
            elif command -v pacman &> /dev/null; then
                 sudo pacman -S --needed --noconfirm gammastep
            else
                echo "⚠️  Unknown distribution. Please ensure 'gammastep', 'gtk4', and 'libadwaita' are installed."
            fi
            ;;
    esac
}

install_deps

# 2. Build
echo "📦 Building project..."
if ! command -v cargo &> /dev/null; then
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    else
        echo "❌ Cargo not found. Please install Rust."
        exit 1
    fi
fi
cargo build --release

# 3. Install Binaries
echo "🛑 Stopping running service..."
systemctl --user stop auto-brightness 2>/dev/null || true
pkill -9 -f auto-brightness-daemon || true
killall -9 auto-brightness-daemon 2>/dev/null || true
echo "Waiting for processes to exit..."
sleep 2

echo "📂 Installing binaries..."
mkdir -p ~/.local/bin

# Source names from Cargo target
DAEMON_SRC="target/release/daemon"
CLI_SRC="target/release/cli"
GUI_SRC="target/release/gui"

cp "$DAEMON_SRC" ~/.local/bin/auto-brightness-daemon
cp "$CLI_SRC" ~/.local/bin/auto-brightness
cp "$GUI_SRC" ~/.local/bin/auto-brightness-gui

# 3.5 Desktop Integration
echo "🖥️  Installing Desktop Shortcut..."
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp gui/icon.svg ~/.local/share/icons/hicolor/scalable/apps/auto-brightness.svg

mkdir -p ~/.local/share/applications
cp gui/auto-brightness.desktop ~/.local/share/applications/

# Fix Exec path and Icon path
sed -i "s|Exec=auto-brightness-gui|Exec=$HOME/.local/bin/auto-brightness-gui|g" ~/.local/share/applications/auto-brightness.desktop
sed -i "s|Icon=display-brightness-symbolic|Icon=$HOME/.local/share/icons/hicolor/scalable/apps/auto-brightness.svg|g" ~/.local/share/applications/auto-brightness.desktop

# Update desktop database
update-desktop-database ~/.local/share/applications/ 2>/dev/null || true
gtk-update-icon-cache ~/.local/share/icons/hicolor/ 2>/dev/null || true

# 3.6 Setup Autostart
echo "🚀 Configuring Autostart..."
mkdir -p ~/.config/autostart
cp ~/.local/share/applications/auto-brightness.desktop ~/.config/autostart/

# 4. Setup Config
echo "⚙️ Setting up config..."
if [ ! -d /etc/auto-brightness ]; then
    sudo mkdir -p /etc/auto-brightness
fi

if [ ! -f /etc/auto-brightness/config.toml ]; then
    echo '
[general]
enabled = true
mode = "normal"
log_level = "info"

[location]
method = "auto"
timezone = "Europe/Istanbul"

[epilepsy_protection]
enabled = true
min_transition_time = 2.0
max_changes_per_second = 3.0
smooth_steps = 50
emergency_hotkey = "Ctrl+Alt+B"
safe_mode_brightness = 40.0

[brightness]
method = "backlight"
min_brightness = 15.0
max_brightness = 95.0
default_brightness = 50.0
' | sudo tee /etc/auto-brightness/config.toml > /dev/null
fi

# 5. Setup Systemd
echo "🔧 Setting up Systemd Service..."
mkdir -p ~/.config/systemd/user
cp systemd/auto-brightness.service ~/.config/systemd/user/
sed -i "s|%h|$HOME|g" ~/.config/systemd/user/auto-brightness.service
sed -i "s|ExecStart=.*|ExecStart=$HOME/.local/bin/auto-brightness-daemon|g" ~/.config/systemd/user/auto-brightness.service

systemctl --user daemon-reload
systemctl --user enable --now auto-brightness.service

# 6. Setup Udev
echo "🔒 Setting up Udev rules (sudo required)..."
sudo cp udev/99-backlight.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=backlight --action=change

# 7. Setup Failsafe Permission Service
echo "🛡️  Installing Failsafe Permission Service..."
sudo cp systemd/fix-brightness-permissions.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now fix-brightness-permissions.service

echo "👥 Configuring user groups..."
sudo groupadd -f video || true
sudo groupadd -f i2c || true
sudo usermod -aG video $USER
sudo usermod -aG i2c $USER

echo "✅ Installation Complete on $PRETTY_NAME!"
echo "👉 Use 'auto-brightness --help' to control the system."
echo "👉 Check status with 'systemctl --user status auto-brightness'"
echo "⚠️  Please log out and log back in for group changes to take effect."
