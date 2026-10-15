//! Persisted settings: the current screenshot style, the theme, and the
//! profiles (each a global kit — palette, fills, label font, watermark,
//! advanced style — that owns its presets).
//!
//! The file is the one electron-store kept, `<config>/escribo/settings.json`,
//! in the same shape, so settings carry over from the Electron app. Everything
//! read from disk or imported is checked field by field the way the old
//! renderer did it: an import is untrusted, and a bad field falls back to its
//! default rather than failing the whole file.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

pub const SOLID_NAMES: [&str; 4] = ["Ink", "Sand", "Slate", "Blush"];
pub const GRAD_NAMES: [&str; 4] = ["Dusk", "Rose", "Sky", "Mint"];

pub const PRESET_FILE_KIND: &str = "escribo-presets";
pub const PROFILE_FILE_KIND: &str = "escribo-profile";

/// Output aspect ratios. `None` is "Orig": the box follows the screenshot.
pub struct Ratio {
    pub id: &'static str,
    pub label: &'static str,
    pub value: Option<f64>,
    pub hint: &'static str,
}

pub const RATIOS: [Ratio; 5] = [
    Ratio { id: "orig", label: "Orig", value: None, hint: "Match the screenshot" },
    Ratio { id: "16:9", label: "16:9", value: Some(16.0 / 9.0), hint: "Slides, video, Twitter" },
    Ratio { id: "4:3", label: "4:3", value: Some(4.0 / 3.0), hint: "Docs and older displays" },
    Ratio { id: "1:1", label: "1:1", value: Some(1.0), hint: "Square — social posts" },
    Ratio { id: "9:16", label: "9:16", value: Some(9.0 / 16.0), hint: "Portrait — stories" },
];

pub const SCALES: [(&str, f64); 3] = [("½×", 0.5), ("1×", 1.0), ("2×", 2.0)];

pub const OUT_W_MIN: u32 = 240;
pub const OUT_W_MAX: u32 = 8000;
pub const PAD_MAX: u32 = 18;

/// Label fonts. Families that exist offline everywhere, plus the bundled brand
/// script; anything else is stored as a custom family string.
pub struct FontChoice {
    pub id: &'static str,
    pub label: &'static str,
    /// A CSS family list; the first family installed wins.
    pub css: &'static str,
}

/// The UI's own font, as the old stylesheet named it.
pub const UI_FONT: &str = "Cantarell, Ubuntu, \"Segoe UI\", system-ui, sans-serif";

pub const FONTS: [FontChoice; 4] = [
    FontChoice { id: "system", label: "System", css: UI_FONT },
    FontChoice { id: "serif", label: "Serif", css: "Georgia, \"Times New Roman\", serif" },
    FontChoice { id: "mono", label: "Mono", css: "Menlo, Consolas, \"DejaVu Sans Mono\", monospace" },
    FontChoice { id: "script", label: "Script", css: "Cookie, cursive" },
];

/// The family list for a label font setting: a known id, or a custom family.
pub fn font_css(id: &str) -> String {
    match FONTS.iter().find(|f| f.id == id) {
        Some(f) => f.css.to_string(),
        None if id.trim().is_empty() => FONTS[0].css.to_string(),
        None => format!("{}, sans-serif", id.trim()),
    }
}

/* ==========================================================================
   Advanced
   ========================================================================== */

/// Knobs that were fixed magic numbers baked into the render. Part of the
/// profile, so a team can share a house style including these.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Advanced {
    /// % of the shot's width — corner rounding when there's padding
    pub corner_radius: f64,
    /// drop-shadow blur behind the shot, at a 900px-wide baseline
    pub shadow_blur: f64,
    /// drop-shadow vertical offset, same baseline
    pub shadow_offset_y: f64,
    /// drop-shadow opacity, 0-100
    pub shadow_opacity: f64,
    /// size of arrows, boxes, labels & emoji, % of the default
    pub annotation_scale: f64,
    /// corner rounding of the box & highlighter tools, same baseline
    pub box_radius: f64,
    /// highlighter fill opacity, 0-100
    pub highlight_opacity: f64,
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            corner_radius: 1.2,
            shadow_blur: 34.0,
            shadow_offset_y: 10.0,
            shadow_opacity: 32.0,
            annotation_scale: 100.0,
            box_radius: 2.0,
            highlight_opacity: 33.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AdvancedKey {
    CornerRadius,
    ShadowBlur,
    ShadowOffsetY,
    ShadowOpacity,
    AnnotationScale,
    BoxRadius,
    HighlightOpacity,
}

