# Packaging icons

`bin/package-linux.sh` installs these as the app's themed icon, and the app
embeds some of them:

| File | Used for | Contents |
| --- | --- | --- |
| `icons/*.png` | Linux packages (hicolor icon theme, AppImage, Flatpak) | 16 → 1024 px |
| `icons/256x256.png` | the window icon, embedded in the binary | 256 px |
| `icons/16x16.png` … `icons/128x128.png` | the tray icon, embedded in the binary | 16 → 128 px |
| `icon.icns` | a future macOS `.app` bundle | 16, 32, 64, 128, 256, 512, 1024 px, plus the @2x variants |
| `icon.ico` | a future Windows `.exe` | 16, 24, 32, 48, 64, 128, 256 px |
| `icon.png` | fallback | 1024 px |

`deb-postinstall` and `rpm-postinstall` become the deb's `postinst` and the
rpm's `%post`; `escribo.key` is the public half of the apt/dnf signing key.

## The mark

A white "e" in a `#ff007f` circle — the same mark the website uses in its
navbar. The glyph is the letter `e` from [Cookie](https://fonts.google.com/specimen/Cookie),
the font vendored at `assets/fonts/Cookie-Regular.ttf` (and as WOFF2 in `web/assets/fonts/`),
centred on the circle so its ink box is 47.7% of the canvas tall and sits at
(48.9%, 49.3%) of the canvas.

Every file above was rendered from that font rather than resampled from
`web/assets/icon.png`, so each size is sharp in its own right. To change the mark,
redraw it at 1024 px and regenerate the set with any icon tool that emits
multi-size `.icns`/`.ico` (for example `iconutil` on macOS, or Pillow's
`Image.save` with an explicit `sizes=` list).
