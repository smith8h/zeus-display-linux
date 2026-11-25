# 🚀 Zeus Display

<p align="center">
  <img src="packaging/icons/io.github.zeusdisplay.ZeusDisplay.svg" alt="Zeus Display icon" width="160">
</p>

<p align="center">
  <strong>Live CPU temperature and load monitoring for your Zeus cooler display</strong>
</p>

Zeus Display is a GTK 4 application for Linux that sends live CPU temperature and load information to a compatible Zeus cooler USB display.

## ✨ Features

- 🌡️ Live CPU temperature and load monitoring
- °C/°F Celsius and Fahrenheit display modes
- 🌓 System, light, and dark themes
- 🖥️ Desktop menu and optional login autostart
- 📍 Status icon in the desktop tray

## 📦 Requirements

- Ubuntu or another Linux distribution with GTK 4
- Rust 1.92 or newer
- Cargo
- A compatible Zeus cooler display
- USB vendor ID `5131` and product ID `2007`

On Ubuntu, install the build dependencies with:

```bash
sudo apt update
sudo apt install -y build-essential cargo libgtk-4-dev
```

## 🔧 Installation

From the project directory, run:

```bash
./install.sh
```

The installer:

- Builds the optimized release binary
- Installs the executable at `/usr/bin/zeus-display`
- Installs the application icons
- Creates a desktop menu entry
- Optionally enables login autostart

Installing the binary requires `sudo` access. To disable login autostart:

```bash
./install.sh --no-autostart
```

## 🔐 USB permissions

The first time the application detects that no udev rule exists, it will print the command needed to grant your desktop user access to the USB display. This rule is also included in `packaging/70-zeus-display.rules`.

Install it with:

```bash
sudo cp packaging/70-zeus-display.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Reconnect the display after applying the rule.

## ▶️ Usage

Open **Zeus Display** from the application menu, or run:

```bash
zeus-display
```

To start without opening the dashboard window:

```bash
zeus-display --background
```

Temperature unit and theme preferences are stored in `~/.config/zeus-display/settings.json`.

## 🛠️ Development

Build and run the debug version:

```bash
cargo run
```

Build the optimized binary:

```bash
cargo build --release
```