pub struct AdvancedMeta {
    pub key: AdvancedKey,
    pub json: &'static str,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub unit: &'static str,
    pub hint: &'static str,
}

/// The order here is the order the sliders render in.
pub const ADVANCED_META: [AdvancedMeta; 7] = [
    AdvancedMeta { key: AdvancedKey::CornerRadius, json: "cornerRadius", label: "Corner radius", min: 0.0, max: 5.0, step: 0.1, unit: "%", hint: "Rounds the screenshot's corners when there's padding." },
    AdvancedMeta { key: AdvancedKey::ShadowBlur, json: "shadowBlur", label: "Shadow blur", min: 0.0, max: 80.0, step: 1.0, unit: "px", hint: "Softness of the drop shadow behind the screenshot." },
    AdvancedMeta { key: AdvancedKey::ShadowOffsetY, json: "shadowOffsetY", label: "Shadow offset", min: 0.0, max: 40.0, step: 1.0, unit: "px", hint: "How far the drop shadow falls below the screenshot." },
    AdvancedMeta { key: AdvancedKey::ShadowOpacity, json: "shadowOpacity", label: "Shadow opacity", min: 0.0, max: 100.0, step: 1.0, unit: "%", hint: "Darkness of the drop shadow." },
    AdvancedMeta { key: AdvancedKey::AnnotationScale, json: "annotationScale", label: "Annotation size", min: 50.0, max: 200.0, step: 5.0, unit: "%", hint: "Size of new arrows, boxes, labels and emoji." },
    AdvancedMeta { key: AdvancedKey::BoxRadius, json: "boxRadius", label: "Box corner radius", min: 0.0, max: 20.0, step: 1.0, unit: "px", hint: "Corner rounding for the box and highlighter tools." },
    AdvancedMeta { key: AdvancedKey::HighlightOpacity, json: "highlightOpacity", label: "Highlighter opacity", min: 10.0, max: 90.0, step: 1.0, unit: "%", hint: "Fill opacity of the highlighter tool." },
];

impl Advanced {
    pub fn get(&self, key: AdvancedKey) -> f64 {
        match key {
            AdvancedKey::CornerRadius => self.corner_radius,
            AdvancedKey::ShadowBlur => self.shadow_blur,
            AdvancedKey::ShadowOffsetY => self.shadow_offset_y,
            AdvancedKey::ShadowOpacity => self.shadow_opacity,
            AdvancedKey::AnnotationScale => self.annotation_scale,
            AdvancedKey::BoxRadius => self.box_radius,
            AdvancedKey::HighlightOpacity => self.highlight_opacity,
        }
    }

    pub fn set(&mut self, key: AdvancedKey, value: f64) {
        let meta = ADVANCED_META.iter().find(|m| m.key == key).expect("every key has meta");
        let value = if value.is_finite() { value.clamp(meta.min, meta.max) } else { self.get(key) };
        match key {
            AdvancedKey::CornerRadius => self.corner_radius = value,
            AdvancedKey::ShadowBlur => self.shadow_blur = value,
            AdvancedKey::ShadowOffsetY => self.shadow_offset_y = value,
            AdvancedKey::ShadowOpacity => self.shadow_opacity = value,
            AdvancedKey::AnnotationScale => self.annotation_scale = value,
            AdvancedKey::BoxRadius => self.box_radius = value,
            AdvancedKey::HighlightOpacity => self.highlight_opacity = value,
        }
    }

    /// Each option checked and clamped to its range, with the default for
    /// anything missing or not a number.
    fn sanitize(v: Option<&Value>) -> Self {
        let mut out = Self::default();
        let src = v.and_then(Value::as_object);
        for meta in &ADVANCED_META {
            let n = src.and_then(|o| o.get(meta.json)).and_then(js_number);
            if let Some(n) = n {
                out.set(meta.key, n);
            }
        }
        out
    }
}

pub fn format_advanced(value: f64, unit: &str) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    format!("{}{unit}", fmt_num(rounded))
}

/* ==========================================================================
   Presets — a screenshot style
   ========================================================================== */

/// The style fields a preset may set. Every field but `out_w` is optional
/// because an imported preset may leave any of them out; `out_w` is always
/// present (`null` follows the scale), as it was in the old app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StyleValues {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_scale: Option<f64>,
    pub out_w: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
}

