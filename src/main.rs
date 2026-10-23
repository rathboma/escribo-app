//! escribo's entry point: one process, resident in the tray, with a window
//! per screenshot.

use escribo::app::{self, Shared, Source};
use escribo::assets::Assets;
use escribo::text::COOKIE_TTF;
use gpui::{App, QuitMode};
use gpui_ce_elements::editable_text::actions::{DEFAULT_INPUT_CONTEXT, default_bindings};
use std::borrow::Cow;
use std::path::PathBuf;

const HELP: &str = "escribo — annotate screenshots

Usage: escribo [FILE...]

Opens each image in a window of its own. With no files, opens an empty
window. A second launch hands its files to the escribo already running.

  -h, --help       show this help
  -V, --version    show the version
      --quit       quit the escribo that is running
";

fn main() {
    // The Electron version's updater installs a new AppImage by running it
    // with this set and waiting for it to exit; it is how 1.0.0 installs
    // update to this one.
    if std::env::var_os("APPIMAGE_EXIT_AFTER_INSTALL").is_some() {
        return;
    }
    if std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: called before any other thread exists.
        unsafe { std::env::set_var("RUST_LOG", "warn") };
    }
    env_logger_init();

    let mut files = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("-h" | "--help") => {
                print!("{HELP}");
                return;
            }
            Some("-V" | "--version") => {
                println!("escribo {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            Some("--quit") => {
                #[cfg(target_os = "linux")]
                escribo::linux::quit_running();
                return;
            }
            Some(s) if s.starts_with('-') => {}
            _ => files.push(PathBuf::from(arg)),
        }
    }

    #[cfg(target_os = "linux")]
    if escribo::linux::forward_to_running(&files) {
        return;
    }

    gpui_platform::application().with_assets(Assets).with_quit_mode(QuitMode::Explicit).run(move |cx: &mut App| {
        cx.set_app_identity(app::APP_ID, "escribo");
        if let Err(err) = cx.text_system().add_fonts(vec![Cow::Borrowed(COOKIE_TTF)]) {
            log::warn!("could not load the Cookie font: {err:#}");
        }
        cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
        cx.set_prompt_builder(escribo::ui::prompt::build);
        Shared::init(cx);
        start_services(cx);

        // Start with a window: the files asked for, or a blank one whose
        // drop area and file picker are the way in.
        if files.is_empty() {
            app::open_window(Source::Empty, cx);
        }
        for f in files.drain(..) {
            app::open_window(Source::Path(f), cx);
        }
        cx.activate(true);
    });
}

#[cfg(target_os = "linux")]
fn start_services(cx: &mut App) {
    use escribo::linux::{self, Command};
    let (tx, rx) = smol::channel::unbounded::<Command>();
    linux::listen_for_instances(tx.clone());
    linux::start_tray(tx.clone());
    linux::watch_screenshots(tx.clone());
    linux::check_for_updates(tx);

    cx.spawn(async move |cx| {
        while let Ok(cmd) = rx.recv().await {
            let quit = cx.update(|cx| {
                match cmd {
                    Command::NewWindow => {
                        app::open_window(Source::Empty, cx);
                    }
                    Command::Open(path) => {
                        app::open_window(Source::Path(path), cx);
                    }
                    Command::Paste => {
                        app::open_window(Source::Clipboard, cx);
                    }
                    Command::ChooseFile => {
                        if let Some(handle) = app::open_window(Source::Empty, cx) {
                            handle.update(cx, |editor, window, cx| editor.choose_file(window, cx)).ok();
                        }
                    }
                    Command::UpdateReady(version) => {
                        cx.show_system_notification(gpui::SystemNotification {
                            tag: "update".into(),
                            title: format!("escribo {version} is ready").into(),
                            body: "It will be used the next time escribo starts.".into(),
                            actions: Vec::new(),
                        });
                    }
                    Command::Quit => return true,
                }
                false
            });
            if quit {
                cx.update(|cx| {
                    Shared::get(cx).settings.clone().update(cx, |m, _| m.flush());
                    cx.quit();
                });
                break;
            }
        }
    })
    .detach();
}

#[cfg(not(target_os = "linux"))]
fn start_services(_: &mut App) {}

/// A minimal stderr logger honouring RUST_LOG's level, so the binary does not
/// carry a full logging framework.
fn env_logger_init() {
    struct Stderr(log::LevelFilter);
    impl log::Log for Stderr {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= self.0
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                eprintln!("[{}] {}: {}", r.level(), r.target(), r.args());
            }
        }
        fn flush(&self) {}
    }
    let level = match std::env::var("RUST_LOG").unwrap_or_default().to_lowercase().as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "error" => log::LevelFilter::Error,
        "off" => log::LevelFilter::Off,
        _ => log::LevelFilter::Warn,
    };
    if log::set_boxed_logger(Box::new(Stderr(level))).is_ok() {
        log::set_max_level(level);
    }
}
