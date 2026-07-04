# Icons

App and tray icons go here. Generate them from a single source PNG with:

```bash
cargo tauri icon path/to/source.png
```

Required files (referenced by `tauri.conf.json`):

- `tray.png` — monochrome template icon for the menu bar / tray (macOS uses `iconAsTemplate`).
- `32x32.png`, `128x128.png`, `128x128@2x.png`
- `icon.icns` (macOS), `icon.ico` (Windows, future)

Placeholder icons are not committed; run the command above before building.