impl StyleValues {
    /// Keeps only fields a preset may set, each checked — imports are untrusted.
    pub fn sanitize(v: Option<&Value>) -> Self {
        let mut out = Self::default();
        let Some(o) = v.and_then(Value::as_object) else {
            return out;
        };
        if let Some(r) = o.get("ratio").and_then(Value::as_str) {
            if RATIOS.iter().any(|x| x.id == r) {
                out.ratio = Some(r.to_string());
            }
        }
        if let Some(k) = o.get("outScale").and_then(Value::as_f64) {
            if SCALES.iter().any(|(_, s)| *s == k) {
                out.out_scale = Some(k);
            }
        }
        out.out_w = o
            .get("outW")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .map(|n| (js_round(n) as i64).clamp(OUT_W_MIN as i64, OUT_W_MAX as i64) as u32);
        if let Some(p) = o.get("pad").and_then(Value::as_f64).filter(|n| n.is_finite()) {
            out.pad = Some((js_round(p) as i64).clamp(0, PAD_MAX as i64) as u32);
        }
        if let Some(b) = o.get("backdrop").and_then(as_integer) {
            if (0..=8).contains(&b) {
                out.backdrop = Some(b as u8);
            }
        }
        if let Some(w) = o.get("watermark").and_then(Value::as_bool) {
            out.watermark = Some(w);
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub values: StyleValues,
}

impl Preset {
    pub fn sanitize(p: &Value) -> Option<Self> {
        let o = p.as_object()?;
        let name = o.get("name")?.as_str()?;
        let values = o.get("values").filter(|v| v.is_object())?;
        Some(Self {
            id: valid_id(o.get("id")).unwrap_or_else(|| new_id("p")),
            name: clean_name(name, "Untitled"),
            created_at: o
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(now_iso),
            updated_at: o.get("updatedAt").and_then(Value::as_str).map(str::to_string),
            values: StyleValues::sanitize(Some(values)),
        })
    }
}

/* ==========================================================================
   Profiles — the global kit
   ========================================================================== */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub a: String,
    pub b: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WmMode {
    Default,
    Custom,
}

/// A profile's kit. Separate from the identity fields so "Restore defaults"
/// can reset exactly this.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kit {
    pub palette: Vec<String>,
    pub default_idx: usize,
    pub solid_bg: Vec<String>,
    pub grad_bg: Vec<Gradient>,
    pub font: String,
    /// 'default' is the escribo mark and wordmark; 'custom' is free text with
    /// no icon. Colour and background apply to both.
    pub wm_mode: WmMode,
    pub wm_text: String,
    pub wm_color: String,
    pub wm_bg: String,
    pub advanced: Advanced,
}

impl Default for Kit {
    fn default() -> Self {
        Self {
            palette: ["#FF007F", "#FF3B30", "#FFB300", "#12B76A", "#2E7DFF", "#15151A"]
                .map(String::from)
                .to_vec(),
            default_idx: 0,
            solid_bg: ["#101014", "#EFE7DC", "#3A4354", "#F7D6E4"].map(String::from).to_vec(),
            grad_bg: [
                ("#2B3242", "#12151D"),
                ("#FFD9E8", "#FF9CC6"),
                ("#C6E2FF", "#7FAEFF"),
                ("#D9F5E8", "#8AD6B8"),
            ]
            .map(|(a, b)| Gradient { a: a.into(), b: b.into() })
            .to_vec(),
            font: "system".into(),
            wm_mode: WmMode::Default,
            wm_text: String::new(),
            wm_color: "#7c7c88".into(),
            wm_bg: "#ffffff".into(),
            advanced: Advanced::default(),
        }
    }
}

impl Kit {
    /// A profile's options, each checked, with defaults filled in.
    fn sanitize(src: Option<&Map<String, Value>>) -> Self {
        let d = Self::default();
        let empty = Map::new();
        let src = src.unwrap_or(&empty);
        let hex_list = |key: &str, len: usize| -> Option<Vec<String>> {
            let arr = src.get(key)?.as_array()?;
            (arr.len() == len && arr.iter().all(|v| v.as_str().is_some_and(is_hex)))
                .then(|| arr.iter().map(|v| v.as_str().unwrap().to_string()).collect())
        };
        let grads = src.get("gradBg").and_then(Value::as_array).and_then(|arr| {
            if arr.len() != 4 {
                return None;
            }
            arr.iter()
                .map(|g| {
                    let a = g.get("a")?.as_str().filter(|s| is_hex(s))?;
                    let b = g.get("b")?.as_str().filter(|s| is_hex(s))?;
                    Some(Gradient { a: a.into(), b: b.into() })
                })
                .collect::<Option<Vec<_>>>()
        });
        Self {
            palette: hex_list("palette", 6).unwrap_or(d.palette),
            default_idx: src
                .get("defaultIdx")
                .and_then(as_integer)
                .filter(|i| (0..6).contains(i))
                .map(|i| i as usize)
                .unwrap_or(d.default_idx),
            solid_bg: hex_list("solidBg", 4).unwrap_or(d.solid_bg),
            grad_bg: grads.unwrap_or(d.grad_bg),
            font: src
                .get("font")
                .and_then(Value::as_str)
                .filter(|f| !f.trim().is_empty() && f.encode_utf16().count() <= 120)
                .map(str::to_string)
                .unwrap_or(d.font),
            wm_mode: if src.get("wmMode").and_then(Value::as_str) == Some("custom") {
                WmMode::Custom
            } else {
                WmMode::Default
            },
            wm_text: src
                .get("wmText")
                .and_then(Value::as_str)
                .map(|t| truncate_chars(t, 120))
                .unwrap_or(d.wm_text),
            wm_color: src
                .get("wmColor")
                .and_then(Value::as_str)
                .filter(|s| is_hex(s))
                .map(str::to_string)
                .unwrap_or(d.wm_color),
            wm_bg: src
                .get("wmBg")
                .and_then(Value::as_str)
                .filter(|s| is_hex(s))
                .map(str::to_string)
                .unwrap_or(d.wm_bg),
            advanced: Advanced::sanitize(src.get("advanced")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(flatten)]
    pub kit: Kit,
    pub presets: Vec<Preset>,
}

impl Profile {
    pub fn new(name: &str, kit: Kit, presets: Vec<Preset>) -> Self {
        Self {
            id: new_id("prof"),
            name: clean_name(name, "Untitled"),
            created_at: now_iso(),
            updated_at: None,
            kit,
            presets,
        }
    }

    pub fn sanitize(p: &Value) -> Option<Self> {
        let o = p.as_object()?;
        let name = o.get("name")?.as_str()?;
        Some(Self {
            id: valid_id(o.get("id")).unwrap_or_else(|| new_id("prof")),
            name: clean_name(name, "Untitled"),
            created_at: o
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(now_iso),
            updated_at: o.get("updatedAt").and_then(Value::as_str).map(str::to_string),
            kit: Kit::sanitize(Some(o)),
            presets: sanitize_presets(o.get("presets")),
        })
    }
}

fn sanitize_presets(v: Option<&Value>) -> Vec<Preset> {
    v.and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Preset::sanitize).collect())
        .unwrap_or_default()
}

/* ==========================================================================
   Settings
   ========================================================================== */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

/// The current screenshot style — what a preset captures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub watermark: bool,
    /// fill: 0 none, 1-4 solid, 5-8 gradient (the profile's fills)
    pub backdrop: u8,
    /// margin, % of the output width
    pub pad: u32,
    /// output aspect ratio id
    pub ratio: String,
    /// export scale when no explicit width is set
    pub out_scale: f64,
    /// explicit export width, or null to follow outScale
    pub out_w: Option<u32>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            watermark: true,
            backdrop: 0,
            pad: 0,
            ratio: "orig".into(),
            out_scale: 1.0,
            out_w: None,
        }
    }
}

