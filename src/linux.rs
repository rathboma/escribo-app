//! Linux desktop integration: the tray icon, the Screenshots folder watcher,
//! the single-instance socket and in-place AppImage updates. Each runs on a
//! thread of its own and talks to the app over one channel.

use crate::app::is_image_path;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Something the app should do, from outside a window.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    NewWindow,
    ChooseFile,
    Paste,
    Open(PathBuf),
    Quit,
    UpdateReady(String),
}

pub type Sender = smol::channel::Sender<Command>;

/* ==========================================================================
Tray
========================================================================== */

struct Tray {
    tx: Sender,
    themed_icon: bool,
    /// The newest images in the Screenshots folder, newest first.
    recent: Vec<PathBuf>,
}

/// The running tray, so the folder watcher can refresh its recent list.
static TRAY: OnceLock<ksni::Handle<Tray>> = OnceLock::new();

/// The tray icon as pixmaps, at the sizes panels draw it, so the host picks
/// one rather than shrinking a large one.
fn tray_icon() -> Vec<ksni::Icon> {
    let sizes: [&[u8]; 5] = [
        include_bytes!("../build/icons/16x16.png"),
        include_bytes!("../build/icons/32x32.png"),
        include_bytes!("../build/icons/48x48.png"),
        include_bytes!("../build/icons/64x64.png"),
        include_bytes!("../build/icons/128x128.png"),
    ];
    sizes
        .iter()
        .filter_map(|png| image::load_from_memory_with_format(png, image::ImageFormat::Png).ok())
        .map(|img| {
            let img = img.to_rgba8();
            let (w, h) = img.dimensions();
            // StatusNotifierItem wants ARGB32 in network byte order.
            let data = img.pixels().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
            ksni::Icon { width: w as i32, height: h as i32, data }
        })
        .collect()
}

/// Whether the desktop can find escribo's icon by name. Installed packages
/// put it in the hicolor theme (a Flatpak under /app/share, which the host
/// sees through its exports); an AppImage carries it inside the image, out
/// of the shell's sight.
fn themed_icon_installed() -> bool {
    if std::env::var_os("APPIMAGE").is_some() {
        return false;
    }
    let system = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let system = if system.is_empty() { "/usr/local/share:/usr/share".to_string() } else { system };
    let icon = format!("icons/hicolor/256x256/apps/{}.png", crate::app::APP_ID);
    dirs::data_dir()
        .into_iter()
        .chain(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from))
        .any(|dir| dir.join(&icon).is_file())
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "escribo".into()
    }

    fn title(&self) -> String {
        "escribo".into()
    }

    fn icon_name(&self) -> String {
        // A name takes precedence over the pixmap, and GNOME's AppIndicator
        // extension shows a placeholder for a name it can't find rather than
        // falling back, so only name an icon that is installed.
        if self.themed_icon { crate::app::APP_ID.into() } else { String::new() }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        tray_icon()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "escribo".into(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.tx.try_send(Command::NewWindow).ok();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, StandardItem};
        let item = |label: &str, cmd: Command| -> ksni::MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |t: &mut Self| {
                    t.tx.try_send(cmd.clone()).ok();
                }),
                ..Default::default()
            }
            .into()
        };
        let heading = |label: &str| -> ksni::MenuItem<Self> {
            StandardItem { label: label.into(), enabled: false, ..Default::default() }.into()
        };

        let mut menu = vec![heading("Recent screenshots")];
        if self.recent.is_empty() {
            menu.push(heading("No screenshots yet"));
        }
        for path in &self.recent {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // A single underscore marks an access key in a tray menu label.
            menu.push(item(&name.replace('_', "__"), Command::Open(path.clone())));
        }
        menu.extend([
            ksni::MenuItem::Separator,
            item("New window", Command::NewWindow),
            item("Choose file", Command::ChooseFile),
            item("Paste from clipboard", Command::Paste),
            ksni::MenuItem::Separator,
            CheckmarkItem {
                label: "Open new screenshots automatically".into(),
                checked: open_on_screenshot().load(Ordering::Relaxed),
                activate: Box::new(|_: &mut Self| {
                    let on = !open_on_screenshot().load(Ordering::Relaxed);
                    open_on_screenshot().store(on, Ordering::Relaxed);
                    save_open_on_screenshot(on);
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            item("Quit", Command::Quit),
        ]);
        menu
    }
}

/// Puts escribo in the tray. Without a StatusNotifierItem host (GNOME needs
/// the AppIndicator extension) this logs and carries on: windows still open
/// as screenshots arrive.
pub fn start_tray(tx: Sender) {
    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            use ksni::TrayMethods;
            let tray =
                Tray { tx, themed_icon: themed_icon_installed(), recent: recent_screenshots(&screenshots_dir()) };
            match smol::block_on(tray.spawn()) {
                Ok(handle) => {
                    // The service runs on ksni's own thread; the handle keeps
                    // it alive, and lets the watcher update the menu.
                    TRAY.set(handle).ok();
                    // Catch anything saved while the tray was starting.
                    refresh_tray();
                }
                Err(err) => log::warn!("no system tray available: {err}"),
            }
        })
        .ok();
}

