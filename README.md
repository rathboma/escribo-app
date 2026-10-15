# escribo: An Open-Source Skitch Alternative

escribo (formerly Pink Arrows) is a lightweight annotation tool for screenshots. The desktop app is native Rust on [gpui-ce](https://github.com/gpui-ce/gpui-ce) — a single small binary that starts instantly — and ships for Linux. The original browser version still runs at [pinkarrows.app](https://pinkarrows.app) with no installation required; its source lives in [`web/`](web/).

Everything runs locally — nothing is stored server-side.

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
| `Delete` | Delete selection | `Ctrl+O` | Open an image |
| `Esc` | Finish a label, close a popover | `Ctrl+0` / `Ctrl+=` / `Ctrl+-` | Fit / zoom in / zoom out |

## Running the desktop app

You need Rust 1.95 or newer and, on Debian/Ubuntu, the libraries gpui links:

```bash
sudo apt-get install -y build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libx11-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev \
  libxcb-xinput-dev libfontconfig-dev libglib2.0-dev libegl1-mesa-dev
cargo run                       # a blank window
cargo run -- path/to/shot.png   # open an image
cargo test
```

The app lives in the system tray. It opens an annotation window automatically when a new screenshot lands in your `Pictures/Screenshots` folder, and the tray menu can also open a file or paste from the clipboard. Launching `escribo` again (from the app menu, or `escribo file.png` from a file manager) hands the request to the running instance; `escribo --quit` stops it.

The tray uses the StatusNotifierItem protocol: KDE, Xfce, Cinnamon and most others show it out of the box; stock GNOME needs the AppIndicator extension. Without a tray escribo still stays running, so screenshots keep opening windows. Copying images on Wayland uses `wl-copy` (from `wl-clipboard`) when it is installed.

Settings live in `~/.config/escribo/settings.json` — the same file the Electron version wrote, so profiles and presets carry over.

### Packaging

```bash
bin/package-linux.sh                  # every format, written to dist/
bin/package-linux.sh appimage deb     # just these
SKIP_BUILD=1 bin/package-linux.sh rpm # reuse target/release/escribo
```

It needs `dpkg-deb`, `rpmbuild`, and `flatpak-builder` with the flathub remote; `appimagetool` is downloaded on first use. The icons the packages install live in [`build/`](build/README.md); the desktop entry, AppStream metadata and Flatpak manifest in [`packaging/linux/`](packaging/linux/).

| OS | Targets |
| --- | --- |
| Linux | AppImage, deb, rpm and Flatpak, each on x64 and arm64 |

macOS and Windows builds are not wired up yet: gpui-ce supports both, so the
app code is portable, but the tray, screenshot folder, clipboard and update
code in `src/linux.rs` has only a Linux implementation so far.

Artifact names are pinned (`escribo-<version>-linux-<arch>.<ext>`) because
the website resolves a download to a release asset by matching the end of its
filename. Renaming one here means editing the matching `suffix` in
`_data/downloads.yml` in [rathboma/escribo-web](https://github.com/rathboma/escribo-web).

## Releasing

Cutting a release is one command:

```bash
bin/make-release.sh   # bumps Cargo.toml and pushes a v* tag
```

Pushing the tag drives the CI pipeline (`.github/workflows/`):

1. **`publish.yml`** builds the Linux packages natively on x64 and arm64
   runners and attaches them to a **draft** GitHub release. It refuses a tag
   that does not match the version in `Cargo.toml`.
2. Publishing that draft (a manual click in the GitHub UI) fires
   **`release-published.yml`**, which:
   - mirrors every asset to the R2 bucket under `releases/<tag>/` and refreshes
     `latest.json` — the manifest the website resolves downloads from, served at
     `download.escriboapp.com`;
   - publishes the GPG-signed `.deb`/`.rpm` packages to the apt/dnf repos under
     the `deb/` and `rpm/` prefixes of the same bucket, served at
     `repo.escriboapp.com`, and uploads the public key to `escribo.key`.

`build.yml` checks formatting, runs the tests and builds every Linux package on
every PR and every push to `master`, purely as a check — it publishes nothing.

### Auto-update

The **AppImage** updates itself: shortly after launch it checks the latest
GitHub release, downloads the matching AppImage, verifies it against the
SHA-256 digest GitHub publishes for the asset, and swaps it in place — the next
launch runs the new version. **deb/rpm** installs update through `apt`/`dnf`:
the `build/deb-postinstall` and `build/rpm-postinstall` scripts register the
`repo.escriboapp.com` repo and its signing key at install time. The
**Flatpak** updates with `flatpak update`. Set `ESCRIBO_NO_UPDATE=1` to skip
the AppImage check.

### Deploy secrets

`release-published.yml` needs:

- Four R2 (S3-compatible) bucket secrets: `ACCESS_KEY_ID`, `SECRET_ACCESS_KEY`,
  `ENDPOINT_URL`, `BUCKET_NAME`.
- `GPG_PRIVATE_KEY` — the armoured, passphrase-less private key that signs the
  apt/dnf repos. Its public half is committed at `build/escribo.key` and baked
  into the deb/rpm postinstall scripts, so **the signing key must be in place
  before the first stable release** — the repo publish refuses to ship an
  unsigned repo, because installed packages pin `signed-by` / `gpgcheck=1`.


## Contributing

Easiest way to contribute is to toss ideas and features in Github Issues.

If you'd like to contribute with code, clone the repository and see
[Running the desktop app](#running-the-desktop-app). The code is laid out as:

| Path | What |
| --- | --- |
| `src/doc.rs` | the annotations, their geometry, undo history and crop |
| `src/render.rs`, `src/text.rs` | the export renderer (tiny-skia) and the text engine (parley + swash) shared with the editor's labels |
| `src/settings.rs` | settings, profiles and presets, and their import/export files |
| `src/ui/` | the gpui window: stage, toolbar, popovers, preferences |
| `src/linux.rs` | tray, Screenshots watcher, single instance, AppImage updates |
| `web/` | the browser version served at pinkarrows.app |

To work on the browser version, serve `web/` (`cd web && python3 -m http.server`) and open http://localhost:8000.

## Roadmap / feature requests
- [ ] Hotkey tooltips
- [ ] Improve arrow dragging
- [ ] Line feature (not an arrow, just a line)