impl Style {
    pub fn values(&self) -> StyleValues {
        StyleValues {
            ratio: Some(self.ratio.clone()),
            out_scale: Some(self.out_scale),
            out_w: self.out_w,
            pad: Some(self.pad),
            backdrop: Some(self.backdrop),
            watermark: Some(self.watermark),
        }
    }

    /// Applies every field the (already sanitized) values define.
    pub fn apply(&mut self, v: &StyleValues) {
        if let Some(r) = &v.ratio {
            self.ratio = r.clone();
        }
        if let Some(k) = v.out_scale {
            self.out_scale = k;
        }
        self.out_w = v.out_w;
        if let Some(p) = v.pad {
            self.pad = p;
        }
        if let Some(b) = v.backdrop {
            self.backdrop = b;
        }
        if let Some(w) = v.watermark {
            self.watermark = w;
        }
    }

    pub fn ratio(&self) -> &'static Ratio {
        RATIOS.iter().find(|r| r.id == self.ratio).unwrap_or(&RATIOS[0])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    #[serde(flatten)]
    pub style: Style,
    pub profiles: Vec<Profile>,
    pub active_profile: String,
    pub active_preset: Option<String>,
}

impl Settings {
    /// Builds settings from whatever was stored, checking each field.
    /// Settings saved before profiles existed kept the palette, fills, font and
    /// presets at the top level — those are folded into one profile.
    pub fn from_value(raw: &Value) -> Self {
        let empty = Map::new();
        let o = raw.as_object().unwrap_or(&empty);

        let mut profiles: Vec<Profile> = o
            .get("profiles")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Profile::sanitize).collect())
            .unwrap_or_default();
        if profiles.is_empty() {
            profiles.push(Profile::new("Default", Kit::sanitize(Some(o)), sanitize_presets(o.get("presets"))));
        }
        let active_profile = o
            .get("activeProfile")
            .and_then(Value::as_str)
            .filter(|id| profiles.iter().any(|p| p.id == *id))
            .map(str::to_string)
            .unwrap_or_else(|| profiles[0].id.clone());

        // The current style, checked the way an imported preset is.
        let checked = StyleValues::sanitize(Some(raw));
        let d = Style::default();
        let style = Style {
            watermark: checked.watermark.unwrap_or(d.watermark),
            backdrop: checked.backdrop.unwrap_or(d.backdrop),
            pad: checked.pad.unwrap_or(d.pad),
            ratio: checked.ratio.unwrap_or(d.ratio),
            out_scale: checked.out_scale.unwrap_or(d.out_scale),
            out_w: checked.out_w,
        };

        Self {
            theme: if o.get("theme").and_then(Value::as_str) == Some("light") {
                Theme::Light
            } else {
                Theme::Dark
            },
            style,
            profiles,
            active_profile,
            active_preset: o.get("activePreset").and_then(Value::as_str).map(str::to_string),
        }
    }

    pub fn profile(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile)
            .unwrap_or(&self.profiles[0])
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        let idx = self.profiles.iter().position(|p| p.id == self.active_profile).unwrap_or(0);
        &mut self.profiles[idx]
    }

    pub fn kit(&self) -> &Kit {
        &self.profile().kit
    }

    pub fn kit_mut(&mut self) -> &mut Kit {
        &mut self.profile_mut().kit
    }

    pub fn active_preset(&self) -> Option<&Preset> {
        let id = self.active_preset.as_deref()?;
        self.profile().presets.iter().find(|p| p.id == id)
    }

    /// True when the current style differs from the preset in any field the
    /// preset defines (an imported preset may leave some out).
    pub fn preset_is_modified(&self, preset: &Preset) -> bool {
        let want = &preset.values;
        let have = self.style.values();
        want.ratio.as_ref().is_some_and(|r| Some(r) != have.ratio.as_ref())
            || want.out_scale.is_some_and(|k| Some(k) != have.out_scale)
            || want.out_w != have.out_w
            || want.pad.is_some_and(|p| Some(p) != have.pad)
            || want.backdrop.is_some_and(|b| Some(b) != have.backdrop)
            || want.watermark.is_some_and(|w| Some(w) != have.watermark)
    }

    /* --- Presets ----------------------------------------------------------- */

    pub fn apply_preset(&mut self, id: &str) -> Option<String> {
        let preset = self.profile().presets.iter().find(|p| p.id == id)?.clone();
        self.style.apply(&preset.values);
        self.active_preset = Some(preset.id);
        Some(preset.name)
    }

    pub fn save_preset(&mut self, name: &str) -> Option<String> {
        let name = clean_name(name, "");
        if name.is_empty() {
            return None;
        }
        let preset = Preset {
            id: new_id("p"),
            name: name.clone(),
            created_at: now_iso(),
            updated_at: None,
            values: self.style.values(),
        };
        self.active_preset = Some(preset.id.clone());
        self.profile_mut().presets.push(preset);
        Some(name)
    }

    pub fn update_preset(&mut self, id: &str) -> Option<String> {
        let values = self.style.values();
        let preset = self.profile_mut().presets.iter_mut().find(|p| p.id == id)?;
        preset.values = values;
        preset.updated_at = Some(now_iso());
        let name = preset.name.clone();
        self.active_preset = Some(id.to_string());
        Some(name)
    }

    pub fn delete_preset(&mut self, id: &str) {
        self.profile_mut().presets.retain(|p| p.id != id);
        if self.active_preset.as_deref() == Some(id) {
            self.active_preset = None;
        }
    }

    /* --- Profiles ---------------------------------------------------------- */

    pub fn set_active_profile(&mut self, id: &str) -> Option<String> {
        let profile = self.profiles.iter().find(|p| p.id == id)?;
        if profile.id == self.active_profile {
            return None;
        }
        let name = profile.name.clone();
        self.active_profile = id.to_string();
        Some(name)
    }

    /// A new profile, from stock or as a copy of `from`, made active.
    pub fn create_profile(&mut self, name: &str, from: Option<&str>) -> String {
        let source = from.and_then(|id| self.profiles.iter().find(|p| p.id == id)).cloned();
        let profile = match source {
            // A copy gets its own presets, not shared ids with the original.
            Some(src) => Profile::new(
                name,
                src.kit.clone(),
                src.presets
                    .iter()
                    .map(|p| Preset { id: new_id("p"), ..p.clone() })
                    .collect(),
            ),
            None => Profile::new(name, Kit::default(), Vec::new()),
        };
        let name = profile.name.clone();
        self.active_profile = profile.id.clone();
        self.profiles.push(profile);
        name
    }

    pub fn rename_profile(&mut self, id: &str, name: &str) -> bool {
        let next = clean_name(name, "");
        if next.is_empty() {
            return false;
        }
        let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        p.name = next;
        p.updated_at = Some(now_iso());
        true
    }

    /// Fails (returns None) when it is the last profile.
    pub fn delete_profile(&mut self, id: &str) -> Option<String> {
        if self.profiles.len() <= 1 {
            return None;
        }
        let name = self.profiles.iter().find(|p| p.id == id)?.name.clone();
        self.profiles.retain(|p| p.id != id);
        if self.active_profile == id {
            self.active_profile = self.profiles[0].id.clone();
        }
        Some(name)
    }

    /// The active profile's kit goes back to stock; its presets stay.
    pub fn restore_defaults(&mut self) {
        self.profile_mut().kit = Kit::default();
    }

    /* --- Files ------------------------------------------------------------- */

    pub fn presets_file(list: &[Preset]) -> String {
        let presets: Vec<Value> = list
            .iter()
            .map(|p| {
                let mut o = Map::new();
                o.insert("id".into(), json!(p.id));
                o.insert("name".into(), json!(p.name));
                o.insert("createdAt".into(), json!(p.created_at));
                if let Some(u) = &p.updated_at {
                    o.insert("updatedAt".into(), json!(u));
                }
                o.insert("values".into(), serde_json::to_value(&p.values).unwrap_or(Value::Null));
                Value::Object(o)
            })
            .collect();
        pretty(&json!({
            "app": "escribo",
            "kind": PRESET_FILE_KIND,
            "version": 1,
            "exportedAt": now_iso(),
            "presets": presets,
        }))
    }

    pub fn profile_file(profile: &Profile) -> String {
        pretty(&json!({
            "app": "escribo",
            "kind": PROFILE_FILE_KIND,
            "version": 1,
            "exportedAt": now_iso(),
            "profile": profile,
        }))
    }

    /// Reads any escribo file: a profile (with its presets) or a list of
    /// presets. A profile is added and switched to; presets join the active
    /// profile. Returns the message to show.
    pub fn import_text(&mut self, text: &str) -> ImportOutcome {
        let Ok(data) = serde_json::from_str::<Value>(text) else {
            return ImportOutcome::failed("That file is not valid JSON");
        };
        let is_profile = data.get("kind").and_then(Value::as_str) == Some(PROFILE_FILE_KIND)
            || data.get("profile").is_some_and(Value::is_object);
        if data.is_object() && is_profile {
            return self.import_profile(data.get("profile").unwrap_or(&Value::Null));
        }
        let list = match &data {
            Value::Array(a) => Some(a),
            Value::Object(o) => o.get("presets").and_then(Value::as_array),
            _ => None,
        };
        match list {
            Some(list) => self.import_preset_list(list),
            None => ImportOutcome::failed("No presets or profile found in that file"),
        }
    }

    fn import_profile(&mut self, raw: &Value) -> ImportOutcome {
        let Some(mut clean) = Profile::sanitize(raw) else {
            return ImportOutcome::failed("No usable profile in that file");
        };
        if let Some(existing) = self.profiles.iter_mut().find(|p| p.id == clean.id) {
            *existing = clean.clone();
        } else {
            if self.profiles.iter().any(|p| p.name == clean.name) {
                clean.name.push_str(" (imported)");
            }
            self.profiles.push(clean.clone());
        }
        self.active_profile = clean.id.clone();
        let n = clean.presets.len();
        ImportOutcome {
            changed: true,
            profile_changed: true,
            message: format!("Imported profile “{}” with {n} preset{}", clean.name, plural(n)),
        }
    }

    fn import_preset_list(&mut self, list: &[Value]) -> ImportOutcome {
        let mut count = 0;
        let profile = self.profile_mut();
        for raw in list {
            let Some(mut clean) = Preset::sanitize(raw) else {
                continue;
            };
            if let Some(existing) = profile.presets.iter_mut().find(|x| x.id == clean.id) {
                *existing = clean;
            } else {
                if profile.presets.iter().any(|x| x.name == clean.name) {
                    clean.name.push_str(" (imported)");
                }
                profile.presets.push(clean);
            }
            count += 1;
        }
        let message = if count > 0 {
            format!("Imported {count} preset{} into “{}”", plural(count), profile.name)
        } else {
            "No usable presets in that file".to_string()
        };
        ImportOutcome { changed: true, profile_changed: false, message }
    }

    /* --- Disk -------------------------------------------------------------- */

    pub fn path() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("ESCRIBO_CONFIG_DIR") {
            return Some(PathBuf::from(dir).join("settings.json"));
        }
        Some(dirs::config_dir()?.join("escribo").join("settings.json"))
    }

    pub fn load() -> Self {
        let raw = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .unwrap_or(Value::Null);
        Self::from_value(&raw)
    }

    /// Writes through a temporary file so a crash mid-write never leaves a
    /// truncated settings file behind.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)
    }
}

