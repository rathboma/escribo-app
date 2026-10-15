//! The window's frame and bars: the client-side window border, the title
//! bar, the toolbar of tools and actions, and the status bar.

use super::widgets::{Tip, divider, icon, icon_button, switch};
use super::{Editor, FlashTarget, Popover, any};
use crate::doc::{Tool, rgb};
use crate::settings::{self, RATIOS};
use crate::theme::{self, Theme};
use gpui::{
    AnyElement, App, Context, Decorations, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    ResizeRegion, StatefulInteractiveElement, Styled, Tiling, Window, WindowButton, WindowButtonLayout, deferred, div,
    point, prelude::FluentBuilder, px,
};

/// How far the shadow of a client-decorated window reaches: libadwaita's
/// `0 0 14px 5px` shadow has faded out by here.
const SHADOW: f32 = 20.0;
/// Resize handles reach this far out from the window's edge, as GTK's do.
/// Clicks further out in the shadow fall through to whatever is behind.
const RESIZE: f32 = 12.0;
/// libadwaita's window corner radius.
const RADIUS: f32 = 15.0;

/// libadwaita's header bar: 47px tall, padded 6px 7px 7px, with 6px between
/// the buttons at either end and the window controls.
const HEADER_H: f32 = 47.0;
const HEADER_PAD: f32 = 7.0;
const HEADER_GAP: f32 = 6.0;
/// A header bar button, and a window control's click target around its
/// 24px circle.
const BUTTON: f32 = 34.0;
const CONTROL_GAP: f32 = 3.0;

/// The window's decoration this frame. With client-side decorations
/// escribo draws GNOME's frame itself — a shadow, rounded corners and resize
/// handles — dropping each where the window meets a screen edge.
#[derive(Clone, Copy)]
pub(super) struct Frame {
    /// None when the compositor draws the frame.
    tiling: Option<Tiling>,
}

impl Frame {
    pub(super) fn new(decorations: Decorations) -> Self {
        match decorations {
            Decorations::Client { tiling } => Self { tiling: Some(tiling) },
            Decorations::Server => Self { tiling: None },
        }
    }

    /// Insets an element filling the surface to the window inside the
    /// shadow, for overlays drawn outside the frame, like prompts.
    pub(super) fn pad<E: Styled + FluentBuilder>(&self, el: E) -> E {
        let Some(t) = self.tiling else { return el };
        let inset = px(SHADOW);
        el.when(!t.top, |d| d.pt(inset))
            .when(!t.bottom, |d| d.pb(inset))
            .when(!t.left, |d| d.pl(inset))
            .when(!t.right, |d| d.pr(inset))
    }

    /// Rounds an element filling the window to the window's corners, just
    /// inside its outline. gpui doesn't clip children to rounded corners, so
    /// anything that paints into a corner has to be rounded itself.
    pub(super) fn round<E: Styled + FluentBuilder>(&self, el: E) -> E {
        let Some(t) = self.tiling else { return el };
        let r = px(RADIUS - 1.0);
        el.when(!(t.top || t.left), |d| d.rounded_tl(r))
            .when(!(t.top || t.right), |d| d.rounded_tr(r))
            .when(!(t.bottom || t.left), |d| d.rounded_bl(r))
            .when(!(t.bottom || t.right), |d| d.rounded_br(r))
    }
}

/// A window control as libadwaita draws one: a symbolic icon on a faint
/// circle, inside a larger click target that lights the circle on hover.
fn window_control(button: WindowButton, maximized: bool, theme: &Theme) -> AnyElement {
    let fg = theme.text;
    let (id, circle, icon_name): (&'static str, &'static str, _) = match button {
        WindowButton::Minimize => ("win-min", "win-min-circle", "win-min"),
        WindowButton::Maximize => ("win-max", "win-max-circle", if maximized { "win-restore" } else { "win-max" }),
        WindowButton::Close => ("win-close", "win-close-circle", "win-close"),
    };
    div()
        .id(id)
        .group(id)
        .size(px(BUTTON))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id(circle)
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme::with_alpha(fg, 0.1))
                .group_hover(id, move |s| s.bg(theme::with_alpha(fg, 0.15)))
                .group_active(id, move |s| s.bg(theme::with_alpha(fg, 0.3)))
                .child(icon(icon_name, 16.0, fg)),
        )
        .on_click(move |_, window, _| match button {
            WindowButton::Minimize => window.minimize_window(),
            WindowButton::Maximize => window.zoom_window(),
            WindowButton::Close => window.remove_window(),
        })
        .into_any_element()
}

