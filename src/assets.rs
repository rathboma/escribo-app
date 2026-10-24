//! Icons, served to gpui's `svg()` element from memory. They are the inline
//! SVGs the old index.html carried; gpui tints them with the text colour.

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

macro_rules! icons {
    ($($name:literal => $svg:expr),* $(,)?) => {
        const ICONS: &[(&str, &str)] = &[$(($name, $svg)),*];
    };
}

icons! {
    "select" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M14.082 2.182a.5.5 0 0 1 .103.557L8.528 15.467a.5.5 0 0 1-.917-.007L5.57 10.694.803 8.652a.5.5 0 0 1-.006-.916l12.728-5.657a.5.5 0 0 1 .556.103z"/></svg>"#,
    "arrow" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path fill-rule="evenodd" d="M14 2.5a.5.5 0 0 0-.5-.5h-6a.5.5 0 0 0 0 1h4.793L2.146 13.146a.5.5 0 0 0 .708.708L13 3.707V8.5a.5.5 0 0 0 1 0v-6z"/></svg>"#,
    "rect" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.6"><rect x="2.2" y="2.2" width="11.6" height="11.6" rx="2.6"/></svg>"#,
    "mark" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.5" stroke-linejoin="round"><path d="M9.6 2.6l3.8 3.8-5.6 5.6H4v-3.8z"/><path d="M2.2 14.4h11.6" stroke-linecap="round"/></svg>"#,
    "redact" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg"><rect x="2" y="2" width="12" height="12" rx="2.4" fill="none" stroke="black" stroke-width="1.4"/><path fill="black" d="M4.4 4.4h2.4v2.4H4.4zM9.2 4.4h2.4v2.4H9.2zM6.8 6.8h2.4v2.4H6.8zM4.4 9.2h2.4v2.4H4.4zM9.2 9.2h2.4v2.4H9.2z"/></svg>"#,
    "text" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M12.258 3h-8.51l-.083 2.46h.479c.26-1.544.758-1.783 2.693-1.845l.424-.013v7.827c0 .663-.144.82-1.3.923v.52h4.082v-.52c-1.162-.103-1.306-.26-1.306-.923V3.602l.431.013c1.934.062 2.434.301 2.693 1.846h.479L12.258 3z"/></svg>"#,
    "emoji" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14zm0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16z"/><path d="M4.285 9.567a.5.5 0 0 1 .683.183A3.498 3.498 0 0 0 8 11.5a3.498 3.498 0 0 0 3.032-1.75.5.5 0 1 1 .866.5A4.498 4.498 0 0 1 8 12.5a4.498 4.498 0 0 1-3.898-2.25.5.5 0 0 1 .183-.683zM7 6.5C7 7.328 6.552 8 6 8s-1-.672-1-1.5S5.448 5 6 5s1 .672 1 1.5zm4 0c0 .828-.448 1.5-1 1.5s-1-.672-1-1.5S9.448 5 10 5s1 .672 1 1.5z"/></svg>"#,
    "crop" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round"><path d="M4.2 1v10.8h10.8"/><path d="M1 4.2h10.8V15"/></svg>"#,
    "undo" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path fill-rule="evenodd" d="M8 3a5 5 0 1 1-4.546 2.914.5.5 0 0 0-.908-.417A6 6 0 1 0 8 2v1z"/><path d="M8 4.466V.534a.25.25 0 0 0-.41-.192L5.23 2.308a.25.25 0 0 0 0 .384l2.36 1.966A.25.25 0 0 0 8 4.466z"/></svg>"#,
    "redo" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path fill-rule="evenodd" d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/><path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/></svg>"#,
    "copy" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.4"><rect x="5.4" y="1.6" width="9" height="9" rx="2"/><path d="M10.6 13.2v.6a1.6 1.6 0 0 1-1.6 1.6H3.2a1.6 1.6 0 0 1-1.6-1.6V6.4a1.6 1.6 0 0 1 1.6-1.6h.6"/></svg>"#,
    "copy-close" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round"><path d="M6.2 2.2H3.4A1.6 1.6 0 0 0 1.8 3.8v8.4a1.6 1.6 0 0 0 1.6 1.6h2.8"/><path d="M10.4 11.2 13.8 8l-3.4-3.2M13.4 8H5.6"/></svg>"#,
    "caret" => r#"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M1 3.3h8L5 7.6z"/></svg>"#,
    "save" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/><path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/></svg>"#,
    "gear" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M8 4.754a3.246 3.246 0 1 0 0 6.492 3.246 3.246 0 0 0 0-6.492zM5.754 8a2.246 2.246 0 1 1 4.492 0 2.246 2.246 0 0 1-4.492 0z"/><path d="M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 0 1-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 0 1-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 0 1 .52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 0 1 1.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 0 1 1.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 0 1 .52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 0 1-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 0 1-1.255-.52l-.094-.319zm-2.633.283c.246-.835 1.428-.835 1.674 0l.094.319a1.873 1.873 0 0 0 2.693 1.115l.291-.16c.764-.415 1.6.42 1.184 1.185l-.159.292a1.873 1.873 0 0 0 1.116 2.692l.318.094c.835.246.835 1.428 0 1.674l-.319.094a1.873 1.873 0 0 0-1.115 2.693l.16.291c.415.764-.42 1.6-1.185 1.184l-.291-.159a1.873 1.873 0 0 0-2.693 1.116l-.094.318c-.246.835-1.428.835-1.674 0l-.094-.319a1.873 1.873 0 0 0-2.692-1.115l-.292.16c-.764.415-1.6-.42-1.184-1.185l.159-.291A1.873 1.873 0 0 0 1.945 8.93l-.319-.094c-.835-.246-.835-1.428 0-1.674l.319-.094A1.873 1.873 0 0 0 3.06 4.377l-.16-.292c-.415-.764.42-1.6 1.185-1.184l.292.159a1.873 1.873 0 0 0 2.692-1.115l.094-.319z"/></svg>"#,
    // Window controls, drawn like Adwaita's symbolic icons on a 16px grid.
    "win-min" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><rect x="4" y="10" width="8" height="2" rx="0.5"/></svg>"#,
    "win-max" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" stroke="black" stroke-width="1.5" fill="none"><rect x="4.25" y="4.25" width="7.5" height="7.5" rx="1"/></svg>"#,
    "win-restore" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" stroke="black" stroke-width="1.5" fill="none"><rect x="5.25" y="5.25" width="5.5" height="5.5" rx="0.75"/></svg>"#,
    "win-close" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" stroke="black" stroke-width="1.6"><path d="M4.4 4.4l7.2 7.2M11.6 4.4l-7.2 7.2"/></svg>"#,
    "close" => r#"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg" stroke="black" stroke-width="1.5" stroke-linecap="round"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7"/></svg>"#,
    "presets" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.4" stroke-linejoin="round"><path d="M8 2.2 2.4 5.3 8 8.4l5.6-3.1z"/><path d="M2.4 8.3 8 11.4l5.6-3.1" stroke-linecap="round"/><path d="M2.4 11.2 8 14.3l5.6-3.1" stroke-linecap="round"/></svg>"#,
    "ratio" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.4"><rect x="1.7" y="3.4" width="12.6" height="9.2" rx="1.6"/><path d="M5.4 3.4v9.2"/></svg>"#,
    "minus" => r#"<svg viewBox="0 0 12 12" xmlns="http://www.w3.org/2000/svg" stroke="black" stroke-width="1.5" stroke-linecap="round"><path d="M2 6h8"/></svg>"#,
    "plus" => r#"<svg viewBox="0 0 12 12" xmlns="http://www.w3.org/2000/svg" stroke="black" stroke-width="1.5" stroke-linecap="round"><path d="M6 2v8M2 6h8"/></svg>"#,
    "moon" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M6 .278a.768.768 0 0 1 .08.858 7.208 7.208 0 0 0-.878 3.46c0 4.021 3.278 7.277 7.318 7.277.527 0 1.04-.055 1.533-.16a.787.787 0 0 1 .81.316.733.733 0 0 1-.031.893A8.349 8.349 0 0 1 8.344 16C3.734 16 0 12.286 0 7.71 0 4.266 2.114 1.312 5.124.06A.752.752 0 0 1 6 .278z"/></svg>"#,
    "star" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M3.612 15.443c-.386.198-.824-.149-.746-.592l.83-4.73L.173 6.765c-.329-.314-.158-.888.283-.95l4.898-.696L7.538.792c.197-.39.73-.39.927 0l2.184 4.327 4.898.696c.441.062.612.636.283.95l-3.523 3.356.83 4.73c.078.443-.36.79-.746.592L8 13.187l-4.389 2.256z"/></svg>"#,
    "update" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path fill-rule="evenodd" d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/><path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/></svg>"#,
    "export" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/><path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/></svg>"#,
    "rename" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/></svg>"#,
    "duplicate" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="black"><path d="M4 2a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V2zm2-1a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V2a1 1 0 0 0-1-1H6zM2 5a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-1h1v1a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h1v1H2z"/></svg>"#,
    "tab-backdrop" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.5"><rect x="1.9" y="1.9" width="12.2" height="12.2" rx="2.4"/><rect x="4.7" y="4.7" width="6.6" height="6.6" rx="1.2"/></svg>"#,
    "tab-advanced" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.5"><path d="M8 1v2.2M8 12.8V15M15 8h-2.2M3.2 8H1M12.6 3.4l-1.55 1.55M4.95 11.05 3.4 12.6M12.6 12.6l-1.55-1.55M4.95 4.95 3.4 3.4" stroke-linecap="round"/><circle cx="8" cy="8" r="3"/></svg>"#,
    "tab-watermark" => r#"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="1.5"><rect x="1.9" y="4.4" width="12.2" height="7.2" rx="1.6"/><circle cx="5" cy="8" r="1.3" fill="black" stroke="none"/><path d="M8.1 9.6h4" stroke-linecap="round"/></svg>"#,
}

pub fn icon_path(name: &str) -> SharedString {
    SharedString::from(format!("icons/{name}.svg"))
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let name = path.strip_prefix("icons/").and_then(|p| p.strip_suffix(".svg"));
        Ok(name.and_then(|n| ICONS.iter().find(|(k, _)| *k == n)).map(|(_, svg)| Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(ICONS.iter().map(|(k, _)| icon_path(k)).filter(|p| p.starts_with(path)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses() {
        for (name, svg) in ICONS {
            assert!(
                resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).is_ok(),
                "icon {name} does not parse"
            );
        }
        assert!(Assets.load("icons/arrow.svg").unwrap().is_some());
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }
}