pub struct ImportOutcome {
    pub changed: bool,
    /// The active profile changed, so everything reading the kit must refresh.
    pub profile_changed: bool,
    pub message: String,
}

impl ImportOutcome {
    fn failed(message: &str) -> Self {
        Self { changed: false, profile_changed: false, message: message.into() }
    }
}

/* ==========================================================================
   Helpers
   ========================================================================== */

pub fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A JSON number as JavaScript's `Number()` would read it from a settings
/// file: numbers and numeric strings.
fn js_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

/// `Number.isInteger` — integral JSON numbers, including `2.0`.
fn as_integer(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    (n.is_finite() && n.fract() == 0.0).then_some(n as i64)
}

/// `Math.round`: halves go up, also for negatives.
fn js_round(n: f64) -> f64 {
    (n + 0.5).floor()
}

pub fn is_hex(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn valid_id(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?;
    let ok = (1..=40).contains(&s.len())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then(|| s.to_string())
}

fn truncate_chars(s: &str, max_utf16: usize) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        n += c.len_utf16();
        if n > max_utf16 {
            break;
        }
        out.push(c);
    }
    out
}

pub fn clean_name(name: &str, fallback: &str) -> String {
    let trimmed = truncate_chars(name.trim(), 60);
    if trimmed.is_empty() { fallback.to_string() } else { trimmed }
}

pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "untitled".into() } else { out }
}

pub fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // A cheap mix of the clock, the process and a counter stands in for the
    // old Math.random() suffix: unique within a run and across machines.
    let mut x = millis
        ^ (std::process::id() as u64).rotate_left(32)
        ^ COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    format!("{prefix}_{}{}", base36(millis), &base36(x)[..6.min(base36(x).len())])
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `new Date().toISOString()`.
pub fn now_iso() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

/// A number the way JavaScript prints it: no trailing `.0`.
pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_flat_settings_fold_into_one_profile() {
        let raw = json!({
            "theme": "light",
            "palette": ["#000000", "#111111", "#222222", "#333333", "#444444", "#555555"],
            "defaultIdx": 3,
            "presets": [{ "id": "p_1", "name": "Slides", "values": { "ratio": "16:9", "pad": 6 } }],
            "pad": 4,
            "ratio": "1:1"
        });
        let s = Settings::from_value(&raw);
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.profiles.len(), 1);
        assert_eq!(s.profile().name, "Default");
        assert_eq!(s.kit().palette[1], "#111111");
        assert_eq!(s.kit().default_idx, 3);
        assert_eq!(s.profile().presets[0].values.ratio.as_deref(), Some("16:9"));
        assert_eq!(s.style.pad, 4);
        assert_eq!(s.style.ratio, "1:1");
    }

    #[test]
    fn bad_fields_fall_back_to_defaults() {
        let raw = json!({
            "profiles": [{ "id": "prof_a", "name": "A", "palette": ["#zzz"], "solidBg": 3,
                           "advanced": { "shadowBlur": 500, "cornerRadius": "2.5" } }],
            "activeProfile": "missing",
            "pad": 99, "backdrop": 12, "outScale": 3, "outW": 20
        });
        let s = Settings::from_value(&raw);
        assert_eq!(s.active_profile, "prof_a");
        assert_eq!(s.kit().palette, Kit::default().palette);
        assert_eq!(s.kit().advanced.shadow_blur, 80.0);
        assert_eq!(s.kit().advanced.corner_radius, 2.5);
        assert_eq!(s.style.pad, 18);
        assert_eq!(s.style.backdrop, 0);
        assert_eq!(s.style.out_scale, 1.0);
        assert_eq!(s.style.out_w, Some(240));
    }

    #[test]
    fn round_trips_through_disk_format() {
        let mut s = Settings::from_value(&Value::Null);
        s.save_preset("Docs").unwrap();
        s.create_profile("Team", None);
        let text = serde_json::to_string(&s).unwrap();
        let back = Settings::from_value(&serde_json::from_str(&text).unwrap());
        assert_eq!(back, s);
        // The kit is flattened into each profile, as electron-store had it.
        let v: Value = serde_json::from_str(&text).unwrap();
        assert!(v["profiles"][0]["palette"].is_array());
        assert!(v["profiles"][0]["presets"].is_array());
        assert!(v.get("palette").is_none());
    }

    #[test]
    fn preset_modified_tracks_defined_fields_only() {
        let mut s = Settings::from_value(&Value::Null);
        let id = {
            s.save_preset("A").unwrap();
            s.active_preset.clone().unwrap()
        };
        assert!(!s.preset_is_modified(s.active_preset().unwrap()));
        s.style.pad = 10;
        assert!(s.preset_is_modified(s.active_preset().unwrap()));
        s.apply_preset(&id);
        assert!(!s.preset_is_modified(s.active_preset().unwrap()));
    }

    #[test]
    fn imports_profiles_and_presets() {
        let mut s = Settings::from_value(&Value::Null);
        let profile = s.profile().clone();
        let out = s.import_text(&Settings::profile_file(&profile));
        assert!(out.message.starts_with("Imported profile"));
        assert_eq!(s.profiles.len(), 1, "same id replaces");

        let presets = json!([{ "name": "One", "values": { "pad": 3 } }, { "nope": 1 }]);
        let out = s.import_text(&presets.to_string());
        assert_eq!(out.message, "Imported 1 preset into “Default”");
        let out = s.import_text(&presets.to_string());
        assert_eq!(out.message, "Imported 1 preset into “Default”");
        assert_eq!(s.profile().presets[1].name, "One (imported)");
        assert_eq!(s.import_text("{").message, "That file is not valid JSON");
    }

    #[test]
    fn profile_lifecycle() {
        let mut s = Settings::from_value(&Value::Null);
        assert!(s.delete_profile(&s.active_profile.clone()).is_none());
        s.save_preset("Keep").unwrap();
        let first = s.active_profile.clone();
        s.create_profile("Copy", Some(&first));
        assert_eq!(s.profile().presets.len(), 1);
        assert_ne!(s.profile().presets[0].id, s.profiles[0].presets[0].id);
        assert!(s.rename_profile(&first, "  Renamed  "));
        assert_eq!(s.profiles[0].name, "Renamed");
        let active = s.active_profile.clone();
        assert_eq!(s.delete_profile(&active).as_deref(), Some("Copy"));
        assert_eq!(s.active_profile, first);
    }

    #[test]
    fn helpers_match_javascript() {
        assert_eq!(slug("My Team's Style!"), "my-team-s-style");
        assert_eq!(slug("!!!"), "untitled");
        assert_eq!(clean_name("   ", "x"), "x");
        assert_eq!(format_advanced(1.25, "%"), "1.3%");
        assert_eq!(format_advanced(34.0, "px"), "34px");
        assert_eq!(now_iso().len(), 24);
        assert!(new_id("p").starts_with("p_"));
        assert_ne!(new_id("p"), new_id("p"));
    }
}