impl Editor {
    /// Wraps the app in the window frame: with client-side decorations, the
    /// shadow, outline and resize handles GNOME draws around its own windows.
    pub(super) fn window_frame(
        &mut self,
        body: AnyElement,
        frame: Frame,
        theme: &Theme,
        window: &mut Window,
    ) -> AnyElement {
        let Some(tiling) = frame.tiling else {
            window.set_client_inset(px(0.0));
            self.set_input_region(None, window);
            return div().size_full().child(body).into_any_element();
        };
        window.set_client_inset(px(SHADOW));

        // Input stops a resize handle's width outside the window; the rest
        // of the shadow lets clicks through.
        let size = window.viewport_size();
        let edge = |tiled: bool| if tiled { 0.0 } else { SHADOW - RESIZE };
        let (l, t, r, b) = (edge(tiling.left), edge(tiling.top), edge(tiling.right), edge(tiling.bottom));
        let region =
            gpui::Bounds::new(point(px(l), px(t)), gpui::size(size.width - px(l + r), size.height - px(t + b)));
        self.set_input_region(if tiling.is_tiled() { None } else { Some(region) }, window);

        let handles = ResizeRegion::new(px(SHADOW)).corner_size(px(SHADOW + RESIZE)).without_tiled_edges(tiling);
        let resizable = window.is_resizable();
        let active = window.is_window_active();
        let black = |a: f32| gpui::hsla(0.0, 0.0, 0.0, a);
        // libadwaita's `window.csd` box-shadow, lighter when the window is in
        // the background. gpui's blur radius is the gaussian's sigma: half
        // the CSS value.
        let shadows = if active {
            vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), black(0.15)).blur_radius(px(7.0)).spread_radius(px(5.0)),
                gpui::BoxShadow::new(px(0.0), px(0.0), black(0.10)).blur_radius(px(2.5)).spread_radius(px(2.0)),
                gpui::BoxShadow::new(px(0.0), px(0.0), black(0.05)).spread_radius(px(1.0)),
            ]
        } else {
            vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), black(0.08)).blur_radius(px(5.0)).spread_radius(px(5.0)),
                gpui::BoxShadow::new(px(0.0), px(0.0), black(0.05)).spread_radius(px(1.0)),
            ]
        };
        let outline = if theme.dark { gpui::hsla(0.0, 0.0, 1.0, 0.07) } else { gpui::hsla(0.0, 0.0, 1.0, 0.5) };
        frame
            .pad(div().id("window-frame").size_full())
            .cursor_with(
                move |pos, bounds| {
                    if resizable { handles.hit_test(pos, bounds).map(gpui::CursorStyle::from) } else { None }
                },
            )
            .on_mouse_down(MouseButton::Left, move |e, window, _| {
                let b = gpui::Bounds::new(point(px(0.0), px(0.0)), window.viewport_size());
                if let Some(edge) = handles.hit_test(e.position, b) {
                    if window.is_resizable() {
                        window.start_window_resize(edge);
                    }
                }
            })
            .child(
                frame
                    .round(div().size_full().relative())
                    .when(!tiling.is_tiled(), |d| d.shadow(shadows))
                    .on_mouse_move(|_, _, cx| cx.stop_propagation())
                    .child(body)
                    // The outline sits on top, inside the edge, like GTK's.
                    .when(!tiling.is_tiled(), |d| {
                        d.child(frame.round(div().absolute().inset_0().border_1().border_color(outline)))
                    }),
            )
            .into_any_element()
    }

    fn set_input_region(&mut self, region: Option<gpui::Bounds<gpui::Pixels>>, window: &mut Window) {
        if self.input_region != Some(region) {
            self.input_region = Some(region);
            window.set_input_region(region.as_ref().map(std::slice::from_ref));
        }
    }

    /// The header bar, after libadwaita's: the title and image size centred
    /// over the window, the window controls where the desktop's button
    /// layout puts them, and everything dimmed while the window is in the
    /// background.
    pub(super) fn render_titlebar(&mut self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let csd = matches!(window.window_decorations(), Decorations::Client { .. });
        let layout = if csd { cx.button_layout() } else { None }.unwrap_or(WindowButtonLayout {
            left: [None; gpui::MAX_BUTTONS_PER_SIDE],
            right: [None; gpui::MAX_BUTTONS_PER_SIDE],
        });
        let (title, subtitle) = match &self.doc {
            Some(d) => (d.name.clone(), Some(format!("{} × {}", d.shot.width, d.shot.height))),
            None => ("No image".to_string(), None),
        };
        let maximized = window.is_maximized();
        let controls = |side: &[Option<WindowButton>; gpui::MAX_BUTTONS_PER_SIDE]| {
            side.iter().flatten().map(|&b| window_control(b, maximized, theme)).collect::<Vec<_>>()
        };
        let (left, right) = (controls(&layout.left), controls(&layout.right));
        let width = |n: usize| if n == 0 { 0.0 } else { n as f32 * BUTTON + (n - 1) as f32 * CONTROL_GAP };
        // The start holds the left-hand controls; the end the preferences
        // button and the right-hand controls.
        let start_w = width(left.len());
        let end_w = BUTTON + if right.is_empty() { 0.0 } else { HEADER_GAP + width(right.len()) };
        // The title centres on the window, kept clear of whichever end is wider.
        let title_pad = HEADER_PAD + start_w.max(end_w) + HEADER_GAP;

        let control_box = |buttons: Vec<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap(px(CONTROL_GAP))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(buttons)
        };
        let fg = theme.text;
        let flat_hover = theme::with_alpha(fg, 0.07);
        let flat_active = theme::with_alpha(fg, 0.16);
        div()
            .id("titlebar")
            .relative()
            .h(px(HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .px(px(HEADER_PAD))
            .pt(px(6.0))
            .pb(px(7.0))
            .border_b_1()
            .border_color(theme.border)
            .when(!window.is_window_active(), |d| d.opacity(0.5))
            .on_mouse_down(MouseButton::Left, |ev, window, _| {
                if ev.click_count == 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
            })
            .on_mouse_down(MouseButton::Right, |ev, window, _| window.show_window_menu(ev.position))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .px(px(title_pad))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .max_w_full()
                            .text_size(px(14.5))
                            .line_height(px(17.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(fg)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title),
                    )
                    .when_some(subtitle, |d, s| {
                        d.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(15.0))
                                .text_color(theme.muted)
                                .whitespace_nowrap()
                                .child(s),
                        )
                    }),
            )
            .when(!left.is_empty(), |d| d.child(control_box(left)))
            .child(div().flex_1())
            .child(
                div()
                    .id("prefs-open")
                    .size(px(BUTTON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .hover(move |s| s.bg(flat_hover))
                    .active(move |s| s.bg(flat_active))
                    .child(icon("gear", 16.0, fg))
                    .tooltip_text("Preferences — Ctrl+,")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.open_prefs(None, cx))),
            )
            .when(!right.is_empty(), |d| d.child(div().w(px(HEADER_GAP))).child(control_box(right)))
            .into_any_element()
    }

    pub(super) fn render_toolbar(&mut self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let width = f32::from(window.viewport_size().width);
        // Shed labels before anything overflows.
        let tool_labels = width >= 1180.0;
        let action_labels = width >= 780.0;
        let has_doc = self.doc.is_some();
        let (hover, accent) = (theme.hover, theme.accent);

        let tools = Tool::ALL.iter().map(|&tool| {
            let pressed = self.tool == tool;
            let name = match tool {
                Tool::Select => "select",
                Tool::Arrow => "arrow",
                Tool::Rect => "rect",
                Tool::Mark => "mark",
                Tool::Redact => "redact",
                Tool::Text => "text",
                Tool::Emoji => "emoji",
                Tool::Crop => "crop",
            };
            let color = if pressed { crate::theme::WHITE } else { theme.text };
            div()
                .id(("tool", tool as usize))
                .h(px(32.0))
                .min_w(px(32.0))
                .px(px(if tool_labels { 10.0 } else { 0.0 }))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .font_weight(FontWeight(550.0))
                .text_color(color)
                .cursor_pointer()
                .map(|el| if pressed { el.bg(accent) } else { el.hover(move |s| s.bg(hover)) })
                .tooltip_text(format!("{} — {}", tool.label(), tool.key()))
                .child(icon(name, 15.0, color))
                .when(tool_labels, |el| el.child(tool.label()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    window.focus(&this.focus, cx);
                    this.finish_edit(cx);
                    // Emoji opens a picker, so its button also closes it again.
                    let next = if tool == Tool::Emoji && this.tool == Tool::Emoji { Tool::Select } else { tool };
                    this.set_tool(next, cx);
                }))
        });

        let palette = self.s(cx).kit().palette.clone();
        let swatches = palette.into_iter().enumerate().map(|(i, hex)| {
            let pressed = self.color_idx == i;
            div()
                .id(("swatch", i))
                .size(px(22.0))
                .flex_none()
                .rounded_full()
                .border_2()
                .border_color(if pressed { theme.text } else { theme.border })
                .bg(theme::from_rgb(rgb(&hex)))
                .cursor_pointer()
                .tooltip_text(hex.to_uppercase())
                .on_click(cx.listener(move |this, _, _, cx| this.pick_color(i, cx)))
        });

        let can_undo = self.doc.as_ref().is_some_and(|d| d.can_undo()) && self.edit.is_none();
        let can_redo = self.doc.as_ref().is_some_and(|d| d.can_redo()) && self.edit.is_none();
        let history_btn = |id: &'static str, name: &'static str, enabled: bool, tip: &'static str| {
            icon_button(id, name, 16.0, 32.0, theme)
                .rounded(px(7.0))
                .when(!enabled, |el| el.opacity(0.38))
                .tooltip_text(tip)
        };
        let copy_label = match self.flash {
            Some((FlashTarget::Copy, t)) => t,
            _ => "Copy",
        };
        let save_label = match self.flash {
            Some((FlashTarget::Save, t)) => t,
            _ => "Save",
        };
        let copy_open = self.popover == Some(Popover::Copy);
        let _ = has_doc;

        div()
            .h(px(52.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .bg(theme.bg)
            .border_b_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| window.focus(&this.focus, cx)))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(3.0))
                    .bg(theme.sunken)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(9.0))
                    .children(tools),
            )
            .child(divider(theme, 24.0))
            .child(div().flex().flex_none().items_center().gap(px(5.0)).children(swatches))
            .child(divider(theme, 24.0))
            .child(
                history_btn("undo", "undo", can_undo, "Undo — Ctrl+Z")
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                history_btn("redo", "redo", can_redo, "Redo — Ctrl+Shift+Z")
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
            .child(div().flex_1().min_w(px(8.0)))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_none()
                    .h(px(34.0))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(8.0))
                    .bg(theme.surface)
                    .child(
                        div()
                            .id("copy")
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .px(px(if action_labels { 14.0 } else { 9.0 }))
                            .rounded_l(px(7.0))
                            .text_size(px(12.5))
                            .font_weight(FontWeight(600.0))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .tooltip_text("Copy to clipboard — Ctrl+C")
                            .child(icon("copy", 15.0, theme.text))
                            .when(action_labels, |el| el.child(copy_label))
                            .on_click(cx.listener(|this, _, window, cx| this.copy_image(false, window, cx))),
                    )
                    .child(
                        div()
                            .id("copy-caret")
                            .w(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_l_1()
                            .border_color(theme.border)
                            .rounded_r(px(7.0))
                            .cursor_pointer()
                            .when(copy_open, |el| el.bg(hover))
                            .hover(move |s| s.bg(hover))
                            .tooltip_text("More copy options")
                            .child(icon("caret", 9.0, theme.muted))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_popover(Popover::Copy, cx))),
                    )
                    .when(copy_open, |el| el.child(deferred(self.render_copy_menu(theme, cx)).priority(1usize))),
            )
            .child(
                div()
                    .id("save")
                    .h(px(34.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(7.0))
                    .px(px(if action_labels { 14.0 } else { 9.0 }))
                    .rounded(px(8.0))
                    .bg(theme.accent)
                    .text_color(crate::theme::WHITE)
                    .text_size(px(12.5))
                    .font_weight(FontWeight(650.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.92))
                    .tooltip_text("Save to disk — Ctrl+S")
                    .child(icon("save", 15.0, crate::theme::WHITE))
                    .when(action_labels, |el| el.child(save_label))
                    .on_click(cx.listener(|this, _, window, cx| this.save_image(window, cx))),
            )
            .into_any_element()
    }

    pub(super) fn render_statusbar(
        &mut self,
        theme: &Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = self.s(cx).clone();
        let (hover, text, muted) = (theme.hover, theme.text, theme.muted);
        let status_btn = move |id: &'static str, on: bool, open: bool| {
            div()
                .id(id)
                .h(px(22.0))
                .px(px(8.0))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .rounded(px(5.0))
                .text_size(px(11.5))
                .text_color(if on { text } else { muted })
                .whitespace_nowrap()
                .cursor_pointer()
                .when(open, |el| el.bg(hover))
                .hover(move |st| st.bg(hover).text_color(text))
        };

        let active = s.active_preset().cloned();
        let modified = active.as_ref().is_some_and(|p| s.preset_is_modified(p));
        let preset_label = match &active {
            Some(p) => format!("{}{}", p.name, if modified { " •" } else { "" }),
            None => "Presets".to_string(),
        };
        let ratio = s.style.ratio();
        let backdrop_on = s.style.backdrop != 0;
        let zoom_label = if self.zoom == 0 {
            "Fit".to_string()
        } else {
            self.geom.map(|g| format!("{}%", (g.k * 100.0).round())).unwrap_or_else(|| "Fit".into())
        };
        let open = self.popover;

        div()
            .h(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.0))
            .pl(px(12.0))
            .pr(px(4.0))
            .border_t_1()
            .border_color(theme.border)
            .text_size(px(11.5))
            .text_color(theme.muted)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.focused_field.is_none() {
                        window.focus(&this.focus, cx);
                    }
                }),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .child(
                        status_btn("presets-btn", active.is_some(), open == Some(Popover::Presets))
                            .tooltip_text(match &active {
                                Some(p) => format!("Preset: {}{}", p.name, if modified { " (modified)" } else { "" }),
                                None => "Presets".into(),
                            })
                            .child(icon("presets", 12.0, if active.is_some() { text } else { muted }))
                            .child(div().max_w(px(160.0)).overflow_hidden().text_ellipsis().child(preset_label))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_popover(Popover::Presets, cx))),
                    )
                    .when(open == Some(Popover::Presets), |el| {
                        el.child(deferred(self.render_presets_pop(theme, cx)).priority(1usize))
                    }),
            )
            .child(
                div()
                    .flex_initial()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.hint()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .relative()
                    .flex()
                    .child(
                        status_btn("ratio-btn", ratio.value.is_some(), open == Some(Popover::Ratio))
                            .tooltip_text("Aspect ratio")
                            .child(icon("ratio", 12.0, if ratio.value.is_some() { text } else { muted }))
                            .child(ratio.label)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_popover(Popover::Ratio, cx))),
                    )
                    .when(open == Some(Popover::Ratio), |el| {
                        el.child(deferred(self.render_ratio_pop(theme, cx)).priority(1usize))
                    }),
            )
            .child(divider(theme, 16.0))
            .child(
                div()
                    .relative()
                    .flex()
                    .child(
                        status_btn("backdrop-btn", backdrop_on || s.style.pad > 0, open == Some(Popover::Backdrop))
                            .gap(px(7.0))
                            .tooltip_text("Padding & fill")
                            .child(self.fill_chip(s.style.backdrop, 11.0, 3.0, 1.0, theme, cx))
                            .child("Padding")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_popover(Popover::Backdrop, cx))),
                    )
                    .when(open == Some(Popover::Backdrop), |el| {
                        el.child(deferred(self.render_backdrop_pop(theme, cx)).priority(1usize))
                    }),
            )
            .child(divider(theme, 16.0))
            .child(
                div()
                    .id("wm-label")
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(7.0))
                    .px(px(4.0))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .child("Watermark")
                    .child(switch(s.style.watermark, theme))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_settings(cx, |s| s.style.watermark = !s.style.watermark);
                    })),
            )
            .child(divider(theme, 16.0))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(1.0))
                    .child(
                        icon_button("zoom-out", "minus", 12.0, 22.0, theme)
                            .w(px(24.0))
                            .rounded(px(5.0))
                            .tooltip_text("Zoom out")
                            .on_click(cx.listener(|this, _, _, cx| this.set_zoom(this.zoom - 1, cx))),
                    )
                    .child(
                        div()
                            .id("zoom-fit")
                            .min_w(px(52.0))
                            .h(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .text_color(text)
                            .font_weight(FontWeight(600.0))
                            .cursor_pointer()
                            .hover(move |st| st.bg(hover))
                            .tooltip_text("Fit to window")
                            .child(zoom_label)
                            .on_click(cx.listener(|this, _, _, cx| this.set_zoom(0, cx))),
                    )
                    .child(
                        icon_button("zoom-in", "plus", 12.0, 22.0, theme)
                            .w(px(24.0))
                            .rounded(px(5.0))
                            .tooltip_text("Zoom in")
                            .on_click(cx.listener(|this, _, _, cx| this.set_zoom(this.zoom + 1, cx))),
                    ),
            )
            .child(divider(theme, 16.0))
            .child(
                status_btn("colours-btn", false, false)
                    .tooltip_text("Preferences")
                    .child("Colours…")
                    .on_click(cx.listener(|this, _, _, cx| this.open_prefs(None, cx))),
            )
            .child(divider(theme, 16.0))
            .child(
                status_btn("theme-btn", false, false)
                    .tooltip_text("Toggle appearance")
                    .child(icon("moon", 12.0, muted))
                    .child(if s.theme == settings::Theme::Light { "Light" } else { "Dark" })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_theme(cx))),
            )
            .into_any_element()
    }

    /// The little square that previews a fill (or none).
    pub(crate) fn fill_chip(&self, index: u8, s: f32, r: f32, border: f32, theme: &Theme, cx: &App) -> AnyElement {
        let kit = self.s(cx).kit();
        let base = div().size(px(s)).flex_none().rounded(px(r)).border(px(border)).border_color(theme.border);
        any(match index {
            0 => base.bg(theme.sunken).child(
                // The "none" chip: a diagonal slash across it.
                div().size_full().bg(gpui::pattern_slash(theme.muted, 1.5, 6.0)),
            ),
            i @ 1..=4 => base.bg(theme::from_rgb(rgb(&kit.solid_bg[i as usize - 1]))),
            i => {
                let g = &kit.grad_bg[(i as usize - 5).min(3)];
                base.bg(super::stage::gradient(rgb(&g.a), rgb(&g.b), gpui::Bounds::default()))
            }
        })
    }
}

/// Ratio list for the popover, with each one's hint.
pub(super) fn ratios() -> &'static [settings::Ratio] {
    &RATIOS
}
