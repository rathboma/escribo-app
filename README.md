# escribo: An Open-Source Skitch Alternative

escribo (formerly Pink Arrows) is a lightweight annotation tool for screenshots: a native desktop app written in Rust on [gpui-ce](https://github.com/gpui-ce/gpui-ce) — a single small binary that starts instantly. It ships for Linux.

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
- Light and dark themes, and a header bar that follows the desktop's window button layout, the way GNOME's own apps do

### Hotkeys

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

The app lives in the system tray. The tray menu lists the five newest screenshots in your `Pictures/Screenshots` folder — click one to annotate it — and can also open a file or paste from the clipboard. By default a new screenshot also opens an annotation window on its own; untick **Open new screenshots automatically** in the tray menu to turn that off. Launching `escribo` again (from the app menu, or `escribo file.png` from a file manager) hands the request to the running instance; `escribo --quit` stops it.

The tray uses the StatusNotifierItem protocol: KDE, Xfce, Cinnamon and most others show it out of the box; stock GNOME needs the AppIndicator extension. Without a tray escribo still stays running, so screenshots keep opening windows. Copying images on Wayland uses `wl-copy` (from `wl-clipboard`) when it is installed.

Settings live in `~/.config/escribo/settings.json`, and the tray's auto-open choice in `app.json` beside it — the same files the Electron version wrote, so profiles, presets and that choice carry over.

### Packaging

```bash
bin/package-linux.sh                  # every format, written to dist/
bin/package-linux.sh appimage deb     # just these
SKIP_BUILD=1 bin/package-linux.sh rpm # reuse target/release/escribo
```

It needs `dpkg-deb`, `rpmbuild`, and `flatpak-builder` with the flathub remote; `appimagetool` is downloaded on first use. The icons the packages install live in [`build/`](build/README.md); the desktop entry, AppStream metadata and Flatpak manifest in [`packaging/linux/`](packaging/linux/).

| OS | Targets |
| --- | --- |
| Linux | AppImage, deb, rpm and Flatpak, each on x86_64 and arm64 |

macOS and Windows builds are not wired up yet: gpui-ce supports both, so the
app code is portable, but the tray, screenshot folder, clipboard and update
code in `src/linux.rs` has only a Linux implementation so far.

Artifact names are pinned (`escribo-<version>-linux-<arch>.<ext>`) because
the website resolves a download to a release asset by matching the end of its
filename. `<arch>` is `x86_64` or `arm64` in every format, as it has been
since 1.0.0 (`escribo-1.2.3-linux-x86_64.AppImage`). Renaming one here means
editing the matching `suffix` in `_data/downloads.yml` in
[rathboma/escribo-web](https://github.com/rathboma/escribo-web), and the names
`release.yml` checks for.

## Releasing

Cutting a release is one command:

```bash
bin/make-release.sh   # bumps Cargo.toml and pushes a v* tag
```

Pushing the tag drives **`release.yml`**, which takes it all the way to a
public GitHub release with no manual step:

1. creates a draft release for the tag, with generated notes;
2. builds the **Linux AppImage** natively on x86_64 and arm64 runners;
3. attaches both, plus the `latest-linux*.yml` update metadata, checks every
   download is there and publishes the release. A tag with a suffix
   (`v1.2.3-beta.1`) publishes as a prerelease.

The website's `/download` page reads the latest published release, so a
version is downloadable the moment step 3 finishes. The tag has to match the
`Cargo.toml` version — `bin/make-release.sh` takes care of that — or the run
stops before building anything.

**Only the AppImage ships for now.** deb, rpm and Flatpak build locally and in
`build.yml`, but are not released yet.

`release-published.yml` mirrors a release to the R2 bucket
(`download.escriboapp.com`) and publishes the GPG-signed `.deb`/`.rpm` packages
to the apt/dnf repos at `repo.escriboapp.com`. It is manual-only for now — run
it from the Actions tab once deb/rpm are released and its secrets exist.

`build.yml` checks formatting, runs the tests and builds every Linux package on
every PR and every push to `main`, purely as a check — it publishes nothing.

### Auto-update

The **AppImage** updates itself: shortly after launch it checks the latest
GitHub release, downloads the matching AppImage, verifies it against the
SHA-256 digest GitHub publishes for the asset, and swaps it in place — the next
launch runs the new version. Set `ESCRIBO_NO_UPDATE=1` to skip the check.

AppImages of the Electron version (1.0.0) update with `electron-updater`, which
reads `latest-linux.yml` / `latest-linux-arm64.yml` from the latest release.
`bin/package-linux.sh` writes those in the same format, so 1.0.0 installs move
to the Rust app like any other update.

**deb/rpm** installs update through `apt`/`dnf`: the `build/deb-postinstall`
and `build/rpm-postinstall` scripts register the `repo.escriboapp.com` repo and
its signing key at install time. The **Flatpak** updates with `flatpak update`.

### Deploy secrets

`release.yml` needs nothing beyond the built-in `GITHUB_TOKEN`.

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

## Roadmap / feature requests
- [ ] Hotkey tooltips
- [ ] Improve arrow dragging
- [ ] Line feature (not an arrow, just a line)