/* ==========================================================================
Screenshots folder
========================================================================== */

pub fn screenshots_dir() -> PathBuf {
    crate::app::pictures_dir().join("Screenshots")
}

/// How many screenshots the tray lists.
const RECENT_COUNT: usize = 5;

/// The newest images in `dir`, newest first.
fn recent_screenshots(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut shots: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_candidate(path))
        .filter_map(|path| {
            // Removed since the listing, or not a file: skip it.
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            Some((meta.modified().ok()?, path))
        })
        .collect();
    shots.sort_by(|a, b| b.0.cmp(&a.0));
    shots.into_iter().take(RECENT_COUNT).map(|(_, path)| path).collect()
}

/// Re-reads the Screenshots folder into the tray's recent list.
fn refresh_tray() {
    if let Some(tray) = TRAY.get() {
        let recent = recent_screenshots(&screenshots_dir());
        smol::block_on(tray.update(|t| t.recent = recent));
    }
}

/// Watches `~/Pictures/Screenshots`: keeps the tray's recent list current
/// and, unless that's switched off in the tray, opens a window for every new
/// image saved there.
pub fn watch_screenshots(tx: Sender) {
    let dir = screenshots_dir();
    if let Err(err) = std::fs::create_dir_all(&dir) {
        log::warn!("could not create {}: {err}", dir.display());
        return;
    }
    std::thread::Builder::new()
        .name("watcher".into())
        .spawn(move || {
            use notify::{
                EventKind, RecursiveMode, Watcher,
                event::{CreateKind, ModifyKind, RenameMode},
            };
            let (etx, erx) = std::sync::mpsc::channel();
            let mut watcher = match notify::recommended_watcher(etx) {
                Ok(w) => w,
                Err(err) => {
                    log::warn!("could not watch {}: {err}", dir.display());
                    return;
                }
            };
            if let Err(err) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                log::warn!("could not watch {}: {err}", dir.display());
                return;
            }
            // One screenshot fires a burst of events as it's written; the
            // recent list is re-read once the folder has been quiet a moment.
            let (ping, pings) = std::sync::mpsc::channel::<()>();
            std::thread::Builder::new()
                .name("recent".into())
                .spawn(move || {
                    while pings.recv().is_ok() {
                        while pings.recv_timeout(Duration::from_millis(300)).is_ok() {}
                        refresh_tray();
                    }
                })
                .ok();

            let mut recent: Vec<(PathBuf, Instant)> = Vec::new();
            for event in erx.into_iter().flatten() {
                if !matches!(event.kind, EventKind::Access(_)) {
                    ping.send(()).ok();
                }
                // Additions only: a new file, or one renamed into place (the
                // way screenshot tools finish writing).
                let added = matches!(
                    event.kind,
                    EventKind::Create(CreateKind::File | CreateKind::Any)
                        | EventKind::Modify(ModifyKind::Name(RenameMode::To | RenameMode::Both | RenameMode::Any))
                );
                if !added {
                    continue;
                }
                let Some(path) = event.paths.last().cloned() else { continue };
                if !is_candidate(&path) {
                    continue;
                }
                recent.retain(|(_, t)| t.elapsed() < Duration::from_secs(3));
                if recent.iter().any(|(p, _)| *p == path) {
                    continue;
                }
                recent.push((path.clone(), Instant::now()));
                let tx = tx.clone();
                std::thread::spawn(move || {
                    if wait_until_written(&path) && open_on_screenshot().load(Ordering::Relaxed) {
                        log::info!("new screenshot: {}", path.display());
                        tx.send_blocking(Command::Open(path)).ok();
                    }
                });
            }
        })
        .ok();
}

