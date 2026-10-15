//! Application-wide state shared by every window, and the glue to the
//! platform: opening windows, reading images, the clipboard and dialogs.

use crate::doc::Shot;
use crate::settings::Settings;
use crate::text::TextEngine;
use crate::ui::Editor;
use anyhow::{Context as _, Result, anyhow};
use gpui::{
    App, AppContext as _, Bounds, ClipboardEntry, Context, Entity, Global, Size, Task, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowHandle, WindowOptions, point, px, size,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The desktop file, icon and Wayland app id all go by this name.
pub const APP_ID: &str = "com.escriboapp.escribo";

/// Settings shared by every window. Any change notifies every editor and is
/// written to disk shortly after — a slider drag saves once, not per pixel.
pub struct SettingsModel {
    pub settings: Settings,
    save: Option<Task<()>>,
}

impl SettingsModel {
    pub fn update(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        f(&mut self.settings);
        cx.notify();
        self.schedule_save(cx);
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let Ok(settings) = this.read_with(cx, |m, _| m.settings.clone()) else {
                return;
            };
            if let Err(err) = cx.background_executor().spawn(async move { settings.save() }).await {
                log::warn!("could not save settings: {err}");
            }
        }));
    }

    /// Writes immediately, for quitting.
    pub fn flush(&mut self) {
        if self.save.take().is_some() {
            if let Err(err) = self.settings.save() {
                log::warn!("could not save settings: {err}");
            }
        }
    }
}

pub struct Shared {
    pub settings: Entity<SettingsModel>,
    /// Text engine for on-screen bitmaps, used on the main thread only.
    pub text: Rc<RefCell<TextEngine>>,
}

impl Global for Shared {}

impl Shared {
    pub fn init(cx: &mut App) {
        let settings = cx.new(|_| SettingsModel { settings: Settings::load(), save: None });
        cx.set_global(Shared { settings, text: Rc::new(RefCell::new(TextEngine::new())) });
    }

    pub fn get(cx: &App) -> &Shared {
        cx.global::<Shared>()
    }
}

/// The export renderer's own text engine: exports compose off the main
/// thread so a 4K render never stalls the UI.
pub fn export_text_engine() -> &'static Mutex<TextEngine> {
    static ENGINE: OnceLock<Mutex<TextEngine>> = OnceLock::new();
    ENGINE.get_or_init(|| Mutex::new(TextEngine::new()))
}

/// What a new window opens on.
pub enum Source {
    Empty,
    Path(PathBuf),
    Clipboard,
}

pub fn open_window(source: Source, cx: &mut App) -> Option<WindowHandle<Editor>> {
    let bounds = Bounds::centered(None, size(px(1280.0), px(830.0)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("escribo".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(13.0), px(13.0))),
        }),
        window_min_size: Some(Size { width: px(720.0), height: px(520.0) }),
        window_decorations: Some(WindowDecorations::Client),
        app_id: Some(APP_ID.into()),
        icon: window_icon(),
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            // The frame's shadow and rounded corners need an alpha channel;
            // on X11 gpui only asks for one when the background isn't opaque.
            window.set_background_appearance(WindowBackgroundAppearance::Transparent);
            let editor = cx.new(|cx| Editor::new(window, cx));
            editor.update(cx, |editor, cx| editor.open_source(source, window, cx));
            editor
        })
        .map_err(|err| log::error!("could not open a window: {err:#}"))
        .ok()?;
    handle.update(cx, |_, window, _| window.activate()).ok();
    Some(handle)
}

fn window_icon() -> Option<Arc<image::RgbaImage>> {
    static ICON: OnceLock<Option<Arc<image::RgbaImage>>> = OnceLock::new();
    ICON.get_or_init(|| {
        image::load_from_memory_with_format(include_bytes!("../build/icons/256x256.png"), image::ImageFormat::Png)
            .ok()
            .map(|img| Arc::new(img.to_rgba8()))
    })
    .clone()
}

/* ==========================================================================
Images in
========================================================================== */

/// Decodes any image escribo opens: PNG, JPEG, GIF (first frame), WebP, BMP
/// and SVG (rasterised at its natural size).
pub fn decode_image(bytes: &[u8]) -> Result<Shot> {
    let looks_svg = {
        let head = &bytes[..bytes.len().min(512)];
        let text = String::from_utf8_lossy(head);
        text.contains("<svg") || (text.trim_start().starts_with("<?xml") && text.contains("svg"))
    };
    if looks_svg {
        return decode_svg(bytes);
    }
    let img = image::load_from_memory(bytes).context("unsupported or damaged image")?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err(anyhow!("empty image"));
    }
    Ok(Shot::new(w, h, rgba.into_raw()))
}

fn decode_svg(bytes: &[u8]) -> Result<Shot> {
    use resvg::usvg;
    let mut opts = usvg::Options::default();
    opts.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(bytes, &opts)?;
    let s = tree.size();
    // Very small icons are blown up so there is something to annotate.
    let scale = (512.0 / s.width().max(s.height())).max(1.0);
    let (w, h) = ((s.width() * scale).ceil() as u32, (s.height() * scale).ceil() as u32);
    let mut pm = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| anyhow!("bad svg size"))?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    Ok(Shot::new(pm.width(), pm.height(), crate::raster::to_rgba_straight(&pm)))
}

