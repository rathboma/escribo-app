//! Linux desktop integration: the tray icon, the Screenshots folder watcher,
//! the single-instance socket and in-place AppImage updates. Each runs on a
//! thread of its own and talks to the app over one channel.

use crate::app::is_image_path;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
}

fn tray_icon() -> Vec<ksni::Icon> {
    let Ok(img) = image::load_from_memory_with_format(include_bytes!("../build/tray-icon.png"), image::ImageFormat::Png)
    else {
        return Vec::new();
    };
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    // StatusNotifierItem wants ARGB32 in network byte order.
    let data = img.pixels().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    vec![ksni::Icon { width: w as i32, height: h as i32, data }]
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        crate::app::APP_ID.into()
    }

    fn title(&self) -> String {
        "escribo".into()
    }

    fn icon_name(&self) -> String {
        // Installed packages ship a themed icon; the pixmap covers the AppImage.
        "escribo".into()
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
        use ksni::menu::StandardItem;
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
        vec![
            item("New window", Command::NewWindow),
            item("Choose file", Command::ChooseFile),
            item("Paste from clipboard", Command::Paste),
            ksni::MenuItem::Separator,
            item("Quit", Command::Quit),
        ]
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
            match smol::block_on(Tray { tx }.spawn()) {
                Ok(handle) => {
                    // The service runs on ksni's own thread; keep it alive.
                    std::mem::forget(handle);
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

/// Opens a window for every new image saved into `~/Pictures/Screenshots`.
pub fn watch_screenshots(tx: Sender) {
    let dir = screenshots_dir();
    if let Err(err) = std::fs::create_dir_all(&dir) {
        log::warn!("could not create {}: {err}", dir.display());
        return;
    }
    std::thread::Builder::new()
        .name("watcher".into())
        .spawn(move || {
            use notify::{EventKind, RecursiveMode, Watcher, event::{CreateKind, ModifyKind, RenameMode}};
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
            let mut recent: Vec<(PathBuf, Instant)> = Vec::new();
            for event in erx.into_iter().flatten() {
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
                    if wait_until_written(&path) {
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
   Single instance
   ========================================================================== */

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("escribo-{}", user_name())));
    std::fs::create_dir_all(&dir).ok();
    dir.join("escribo.sock")
}

fn user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "user".into())
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

fn arch_suffix() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
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