fn is_candidate(path: &Path) -> bool {
    let hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
    !hidden && is_image_path(path)
}

/// Waits for the file's size to settle, so a window never opens on a
/// half-written PNG.
fn wait_until_written(path: &Path) -> bool {
    let mut last = None;
    for _ in 0..40 {
        let size = std::fs::metadata(path).map(|m| m.len()).ok();
        match size {
            None => return false,
            Some(n) if n > 0 && last == Some(n) => return true,
            _ => {}
        }
        last = size;
        std::thread::sleep(Duration::from_millis(120));
    }
    last.is_some_and(|n| n > 0)
}

/* ==========================================================================
Tray preferences
========================================================================== */

/// `app.json`, beside settings.json: preferences the tray owns. The Electron
/// version kept them in the same file under the same keys, so a choice made
/// there carries over.
fn app_store_path() -> Option<PathBuf> {
    crate::settings::Settings::path().map(|p| p.with_file_name("app.json"))
}

fn read_app_store() -> serde_json::Map<String, serde_json::Value> {
    app_store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| match v {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// Whether a new screenshot opens a window by itself. On unless switched off.
fn open_on_screenshot() -> &'static AtomicBool {
    static ON: OnceLock<AtomicBool> = OnceLock::new();
    ON.get_or_init(|| {
        AtomicBool::new(read_app_store().get("openOnScreenshot").and_then(|v| v.as_bool()).unwrap_or(true))
    })
}

fn save_open_on_screenshot(on: bool) {
    let Some(path) = app_store_path() else { return };
    let mut store = read_app_store();
    store.insert("openOnScreenshot".into(), on.into());
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&store).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, &path)
    };
    if let Err(err) = write() {
        log::warn!("could not save {}: {err}", path.display());
    }
}

/* ==========================================================================
Single instance
========================================================================== */

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("escribo-{}", uid())));
    std::fs::create_dir_all(&dir).ok();
    dir.join("escribo.sock")
}

fn uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0)
}

/// Hands `files` to an escribo that is already running. True when one was
/// there to take them — this process should then exit.
pub fn forward_to_running(files: &[PathBuf]) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path()) else {
        return false;
    };
    let mut msg = String::new();
    if files.is_empty() {
        msg.push_str("new\n");
    }
    for f in files {
        let abs = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        msg.push_str(&format!("open {}\n", abs.display()));
    }
    stream.write_all(msg.as_bytes()).is_ok()
}

/// Asks a running escribo to quit (`escribo --quit`).
pub fn quit_running() {
    if let Ok(mut stream) = UnixStream::connect(socket_path()) {
        stream.write_all(b"quit\n").ok();
    }
}

/// Listens for later launches handing over their files.
pub fn listen_for_instances(tx: Sender) {
    let path = socket_path();
    std::fs::remove_file(&path).ok();
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(err) => {
            log::warn!("single-instance socket unavailable: {err}");
            return;
        }
    };
    std::thread::Builder::new()
        .name("instances".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let cmd = if let Some(p) = line.strip_prefix("open ") {
                        Command::Open(PathBuf::from(p))
                    } else if line == "quit" {
                        Command::Quit
                    } else {
                        Command::NewWindow
                    };
                    tx.send_blocking(cmd).ok();
                }
            }
        })
        .ok();
}

/* ==========================================================================
AppImage updates
========================================================================== */

const RELEASES: &str = "https://api.github.com/repos/rathboma/escribo-app/releases/latest";

/// The AppImage updates itself: deb and rpm installs update through apt/dnf
/// from the repos their postinstall scripts register, so they are left alone.
pub fn check_for_updates(tx: Sender) {
    let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
        return;
    };
    if std::env::var_os("ESCRIBO_NO_UPDATE").is_some() {
        return;
    }
    std::thread::Builder::new()
        .name("updater".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(10));
            match update_appimage(&appimage) {
                Ok(Some(version)) => {
                    tx.send_blocking(Command::UpdateReady(version)).ok();
                }
                Ok(None) => {}
                Err(err) => log::warn!("update check failed: {err:#}"),
            }
        })
        .ok();
}

