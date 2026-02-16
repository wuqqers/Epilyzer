# <img src="https://raw.githubusercontent.com/FortAwesome/Font-Awesome/6.x/svgs/solid/shield-halved.svg" width="32" height="32"> Epilyzer

### **Accessibility-First, High-Performance Auto-Brightness for Linux**

<p align="left">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Built%20with-Rust-orange?style=for-the-badge&logo=rust" alt="Built with Rust"></a>
  <a href="https://www.w3.org/WAI/standards-guidelines/wcag/"><img src="https://img.shields.io/badge/Accessibility-WCAG--Compliant-green?style=for-the-badge" alt="WCAG Compliant"></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow?style=for-the-badge" alt="License: MIT"></a>
</p>

Traditional auto-brightness tools focus on convenience. **Epilyzer** focuses on **safety**. 

Built entirely in **Rust**, it is a high-performance daemon specifically engineered to prevent photosensitive epilepsy triggers while providing the smoothest ambient light adjustment possible on the Linux desktop.

---

## 🛡️ The Core Mission: Photo-Safety

### 🚫 Anti-Flicker & Epilepsy Guard
Unlike standard tools that "jump" between brightness levels, our custom-built **Epilepsy Guard** enforces a strict maximum change frequency (3Hz). By using sinusoidal easing, we ensure your screen never flickers or flashes—keeping light transitions well within safe neurological limits.

### ⚡ Instant Flashbang Protection
A sudden white screen in a dark room isn't just annoying; it can be a trigger. Our content analyzer monitors your display buffer in real-time and preemptively dims the backlight at a hardware level before your eyes even register the flash.

### 💨 125Hz Liquid Smoothness
Running at a ultra-responsive **8ms tick rate**, transitions are virtually invisible. This high-frequency interpolation eliminates the "stepping" effect found in other tools, drastically reducing ocular strain and brain fatigue.

---

## 🚀 Key Features

*   **Sun-Sync Circadian Engine**: Precision location-based brightness and Kelvin shifts tailored to your local sunrise/sunset.
*   **Weather-Aware Scaling**: Automatically adjusts to ambient sky conditions using real-time local weather data (sunny vs. overcast).
*   **Modern GUI**: A beautiful control center built with **GTK4 and Libadwaita** for fine-tuning your safety margins.
*   **Multi-Backend Support**: Native integration with `sysfs` (backlight), `DDC/CI` (external monitors), and `KDE Plasma` (DBus).
*   **Efficient Core**: Near-zero CPU overhead thanks to Rust's memory safety and performance.

---


---

## 🛠️ Quick Setup

### 1. Requirements
Ensure you have the following installed on your system:
- `ddcutil` (for external monitors)
- `libadwaita` / `gtk4` (for the GUI)

### 2. Build & Install
```bash
# Clone and build the release binary
cargo build --release

# Setup hardware permissions (backlight access)
sudo cp udev/99-backlight.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

### 3. Run the Daemon
```bash
systemctl --user enable --now auto-brightness.service
```

---

## 🤝 Contributing
We welcome contributions that improve safety and accessibility! Feel free to open an issue or submit a pull request.

---

<p align="center">
  Built with ❤️ for the Linux Accessibility Community
</p>