pub fn read_image_file(path: &Path) -> Result<Shot> {
    let bytes = std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    decode_image(&bytes)
}

pub fn display_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into())
}

/// The image on the clipboard: image data, or a copied image file.
pub enum ClipboardImage {
    Bytes(Vec<u8>),
    File(PathBuf),
}

pub fn clipboard_image(cx: &App) -> Option<ClipboardImage> {
    let item = cx.read_from_clipboard()?;
    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(img) if !img.bytes.is_empty() => {
                return Some(ClipboardImage::Bytes(img.bytes.clone()));
            }
            ClipboardEntry::ExternalPaths(paths) => {
                if let Some(p) = paths.paths().iter().find(|p| is_image_path(p)) {
                    return Some(ClipboardImage::File(p.clone()));
                }
            }
            ClipboardEntry::String(s) => {
                // A path or file:// URI copied as text.
                let text = s.text().trim();
                let path = PathBuf::from(text.strip_prefix("file://").unwrap_or(text));
                if is_image_path(&path) && path.is_file() {
                    return Some(ClipboardImage::File(path));
                }
            }
            _ => {}
        }
    }
    None
}

pub fn is_image_path(p: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
}

/* ==========================================================================
Images out
========================================================================== */

/// Puts an image (straight RGBA) on the clipboard. gpui-ce's Linux backends
/// only write text, so images go through wl-copy on Wayland — which works on
/// every compositor, GNOME included — or arboard (X11, or Wayland's
/// data-control protocol). Both keep serving the image after the window that
/// copied it closes, as escribo stays running in the tray.
pub fn copy_image(w: u32, h: u32, rgba: Vec<u8>) -> Result<()> {
    if gpui::guess_compositor() == "Wayland" {
        let png = crate::raster::encode_png(w, h, &rgba)?;
        match wl_copy(&png) {
            Ok(()) => return Ok(()),
            Err(err) => log::info!("wl-copy unavailable, trying arboard: {err:#}"),
        }
    }
    copy_with_arboard(w, h, rgba)
}

#[cfg(target_os = "linux")]
fn copy_with_arboard(w: u32, h: u32, rgba: Vec<u8>) -> Result<()> {
    static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    let slot = CLIPBOARD.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().map_err(|_| anyhow!("clipboard poisoned"))?;
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().context("no clipboard available")?);
    }
    let clip = guard.as_mut().expect("just set");
    clip.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: rgba.into() })
        .context("could not put the image on the clipboard")
}

#[cfg(not(target_os = "linux"))]
fn copy_with_arboard(_: u32, _: u32, _: Vec<u8>) -> Result<()> {
    Err(anyhow!("image copy is not supported on this platform yet"))
}

fn wl_copy(png: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("wl-copy is not installed")?;
    child.stdin.take().ok_or_else(|| anyhow!("wl-copy has no stdin"))?.write_all(png)?;
    // wl-copy forks to serve the clipboard and its first process exits.
    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("wl-copy failed"));
    }
    Ok(())
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `~/Pictures`, or wherever the XDG user dirs put it — as Electron's
/// `app.getPath('pictures')` resolved it.
pub fn pictures_dir() -> PathBuf {
    dirs::picture_dir().unwrap_or_else(|| home().join("Pictures"))
}

pub fn documents_dir() -> PathBuf {
    dirs::document_dir().unwrap_or_else(|| home().join("Documents"))
}

/// Saves through the dialog; where there is no dialog to show (no
/// xdg-desktop-portal on this desktop) the file goes straight into `dir`
/// under a name that does not clobber anything. Returns the path and whether
/// the dialog was used, or None when it was cancelled.
pub async fn save_or_fallback(
    cx: &mut gpui::AsyncApp,
    dir: PathBuf,
    name: String,
    bytes: Vec<u8>,
) -> Result<Option<(PathBuf, bool)>> {
    let rx = cx.update(|cx| cx.prompt_for_new_path(&dir, Some(&name)));
    match rx.await.map_err(|_| anyhow!("dialog closed")).and_then(|r| r) {
        Ok(Some(path)) => {
            let write_path = path.clone();
            cx.background_executor().spawn(async move { std::fs::write(&write_path, bytes) }).await?;
            Ok(Some((path, true)))
        }
        Ok(None) => Ok(None),
        Err(err) => {
            log::warn!("no save dialog ({err:#}); saving into {}", dir.display());
            let path = unique_path(&dir, &name);
            let write_path = path.clone();
            cx.background_executor()
                .spawn(async move {
                    std::fs::create_dir_all(write_path.parent().unwrap_or(Path::new(".")))?;
                    std::fs::write(&write_path, bytes)
                })
                .await?;
            Ok(Some((path, false)))
        }
    }
}

/// `dir/name`, or `dir/stem (2).ext` and so on if that is taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// A path for display, with the home directory as `~`.
pub fn tilde(p: &Path) -> String {
    match dirs::home_dir().and_then(|h| p.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => p.display().to_string(),
    }
}

/// Asks for one file to open. Ok(None) when cancelled.
pub async fn open_with_dialog(cx: &mut gpui::AsyncApp, prompt: &str) -> Result<Option<PathBuf>> {
    let rx = cx.update(|cx| {
        cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(prompt.to_string().into()),
        })
    });
    let paths = rx.await.map_err(|_| anyhow!("dialog closed"))??;
    Ok(paths.and_then(|p| p.into_iter().next()))
}