/// The architecture in release asset names, as bin/package-linux.sh writes it.
fn arch_suffix() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("escribo/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(300)))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
        .build()
        .into()
}

/// Downloads a newer AppImage over the running one. Returns the new version.
fn update_appimage(appimage: &Path) -> anyhow::Result<Option<String>> {
    use sha2::Digest;
    let agent = agent();
    let release: serde_json::Value = serde_json::from_reader(agent.get(RELEASES).call()?.into_body().into_reader())?;
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let latest = tag.trim_start_matches('v');
    if !is_newer(latest, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let want = format!("-linux-{}.AppImage", arch_suffix());
    let Some(asset) = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.ends_with(&want))))
    else {
        return Ok(None);
    };
    let url = asset["browser_download_url"].as_str().unwrap_or_default();
    let digest = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase);

    let part = appimage.with_extension("AppImage.part");
    let mut reader = agent.get(url).call()?.into_body().into_reader();
    let mut file = std::fs::File::create(&part)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }
    file.sync_all()?;
    drop(file);
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if let Some(want) = digest {
        if want != got {
            std::fs::remove_file(&part).ok();
            anyhow::bail!("downloaded AppImage does not match its published checksum");
        }
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
    // Renaming over the running file is safe: the running process keeps the
    // old inode, and the next launch gets the new one.
    std::fs::rename(&part, appimage)?;
    Ok(Some(latest.to_string()))
}

/// Semver-ish comparison of `x.y.z[-pre]` strings; prereleases sort before
/// their release.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> (Vec<u64>, Option<String>) {
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p.to_string())),
            None => (v, None),
        };
        (core.split('.').map(|n| n.parse().unwrap_or(0)).collect(), pre)
    }
    let (a, ap) = parse(candidate);
    let (b, bp) = parse(current);
    match a.cmp(&b) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (ap, bp) {
            (None, Some(_)) => true,
            (Some(x), Some(y)) => x > y,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_comes_in_panel_sizes() {
        let icons = tray_icon();
        let sizes: Vec<i32> = icons.iter().map(|i| i.width).collect();
        assert_eq!(sizes, [16, 32, 48, 64, 128]);
        for icon in &icons {
            assert_eq!(icon.width, icon.height);
            assert_eq!(icon.data.len(), (icon.width * icon.height * 4) as usize);
        }
        // ARGB: the corner is transparent, the middle of the mark opaque.
        let big = &icons[4];
        assert_eq!(big.data[0], 0);
        let mid = ((64 * 128 + 20) * 4) as usize;
        assert_eq!(big.data[mid], 255);
    }

    #[test]
    fn recent_screenshots_are_the_newest_images_first() {
        let dir = std::env::temp_dir().join(format!("escribo-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = SystemTime::now() - Duration::from_secs(3600);
        let names = ["a.png", "b.jpg", "c.png", "d.webp", "e.png", "f.png", ".hidden.png", "notes.txt"];
        for (i, name) in names.iter().enumerate() {
            let file = std::fs::File::create(dir.join(name)).unwrap();
            file.set_modified(base + Duration::from_secs(i as u64 * 60)).unwrap();
        }
        std::fs::create_dir_all(dir.join("folder.png")).unwrap();
        let recent: Vec<String> =
            recent_screenshots(&dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(recent, ["f.png", "e.png", "d.webp", "c.png", "b.jpg"]);
        assert!(recent_screenshots(Path::new("/nonexistent/escribo")).is_empty());
    }

    #[test]
    fn version_ordering() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(is_newer("1.0.0", "1.0.0-beta.1"));
        assert!(is_newer("1.0.0-beta.2", "1.0.0-beta.1"));
    }

    #[test]
    fn screenshot_candidates() {
        assert!(is_candidate(Path::new("/x/Screenshot from 2026.png")));
        assert!(!is_candidate(Path::new("/x/.Screenshot.png")));
        assert!(!is_candidate(Path::new("/x/notes.txt")));
    }
}
