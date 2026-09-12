# escribo: An Open-Source Web-Based Skitch Alternative

escribo (formerly Pink Arrows) is a lightweight annotation tool for screenshots. It runs as a desktop app on Linux, macOS and Windows, and as a web app at [pinkarrows.app](https://pinkarrows.app) with no installation required.

Everything runs locally — nothing is stored server-side. It heavily uses [Fabric JS](http://fabricjs.com/).

![escribo in Action](assets/readme_gif.gif)

I really loved Skitch. In fact, the pink Skitch arrows and dumbed-down text became a trademark of mine in multiple jobs. In a sea of text on Slack and email, Skitch annotations are a refreshing way to make a single, obvious, and easy to digest point. Since Skitch shut down, I've looked for multiple alternatives that are: Free-ish, have similar styling, and are lightweight (no signin, no server syncing). I didn't find any, and that's how this was born.

## Features

- **Annotate** with arrows, boxes, a highlighter, text labels and emoji
- **Redact** what shouldn't be readable — drag over it and those pixels are scrambled in place, not covered by an overlay
- **Crop** to the part of the screenshot that matters
- **Padding & fill** — add a margin around the shot, on a solid colour or gradient, or leave it transparent
- **Aspect ratio & export size** — frame the output as 16:9, 4:3, 1:1 or 9:16 and export at ½×, 1×, 2× or any width; the choice is remembered between launches
- **Copy to clipboard** (or copy and close the window) or save to disk
- **Presets** — a screenshot style (aspect ratio, export size, padding, fill, watermark) saved under a name and applied in one click from the status bar
- **Profiles** — the global kit: annotation palette, padding fills, label font, watermark branding and advanced screenshot styling (corner rounding, drop shadow, annotation size), plus the presets built on it. Switch profiles in Preferences; import or export a profile (presets included) or a set of presets as JSON to share with your team
- **Watermark tab** — swap the escribo mark for custom text and colours, or keep the default branding
- **Advanced tab** — fine control over screenshot corner rounding, drop shadow and default annotation size, for teams who want more than the defaults
- Light and dark themes, native window chrome on macOS, Windows and Linux

### Hotkeys

On macOS, read `Ctrl` as `⌘`.

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `S` | Select | `Ctrl+C` | Copy image to clipboard |
| `A` | Arrow | `Ctrl+Shift+C` | Copy and close the window |
| `B` | Box | `Ctrl+S` | Save to disk |
| `M` | Mark (highlighter) | `Ctrl+V` | Paste an image |
| `R` | Redact (scramble) | `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `T` | Text | `Ctrl+D` | Duplicate selection |
| `E` | Emoji | `Ctrl+A` | Select every annotation |
| `C` | Crop | `Ctrl+,` | Preferences |

## Running the desktop app

```bash
yarn install
yarn start
```

The app lives in the system tray. It opens an annotation window automatically when a new screenshot lands in your `Pictures/Screenshots` folder, and the tray menu can also open a file or paste from the clipboard.

### Packaging

```bash
yarn dist      # installers for the host OS, written to dist/
yarn pack:dir  # unpacked app only, for a quick look
```

Windows and macOS builds have to run on their own OS. The icons the installers
apply live in [`build/`](build/README.md).

## Contributing

Easiest way to contribute is to toss ideas and features in Github Issues.

If you'd like to contribute with code:

1. Clone the repository:
```bash
git clone https://github.com/rathboma/escribo-app.git
```

2. Run a simple http server:
```bash
cd escribo-app
python3 -m http.server
```

3. Open your browser and go to http://localhost:8000 to start using escribo locally

## Roadmap / feature requests
- [ ] Hotkey tooltips
- [ ] Improve arrow dragging
- [ ] Line feature (not an arrow, just a line)
