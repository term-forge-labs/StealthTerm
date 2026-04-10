![图标](./assets/icon.svg)


# StealthTerm

[![English](https://img.shields.io/badge/lang-English-blue)](./README.md) | [![Chinese](https://img.shields.io/badge/语言-中文-red)](./README_zh.md)

A minimalist, efficient, modern terminal emulator built with Rust.

## ✨ Why Choose StealthTerm?

StealthTerm is designed to provide a **distraction-free, clean terminal experience**. We’ve removed the traditional menu bar to keep all your attention focused on the terminal content itself. The project’s core features are already in place, and we’ll continue to iterate in the future to bring you more practical features.

## 🚀 Current Features

- [x] Minimalist Interface: No menu bar, no title bar, collapsible sidebar, maximizing the content display area
- [x] Terminal Emulation: Full xterm-256color support, ANSI colors, cursor control, scrollable areas, TrueColor
- [x] SSH Connection Management: Sidebar connection tree with support for grouping, drag-and-drop sorting, and AES-256-GCM encrypted credential storage
- [x] SFTP File Manager: Dual-pane local/remote file browser with transfer queue and progress display; supports drag-and-drop file uploads
- [x] Zmodem: Automatically detects rz/sz commands for file transfer; also supports drag-and-drop file uploads
- [x] Server Monitoring: Status bar displays real-time CPU, memory, disk, and network status of SSH sessions
- [x] Multilingual Interface: Supports English and Chinese (Settings → Language; switch on the fly)
- [x] Tabbed Session Management: Easily switch between multiple sessions
- [x] Theme System: Built-in Dracula theme (default)
- [x] Chrome-Style Tabs: Right-click menu, tab copying
- [x] Split Screen: Horizontal and vertical split screens; drag to adjust proportions
- [x] Batch Execution: Send commands to multiple SSH sessions simultaneously
- [x] Command History Autocomplete: Smart suggestions for past commands
- [x] Collapsible Command Output: Collapse command output and copy it with a single click
- [x] Screen Lock: Password-protected screen lock with support for auto-lock on inactivity
- [x] Cross-Platform: Supports Linux, Windows, and macOS

## 🕹️ Tech Stack

| Category | Technology |
|------|------|
| Language | Rust (2024 edition) |
| GUI | egui + eframe (wgpu renderer) |
| SSH | russh + russh-keys |
| SFTP | russh-sftp |
| Terminal | portable-pty + vte |
| Async | tokio |
| Encryption | ring + zeroize |

## 🪁 Build

```bash
# Development build
cargo build

# Release build
cargo build --release

# Windows cross-compilation
cargo xwin build --target x86_64-pc-windows-msvc --release

# Run tests
cargo test --workspace
```

## 🎨 Configuration

Configuration file location:
- Linux/MacOS: `~/.config/stealthterm/settings.toml`
- Windows: `%APPDATA%/stealthterm/settings.toml`

SSH connection configurations and encryption credentials are stored in the same directory.


## 🧵 Known Limitations

- MFA (multi-factor authentication) for Bastion Hosts is not yet supported
- SSH ProxyJump/port forwarding is not yet supported
- Session recovery after crashes is not yet implemented
- Drag-and-drop reordering of tabs is not yet supported
- SFTP resume-from-breakpoint and speed display are not yet implemented
- Custom themes are not yet supported

## 🛠️ Development Roadmap

We are continuously optimizing the project in the following areas:

**🖌️ Polishing and Optimization**
- [ ] Improve terminal rendering quality
- [ ] Improve screen scaling stability
- [ ] Enhance the stability of command history autocompletion and collapsed output
- [ ] Optimize split-screen functionality
- [ ] Improve compatibility across all platform versions


**📋 Planned Features**
- [ ] Session saving and restoration
- [ ] Support for more custom configuration options
- [ ] True theme switching

**🧪 Community Testing**
We welcome your help in testing the stability of the following scenarios through actual use:
- [ ] Display performance in high-resolution/multi-monitor environments
- [ ] Memory/CPU usage during prolonged operation
- [ ] Whether crashes or freezes occur during prolonged operation
- [ ] Program verification on operating systems other than Windows

## 🤝 Contribute

StealthTerm is still in active development, and every piece of feedback you provide is crucial. If you find a bug or have any feature suggestions, please feel free to submit an issue or directly create a pull request.

**Let’s work together to refine a truly user-friendly, modern terminal application.**

## 📄 License

StealthTerm is licensed under the MIT open-source license.

All rights reserved.
