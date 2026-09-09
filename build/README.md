# Packaging icons

electron-builder reads this directory (`directories.buildResources`) when it
packages the app, and picks the icon per platform:

| File | Used for | Contents |
| --- | --- | --- |
| `icon.icns` | macOS `.app` bundle | 16, 32, 64, 128, 256, 512, 1024 px, plus the @2x variants |
| `icon.ico` | Windows `.exe` and installer | 16, 24, 32, 48, 64, 128, 256 px |
| `icons/*.png` | Linux (AppImage, deb, `.desktop` entry) | 16 → 1024 px |
| `icon.png` | fallback for any target without one of the above | 1024 px |

At runtime the icon comes from elsewhere: `assets/icon.png` is passed as the
`BrowserWindow` icon (Windows and Linux) and set as the dock icon on macOS, and
`tray-icon.png` is the tray image.

## The mark

A white "e" in a `#ff007f` circle — the same mark the website uses in its
navbar. The glyph is the letter `e` from [Cookie](https://fonts.google.com/specimen/Cookie),
the font already vendored at `assets/fonts/cookie-v23-latin-regular.woff2`,
centred on the circle so its ink box is 47.7% of the canvas tall and sits at
(48.9%, 49.3%) of the canvas.

Every file above was rendered from that font rather than resampled from
`assets/icon.png`, so each size is sharp in its own right. To change the mark,
redraw it at 1024 px and regenerate the set with any icon tool that emits
multi-size `.icns`/`.ico` (for example `iconutil` on macOS, or Pillow's
`Image.save` with an explicit `sizes=` list).
