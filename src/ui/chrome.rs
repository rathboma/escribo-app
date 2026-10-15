//! The window's frame and bars: the client-side window border, the title
//! bar, the toolbar of tools and actions, and the status bar.

use super::widgets::{Tip, divider, icon, icon_button, mark, switch};
use super::{Editor, FlashTarget, Popover, any};
use crate::assets::icon_path;
use crate::doc::{Tool, rgb};
use crate::settings::{self, RATIOS};
use crate::theme::{self, Theme};
use gpui::{
    AnyElement, App, Context, Decorations, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    ResizeEdge, ResizeRegion, StatefulInteractiveElement, Styled, Window, deferred, div, point,
    prelude::FluentBuilder as _, px, svg,
};

const SHADOW: f32 = 10.0;
const RADIUS: f32 = 10.0;

/// Client-side decorations on Linux: a soft shadow, rounded corners and
/// resize edges around the app, all dropped when the window is tiled or
/// maximised, or when the compositor draws the frame itself.
pub(super) fn window_frame(
    body: AnyElement,
    decorations: Decorations,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Decorations::Client { tiling } = decorations else {
        window.set_client_inset(px(0.0));
        return div().size_full().child(body).into_any_element();
    };
    let _ = cx;
    let inset = if tiling.is_tiled() { px(0.0) } else { px(SHADOW) };
    window.set_client_inset(inset);
    let region = ResizeRegion::new(px(SHADOW)).corner_size(px(SHADOW)).without_tiled_edges(tiling);
    let resizable = window.is_resizable();
    let border = theme.border;
    div()
        .id("window-frame")
        .size_full()
        .bg(gpui::transparent_black())
        .cursor_with(
            move |pos, bounds| if resizable { region.hit_test(pos, bounds).map(gpui::CursorStyle::from) } else { None },
        )
        .when(!tiling.top, |d| d.pt(inset))
        .when(!tiling.bottom, |d| d.pb(inset))
        .when(!tiling.left, |d| d.pl(inset))
        .when(!tiling.right, |d| d.pr(inset))
        .on_mouse_down(MouseButton::Left, move |e, window, _| {
            let b = gpui::Bounds::new(point(px(0.0), px(0.0)), window.viewport_size());
            if let Some(edge) = region.hit_test(e.position, b) {
                if window.is_resizable() {
                    window.start_window_resize(edge);
                }
            }
        })
        .child(
            div()
                .size_full()
                .overflow_hidden()
                .when(!(tiling.top || tiling.left), |d| d.rounded_tl(px(RADIUS)))
                .when(!(tiling.top || tiling.right), |d| d.rounded_tr(px(RADIUS)))
                .when(!(tiling.bottom || tiling.left), |d| d.rounded_bl(px(RADIUS)))
                .when(!(tiling.bottom || tiling.right), |d| d.rounded_br(px(RADIUS)))
                .when(!tiling.is_tiled(), |d| {
                    d.border_1().border_color(border).shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(2.0), gpui::rgba(0x00000066)).blur_radius(px(SHADOW / 1.5)),
                    ])
                })
                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                .child(body),
        )
        .into_any_element()
}

#[allow(dead_code)]
fn _edge(_: ResizeEdge) {}

impl Editor {
    pub(super) fn render_titlebar(&mut self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let csd = matches!(window.window_decorations(), Decorations::Client { .. });
        let (name, dims) = match &self.doc {
            Some(d) => (d.name.clone(), format!("{} × {}", d.shot.width, d.shot.height)),
            None => ("No image".to_string(), String::new()),
        };
        let hover = theme.hover;
        let win_btn = |id: &'static str, icon_name: &'static str, close: bool, theme: &Theme| {
            let (surface, muted, text) = (theme.surface, theme.muted, theme.text);
            let close_hover = theme.close_hover;
            div()
                .id(id)
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(surface)
                .text_color(muted)
                .cursor_pointer()
                .hover(move |s| {
                    if close { s.bg(close_hover).text_color(crate::theme::WHITE) } else { s.bg(hover).text_color(text) }
                })
                .child(svg().path(icon_path(icon_name)).size(px(9.0)))
        };
        div()
            .id("titlebar")
            .h(px(38.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.0))
            .pl(px(12.0))
            .pr(px(10.0))
            .bg(theme.bg)
            .border_b_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, |ev, window, _| {
                if ev.click_count == 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
            })
            .on_mouse_down(MouseButton::Right, |ev, window, _| window.show_window_menu(ev.position))
            .child(mark(16.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_baseline()
                    .justify_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight(600.0))
                            .text_color(theme.text)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(div().text_size(px(11.5)).text_color(theme.muted).whitespace_nowrap().child(dims)),
            )
            .child(
                icon_button("prefs-open", "gear", 14.0, 24.0, theme)
                    .tooltip_text("Preferences — Ctrl+,")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.open_prefs(None, cx))),
            )
            .when(csd, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            win_btn("win-min", "win-min", false, theme)
                                .on_click(|_, window, _| window.minimize_window()),
                        )
                        .child(
                            win_btn("win-max", "win-max", false, theme).on_click(|_, window, _| window.zoom_window()),
                        )
                        .child(
                            win_btn("win-close", "win-close", true, theme)
                                .on_click(|_, window, _| window.remove_window()),
                        ),
                )
            })
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
            .bg(theme.bg)
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
