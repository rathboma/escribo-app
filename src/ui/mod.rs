//! The editor window: one screenshot, its annotations, and every control
//! around them.

mod chrome;
mod popovers;
mod prefs;
pub mod prompt;
mod stage;
mod widgets;

pub use stage::StageGeom;

use crate::app::{self, SettingsModel, Shared, Source};
use crate::doc::{Document, Id, Rgb, Shape, Tool, rgb};
use crate::geom::{Rect, V2, v2};
use crate::render;
use crate::settings::{self, Settings};
use crate::text::TextEngine;
use crate::theme::Theme;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, Styled, Subscription, Task, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_ce_elements::editable_text::{EditableTextState, StringStorage, TextChanged};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub const AREA_PAD: f32 = 22.0;
pub const ZOOM_STEP: f32 = 1.25;
pub const MAX_ZOOM_STEPS: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Popover {
    Backdrop,
    Ratio,
    Copy,
    Presets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefsTab {
    Annotate,
    Backdrop,
    Advanced,
    Watermark,
    Profiles,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProfileForm {
    New,
    Rename(String),
    Duplicate(String),
}

/// Which setting a colour picker edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTarget {
    Palette(usize),
    Solid(usize),
    GradA(usize),
    GradB(usize),
    WmColor,
    WmBg,
}

pub struct ColorPicker {
    pub target: ColorTarget,
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub anchor: gpui::Point<gpui::Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashTarget {
    Copy,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    PresetName,
    ProfileName,
    OutW,
    Font,
    WmText,
    Hex,
}

/// The text fields, kept alive for the window's lifetime.
pub struct Inputs {
    pub preset_name: Entity<EditableTextState>,
    pub profile_name: Entity<EditableTextState>,
    pub out_w: Entity<EditableTextState>,
    pub font: Entity<EditableTextState>,
    pub wm_text: Entity<EditableTextState>,
    pub hex: Entity<EditableTextState>,
}

impl Inputs {
    pub fn get(&self, f: Field) -> &Entity<EditableTextState> {
        match f {
            Field::PresetName => &self.preset_name,
            Field::ProfileName => &self.profile_name,
            Field::OutW => &self.out_w,
            Field::Font => &self.font,
            Field::WmText => &self.wm_text,
            Field::Hex => &self.hex,
        }
    }
}

pub struct Editor {
    pub(crate) focus: FocusHandle,
    pub(crate) settings: Entity<SettingsModel>,
    pub(crate) text: Rc<RefCell<TextEngine>>,
    pub(crate) doc: Option<Document>,
    pub(crate) loading: bool,

    pub(crate) tool: Tool,
    pub(crate) color_idx: usize,
    pub(crate) zoom: i32,
    pub(crate) pending_emoji: Option<&'static str>,
    pub(crate) selection: Vec<Id>,
    pub(crate) edit: Option<stage::TextEdit>,
    pub(crate) drag: Option<stage::Drag>,
    pub(crate) hover: stage::Hover,
    pub(crate) crop: Option<Rect>,
    pub(crate) scroll: V2,

    pub(crate) popover: Option<Popover>,
    /// The mouse-down a popover was closed by clicking outside it, so the
    /// same click on its own button does not reopen it.
    pub(crate) popover_closed_by: Option<(Popover, u64)>,
    pub(crate) mouse_seq: u64,
    pub(crate) emoji_open: bool,
    pub(crate) prefs: Option<PrefsTab>,
    pub(crate) profile_form: Option<ProfileForm>,
    pub(crate) preset_form: bool,
    pub(crate) color_picker: Option<ColorPicker>,

    pub(crate) toast: Option<SharedString>,
    toast_task: Option<Task<()>>,
    pub(crate) flash: Option<(FlashTarget, &'static str)>,
    flash_task: Option<Task<()>>,
    pub(crate) file_hover: bool,
    pub(crate) busy: bool,

    pub(crate) geom: Option<StageGeom>,
    pub(crate) gpu: stage::GpuCache,
    pub(crate) sliders: std::collections::HashMap<widgets::SliderId, gpui::Bounds<gpui::Pixels>>,
    pub(crate) inputs: Inputs,
    pub(crate) focused_field: Option<Field>,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn new_input(cx: &mut Context<Editor>) -> Entity<EditableTextState> {
    cx.new(|cx| EditableTextState::new(StringStorage::default(), cx))
}

impl Editor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shared = Shared::get(cx);
        let settings = shared.settings.clone();
        let text = shared.text.clone();
        let inputs = Inputs {
            preset_name: new_input(cx),
            profile_name: new_input(cx),
            out_w: new_input(cx),
            font: new_input(cx),
            wm_text: new_input(cx),
            hex: new_input(cx),
        };
        let mut subs = vec![cx.observe(&settings, |this, _, cx| {
            this.sync_inputs(cx);
            cx.notify();
        })];
        // Watermark text applies as it is typed, like the old `input` event.
        subs.push(cx.subscribe(&inputs.wm_text, |this, state, _: &TextChanged, cx| {
            let value: String = state.read(cx).as_str().chars().take(120).collect();
            this.update_settings(cx, |s| s.kit_mut().wm_text = value);
        }));
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let color_idx = settings.read(cx).settings.kit().default_idx;
        let mut editor = Self {
            focus,
            settings,
            text,
            doc: None,
            loading: false,
            tool: Tool::Arrow,
            color_idx,
            zoom: 0,
            pending_emoji: None,
            selection: Vec::new(),
            edit: None,
            drag: None,
            hover: stage::Hover::default(),
            crop: None,
            scroll: V2::ZERO,
            popover: None,
            popover_closed_by: None,
            mouse_seq: 0,
            emoji_open: false,
            prefs: None,
            profile_form: None,
            preset_form: false,
            color_picker: None,
            toast: None,
            toast_task: None,
            flash: None,
            flash_task: None,
            file_hover: false,
            busy: false,
            geom: None,
            gpu: stage::GpuCache::default(),
            sliders: Default::default(),
            inputs,
            focused_field: None,
            _subscriptions: subs,
        };
        editor.sync_inputs(cx);
        editor.track_fields(window, cx);
        editor
    }

    /// Knows which field has the keyboard, so settings changes never rewrite
    /// a field mid-typing; and commits the fields that apply on leaving, as
    /// the old `change` events did.
    fn track_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for field in [Field::PresetName, Field::ProfileName, Field::OutW, Field::Font, Field::WmText, Field::Hex] {
            let handle = self.inputs.get(field).read(cx).focus_handle(cx);
            self._subscriptions.push(cx.on_focus(&handle, window, move |this, _, cx| {
                this.focused_field = Some(field);
                cx.notify();
            }));
            self._subscriptions.push(cx.on_blur(&handle, window, move |this, _, cx| {
                if this.focused_field == Some(field) {
                    this.focused_field = None;
                }
                match field {
                    Field::OutW => this.commit_out_w(cx),
                    Field::Font => this.commit_font(cx),
                    Field::Hex => this.commit_hex(cx),
                    _ => {}
                }
                cx.notify();
            }));
        }
    }

    /* --- Settings ---------------------------------------------------------- */

    pub(crate) fn s<'a>(&self, cx: &'a App) -> &'a Settings {
        &self.settings.read(cx).settings
    }

    pub(crate) fn update_settings(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        self.settings.update(cx, |m, cx| m.update(cx, f));
    }

    pub(crate) fn theme(&self, cx: &App) -> Theme {
        Theme::for_settings(self.s(cx).theme)
    }

    /// Re-fills text fields from settings, except the one being typed in.
    fn sync_inputs(&mut self, cx: &mut Context<Self>) {
        let s = self.s(cx).clone();
        let known = settings::FONTS.iter().any(|f| f.id == s.kit().font);
        let focused = self.focused_field;
        set_input_unless_focused(
            &self.inputs.font,
            if known { "" } else { &s.kit().font },
            focused == Some(Field::Font),
            cx,
        );
        set_input_unless_focused(&self.inputs.wm_text, &s.kit().wm_text, focused == Some(Field::WmText), cx);
        let w = self.doc.as_ref().map(|d| {
            let l = render::layout_for(&s, &d.shot);
            render::export_size(&s, &l).0.to_string()
        });
        set_input_unless_focused(&self.inputs.out_w, w.as_deref().unwrap_or(""), focused == Some(Field::OutW), cx);
    }

    /// Re-reads everything that follows the active profile: the armed colour
    /// goes back to the profile's default.
    pub(crate) fn profile_changed(&mut self, cx: &mut Context<Self>) {
        self.color_idx = self.s(cx).kit().default_idx;
        self.sync_inputs(cx);
        cx.notify();
    }

    /* --- Loading ----------------------------------------------------------- */

    pub fn open_source(&mut self, source: Source, window: &mut Window, cx: &mut Context<Self>) {
        match source {
            Source::Empty => {}
            Source::Path(path) => self.load_path(path, window, cx),
            Source::Clipboard => self.paste(window, cx),
        }
    }

    pub(crate) fn load_path(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = app::display_name(&path);
        self.loading = true;
        cx.notify();
        let task = cx.background_executor().spawn(async move { app::read_image_file(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok(shot) => this.set_doc(Document::new(shot, name), window, cx),
                    Err(err) => {
                        log::warn!("{err:#}");
                        this.show_toast("Could not read that image", cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn load_bytes(&mut self, bytes: Vec<u8>, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();
        let task = cx.background_executor().spawn(async move { app::decode_image(&bytes) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok(shot) => this.set_doc(Document::new(shot, name), window, cx),
                    Err(_) => this.show_toast("Could not read that image", cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// Ctrl+V and the tray's "Paste from clipboard".
    pub(crate) fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match app::clipboard_image(cx) {
            Some(app::ClipboardImage::Bytes(bytes)) => self.load_bytes(bytes, "Clipboard image".into(), window, cx),
            Some(app::ClipboardImage::File(path)) => self.load_path(path, window, cx),
            None => self.show_toast("No image on the clipboard", cx),
        }
    }

    pub(crate) fn set_doc(&mut self, doc: Document, window: &mut Window, cx: &mut Context<Self>) {
        window.set_window_title(&format!("{} — escribo", doc.name));
        self.gpu.reset(window);
        self.doc = Some(doc);
        self.selection.clear();
        self.edit = None;
        self.drag = None;
        self.crop = None;
        self.zoom = 0;
        self.scroll = V2::ZERO;
        let tool = if self.tool == Tool::Crop { Tool::Arrow } else { self.tool };
        self.set_tool(tool, cx);
        self.sync_inputs(cx);
        cx.notify();
    }

    pub fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let picked = app::open_with_dialog(cx, "Choose an image").await;
            this.update_in(cx, |this, window, cx| match picked {
                Ok(Some(path)) => this.load_path(path, window, cx),
                Ok(None) => {}
                Err(err) => {
                    log::warn!("{err:#}");
                    this.show_toast("Could not open the file dialog", cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /* --- Tools ------------------------------------------------------------- */

    pub(crate) fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.tool == Tool::Crop && tool != Tool::Crop {
            self.crop = None;
        }
        self.tool = tool;
        if tool != Tool::Emoji {
            self.pending_emoji = None;
        }
        if tool != Tool::Select && self.edit.is_none() {
            self.selection.clear();
        }
        self.emoji_open = tool == Tool::Emoji;
        if tool == Tool::Crop {
            self.enter_crop();
        }
        // Reading the screenshot back costs a few milliseconds; spend them on
        // the click rather than on the first drag.
        if tool == Tool::Redact {
            if let Some(doc) = &self.doc {
                let shot = doc.shot.clone();
                cx.background_executor().spawn(async move { shot.plate().is_some() }).detach();
            }
        }
        cx.notify();
    }

    pub(crate) fn hint(&self) -> String {
        if self.doc.is_none() {
            return "Drop a screenshot here, or paste with Ctrl+V".into();
        }
        if self.edit.is_some() {
            return "Type the label — Esc or Ctrl+Enter when done".into();
        }
        if self.tool == Tool::Emoji {
            if let Some(e) = self.pending_emoji {
                return format!("Click to place {e}");
            }
        }
        self.tool.hint().into()
    }

    pub(crate) fn color(&self, cx: &App) -> Rgb {
        let kit = self.s(cx).kit();
        rgb(kit.palette.get(self.color_idx).unwrap_or(&kit.palette[0]))
    }

    /// One "unit" scaled by the profile's annotation size.
    pub(crate) fn annotation_unit(&self, cx: &App) -> f32 {
        let u = self.doc.as_ref().map(|d| d.shot.unit()).unwrap_or(1.0);
        u * (self.s(cx).kit().advanced.annotation_scale as f32 / 100.0)
    }

    pub(crate) fn pick_color(&mut self, i: usize, cx: &mut Context<Self>) {
        self.color_idx = i;
        let color = self.color(cx);
        let alpha = self.s(cx).kit().advanced.highlight_opacity as f32 / 100.0;
        if let Some(doc) = &mut self.doc {
            let mut changed = false;
            for id in &self.selection {
                if let Some(a) = doc.get_mut(*id) {
                    a.shape.set_color(color, alpha);
                    changed = true;
                }
            }
            if changed {
                doc.commit();
            }
        }
        cx.notify();
    }

    pub(crate) fn set_font(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = if id.trim().is_empty() { "system".to_string() } else { id.to_string() };
        self.update_settings(cx, |s| s.kit_mut().font = id.clone());
        // Like the swatches: a selected label takes the new font straight away.
        if let Some(doc) = &mut self.doc {
            let mut changed = false;
            for sel in &self.selection {
                if let Some(a) = doc.get_mut(*sel) {
                    if let Shape::Text(l) = &mut a.shape {
                        l.font = id.clone();
                        changed = true;
                    }
                }
            }
            if changed {
                doc.commit();
            }
        }
        cx.notify();
    }

    fn commit_font(&mut self, cx: &mut Context<Self>) {
        let value = self.inputs.font.read(cx).as_str().trim().to_string();
        let current = self.s(cx).kit().font.clone();
        let known = settings::FONTS.iter().any(|f| f.id == current);
        if value.is_empty() && known {
            return;
        }
        if value != current {
            self.set_font(if value.is_empty() { "system" } else { &value }, cx);
        }
    }

    fn commit_out_w(&mut self, cx: &mut Context<Self>) {
        let raw = self.inputs.out_w.read(cx).as_str().trim().to_string();
        let Ok(n) = raw.parse::<f64>() else {
            self.sync_inputs(cx);
            return;
        };
        let w = (n.round() as i64).clamp(settings::OUT_W_MIN as i64, settings::OUT_W_MAX as i64) as u32;
        self.update_settings(cx, |s| s.style.out_w = Some(w));
        self.sync_inputs(cx);
    }

    /* --- History ----------------------------------------------------------- */

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        if let Some(doc) = &mut self.doc {
            if doc.undo() {
                self.selection.clear();
                cx.notify();
            }
        }
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        if let Some(doc) = &mut self.doc {
            if doc.redo() {
                self.selection.clear();
                cx.notify();
            }
        }
    }

    pub(crate) fn delete_selection(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        if self.selection.is_empty() {
            return;
        }
        doc.remove(&self.selection);
        doc.commit();
        self.selection.clear();
        cx.notify();
    }

    pub(crate) fn duplicate_selection(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        let d = 12.0 * doc.shot.unit();
        let copies: Vec<Shape> = self
            .selection
            .iter()
            .filter_map(|id| doc.get(*id))
            .map(|a| {
                let mut s = a.shape.clone();
                s.translate(v2(d, d));
                s
            })
            .collect();
        if copies.is_empty() {
            return;
        }
        self.selection = copies.into_iter().map(|s| doc.add(s)).collect();
        doc.commit();
        self.tool = Tool::Select;
        cx.notify();
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        self.set_tool(Tool::Select, cx);
        if let Some(doc) = &self.doc {
            self.selection = doc.annotations.iter().map(|a| a.id).collect();
        }
        cx.notify();
    }

    /* --- Zoom & theme ------------------------------------------------------ */

    pub(crate) fn set_zoom(&mut self, step: i32, cx: &mut Context<Self>) {
        self.zoom = step.clamp(0, MAX_ZOOM_STEPS);
        cx.notify();
    }

    pub(crate) fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.update_settings(cx, |s| {
            s.theme = match s.theme {
                settings::Theme::Dark => settings::Theme::Light,
                settings::Theme::Light => settings::Theme::Dark,
            }
        });
    }

    /* --- Feedback ---------------------------------------------------------- */

    pub(crate) fn show_toast(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(message.into());
        self.toast_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1800)).await;
            this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn flash(&mut self, target: FlashTarget, text: &'static str, cx: &mut Context<Self>) {
        self.flash = Some((target, text));
        self.flash_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1400)).await;
            this.update(cx, |this, cx| {
                this.flash = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /* --- Export ------------------------------------------------------------ */

    /// Renders the export off the main thread: (width, height, straight RGBA).
    fn render_export(&mut self, cx: &mut Context<Self>) -> Option<Task<anyhow::Result<(u32, u32, Vec<u8>)>>> {
        self.finish_edit(cx);
        let doc = self.doc.as_ref()?;
        let snapshot = doc.snapshot();
        let settings = self.s(cx).clone();
        Some(cx.background_executor().spawn(async move {
            let mut text = app::export_text_engine().lock().map_err(|_| anyhow::anyhow!("text engine poisoned"))?;
            render::compose(&snapshot, &settings, &mut text).ok_or_else(|| anyhow::anyhow!("nothing to render"))
        }))
    }

    pub(crate) fn copy_image(&mut self, close_after: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.popover = None;
        let Some(task) = self.render_export(cx) else {
            self.show_toast("Nothing to copy yet", cx);
            return;
        };
        self.busy = true;
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = match task.await {
                Ok((w, h, rgba)) => executor.spawn(async move { app::copy_image(w, h, rgba) }).await,
                Err(err) => Err(err),
            };
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        this.flash(FlashTarget::Copy, if close_after { "Closing…" } else { "Copied" }, cx);
                        this.show_toast("Copied to clipboard", cx);
                        if close_after {
                            cx.spawn_in(window, async move |_, cx| {
                                cx.background_executor().timer(Duration::from_millis(350)).await;
                                cx.update(|window, _| window.remove_window()).ok();
                            })
                            .detach();
                        }
                    }
                    Err(err) => {
                        log::error!("clipboard write failed: {err:#}");
                        this.show_toast("Could not copy the image", cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn save_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.render_export(cx) else {
            self.show_toast("Nothing to save yet", cx);
            return;
        };
        let name = render::export_name(self.doc.as_ref().map(|d| d.name.as_str()).unwrap_or("escribo"));
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let png = match task.await {
                Ok((w, h, rgba)) => executor.spawn(async move { crate::raster::encode_png(w, h, &rgba) }).await,
                Err(err) => Err(err),
            };
            let result = match png {
                Ok(png) => app::save_or_fallback(cx, app::pictures_dir(), name, png).await,
                Err(err) => Err(err),
            };
            this.update(cx, |this, cx| match result {
                Ok(Some((path, dialog))) => {
                    this.flash(FlashTarget::Save, "Saved", cx);
                    if dialog {
                        this.show_toast("Saved", cx);
                    } else {
                        this.show_toast(format!("Saved to {}", app::tilde(&path)), cx);
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    log::error!("save failed: {err:#}");
                    this.show_toast("Could not save the image", cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /* --- Keyboard ---------------------------------------------------------- */

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        let key = ks.key.as_str();
        let m = ks.modifiers;
        let mod_key = m.control || m.platform;

        if self.edit.is_some() && self.focus.is_focused(window) {
            if self.edit_key(event, window, cx) {
                cx.stop_propagation();
            }
            return;
        }

        if key == "escape" {
            if self.color_picker.is_some() {
                self.color_picker = None;
            } else if self.prefs.is_some() {
                self.close_prefs(cx);
            } else if self.popover.is_some() {
                self.popover = None;
            } else if matches!(self.tool, Tool::Crop | Tool::Emoji) {
                self.set_tool(Tool::Select, cx);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if mod_key && key == "," {
            self.open_prefs(None, cx);
            cx.stop_propagation();
            return;
        }

        // A text field or the preferences have the keyboard.
        if !self.focus.is_focused(window) || self.prefs.is_some() {
            return;
        }

        let handled = if mod_key {
            match key {
                "c" if m.shift => {
                    self.copy_image(true, window, cx);
                    true
                }
                "c" => {
                    self.copy_image(false, window, cx);
                    true
                }
                "s" => {
                    self.save_image(window, cx);
                    true
                }
                "d" => {
                    self.duplicate_selection(cx);
                    true
                }
                "z" if m.shift => {
                    self.redo(cx);
                    true
                }
                "z" => {
                    self.undo(cx);
                    true
                }
                "y" => {
                    self.redo(cx);
                    true
                }
                "a" => {
                    self.select_all(cx);
                    true
                }
                "v" => {
                    self.paste(window, cx);
                    true
                }
                "o" => {
                    self.choose_file(window, cx);
                    true
                }
                "0" => {
                    self.set_zoom(0, cx);
                    true
                }
                "=" | "+" => {
                    self.set_zoom(self.zoom + 1, cx);
                    true
                }
                "-" => {
                    self.set_zoom(self.zoom - 1, cx);
                    true
                }
                _ => false,
            }
        } else if key == "delete" || key == "backspace" {
            self.delete_selection(cx);
            true
        } else if let (Some(tool), true) = (Tool::from_key(key), self.doc.is_some() && !m.alt) {
            self.set_tool(tool, cx);
            true
        } else {
            false
        };
        if handled {
            cx.stop_propagation();
        }
    }

    /* --- Popovers ---------------------------------------------------------- */

    pub(crate) fn toggle_popover(&mut self, which: Popover, cx: &mut Context<Self>) {
        if self.popover_closed_by == Some((which, self.mouse_seq)) {
            self.popover_closed_by = None;
            return;
        }
        self.popover = if self.popover == Some(which) { None } else { Some(which) };
        self.preset_form = false;
        cx.notify();
    }

    pub(crate) fn close_popover_from_outside(&mut self, which: Popover, cx: &mut Context<Self>) {
        if self.popover == Some(which) {
            self.popover = None;
            self.popover_closed_by = Some((which, self.mouse_seq));
            cx.notify();
        }
    }

    pub(crate) fn open_prefs(&mut self, tab: Option<PrefsTab>, cx: &mut Context<Self>) {
        self.popover = None;
        self.prefs = Some(tab.unwrap_or(self.prefs.unwrap_or(PrefsTab::Annotate)));
        self.sync_inputs(cx);
        cx.notify();
    }

    pub(crate) fn close_prefs(&mut self, cx: &mut Context<Self>) {
        self.prefs = None;
        self.profile_form = None;
        self.color_picker = None;
        cx.notify();
    }
}

pub(crate) fn set_input_unless_focused(state: &Entity<EditableTextState>, value: &str, focused: bool, cx: &mut App) {
    if focused || state.read(cx).as_str() == value {
        return;
    }
    state.update(cx, |s, cx| s.emplace(value, cx));
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme(cx);
        let decorations = window.window_decorations();
        let body = div()
            .id("app")
            .key_context("Editor")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .capture_any_mouse_down(cx.listener(|this, _: &gpui::MouseDownEvent, _, _| {
                this.mouse_seq += 1;
            }))
            .on_mouse_move_all({
                let listener = cx.listener(Self::on_pointer_move);
                move |ev: &gpui::MouseMoveEvent,
                      phase: gpui::DispatchPhase,
                      _: &gpui::Hitbox,
                      window: &mut Window,
                      cx: &mut App| {
                    if phase == gpui::DispatchPhase::Bubble {
                        listener(ev, window, cx)
                    }
                }
            })
            .capture_any_mouse_up(cx.listener(Self::on_pointer_up))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .font_family(settings::UI_FONT)
            .child(self.render_titlebar(&theme, window, cx))
            .child(self.render_toolbar(&theme, window, cx))
            .child(self.render_area(&theme, window, cx))
            .child(self.render_statusbar(&theme, window, cx))
            .when_some(self.prefs, |el, tab| el.child(self.render_prefs(tab, &theme, window, cx)))
            .when(self.color_picker.is_some(), |el| el.child(self.render_color_picker(&theme, window, cx)));
        chrome::window_frame(body.into_any_element(), decorations, &theme, window, cx)
    }
}

pub(crate) fn any<E: IntoElement>(e: E) -> AnyElement {
    e.into_any_element()
}
