//! Small controls shared by the toolbar, status bar, popovers and
//! preferences: buttons, segmented pickers, sliders, the switch, text fields.

use super::Editor;
use super::stage::Drag;
use crate::assets::icon_path;
use crate::settings::{ADVANCED_META, AdvancedKey, PAD_MAX};
use crate::theme::Theme;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, PaintQuad, ParentElement, Pixels, Point, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Svg, canvas, div, point, prelude::FluentBuilder as _, px, size, svg,
};
use gpui_ce_elements::editable_text::{EditableTextState, text_input};

pub fn icon(name: &str, s: f32, color: Hsla) -> Svg {
    svg().path(icon_path(name)).size(px(s)).flex_none().text_color(color)
}

/// A plain text tooltip, shown on hover.
struct TipView(SharedString);

impl gpui::Render for TipView {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(gpui::rgb(0x2a2a31))
            .border_1()
            .border_color(gpui::rgb(0x37373f))
            .text_size(px(11.5))
            .text_color(gpui::rgb(0xececf1))
            .font_family(crate::settings::UI_FONT)
            .child(self.0.clone())
    }
}

pub trait Tip: StatefulInteractiveElement + Sized {
    fn tooltip_text(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |_, cx| cx.new(|_| TipView(text.clone())).into())
    }
}

impl<E: StatefulInteractiveElement> Tip for E {}

/// The uppercase section label popovers use.
pub fn pop_label(text: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(10.0))
        .font_weight(FontWeight(700.0))
        .text_color(theme.muted)
        .mb(px(8.0))
        .child(text.to_uppercase())
}

pub fn pop_note(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div().text_size(px(11.0)).line_height(px(16.0)).text_color(theme.muted).child(text.into())
}

/// A floating panel: popovers, menus, the emoji picker.
pub fn panel(theme: &Theme) -> Div {
    div()
        .bg(theme.bg)
        .border_1()
        .border_color(theme.border)
        .rounded(px(11.0))
        .shadow(vec![gpui::BoxShadow::new(px(0.0), px(16.0), theme.pop_shadow).blur_radius(px(40.0))])
}

pub fn divider(theme: &Theme, h: f32) -> Div {
    div().w(px(1.0)).h(px(h)).flex_none().bg(theme.border)
}

/// A plain bordered button, as the old popover action buttons were.
pub fn button(id: impl Into<ElementId>, text: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    let hover = theme.hover;
    div()
        .id(id)
        .h(px(29.0))
        .px(px(11.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme.border)
        .rounded(px(7.0))
        .text_size(px(12.0))
        .font_weight(FontWeight(600.0))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(text.into())
}

pub fn primary_button(id: impl Into<ElementId>, text: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(29.0))
        .px(px(13.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .bg(theme.accent)
        .text_size(px(12.0))
        .font_weight(FontWeight(650.0))
        .text_color(crate::theme::WHITE)
        .cursor_pointer()
        .hover(|s| s.opacity(0.92))
        .child(text.into())
}

/// A small square icon button (undo, redo, row actions).
pub fn icon_button(id: impl Into<ElementId>, name: &str, s: f32, box_size: f32, theme: &Theme) -> Stateful<Div> {
    let (hover, text) = (theme.hover, theme.text);
    div()
        .id(id)
        .size(px(box_size))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .text_color(theme.muted)
        .cursor_pointer()
        .hover(move |st| st.bg(hover).text_color(text))
        .child(svg().path(icon_path(name)).size(px(s)).text_color(theme.muted))
}

/// A row of mutually exclusive choices.
pub fn segmented(theme: &Theme) -> Div {
    div().flex().gap(px(2.0)).p(px(3.0)).bg(theme.sunken).border_1().border_color(theme.border).rounded(px(8.0))
}

pub fn seg_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    pressed: bool,
    theme: &Theme,
) -> Stateful<Div> {
    let (hover, txt) = (theme.hover, theme.text);
    div()
        .id(id)
        .flex_1()
        .h(px(25.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .text_size(px(11.0))
        .font_weight(FontWeight(600.0))
        .cursor_pointer()
        .map(|el| {
            if pressed {
                el.bg(theme.accent).text_color(crate::theme::WHITE)
            } else {
                el.text_color(theme.muted).hover(move |s| s.bg(hover).text_color(txt))
            }
        })
        .child(text.into())
}

/// The on/off switch beside "Watermark".
pub fn switch(on: bool, theme: &Theme) -> Div {
    div()
        .relative()
        .w(px(28.0))
        .h(px(16.0))
        .flex_none()
        .rounded(px(9.0))
        .bg(if on { theme.accent } else { theme.border })
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .left(px(if on { 14.0 } else { 2.0 }))
                .size(px(12.0))
                .rounded_full()
                .bg(crate::theme::WHITE)
                .shadow(vec![gpui::BoxShadow::new(px(0.0), px(1.0), gpui::rgba(0x0000004d)).blur_radius(px(2.0))]),
        )
}

/// A single-line text field.
pub fn field(id: &str, state: &gpui::Entity<EditableTextState>, placeholder: &str, theme: &Theme) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .h(px(29.0))
        .px(px(9.0))
        .flex()
        .items_center()
        .bg(theme.sunken)
        .border_1()
        .border_color(theme.border)
        .rounded(px(7.0))
        .text_size(px(12.0))
        .text_color(theme.text)
        .overflow_hidden()
        .child(
            text_input(SharedString::from(id.to_string()))
                .state(state.downgrade())
                .placeholder(placeholder.to_string())
                .w_full()
                .whitespace_nowrap()
                .min_h_auto()
                .caret_blink_interval_500ms(),
        )
        .into_any_element()
}

/// The brand mark: a white Cookie "e" in a pink circle, `s` pixels across.
pub fn mark(s: f32) -> Div {
    let glyph = s * 1.324;
    div()
        .size(px(s))
        .flex_none()
        .rounded_full()
        .bg(gpui::rgb(0xFF007F))
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .child(
            div()
                .relative()
                .left(px(-0.034 * glyph))
                .top(px(-0.064 * glyph))
                .font_family("Cookie")
                .text_size(px(glyph))
                .line_height(px(glyph))
                .text_color(crate::theme::WHITE)
                .child("e"),
        )
}

/* ==========================================================================
Sliders
========================================================================== */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SliderId {
    Pad,
    PadPrefs,
    Advanced(AdvancedKey),
    Hue,
    Sv,
}

const THUMB: f32 = 14.0;

impl Editor {
    /// A horizontal range slider. Dragging anywhere on it sets the value.
    pub(crate) fn slider(&self, id: SliderId, frac: f32, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let (track, thumb) = (theme.border, theme.accent);
        let frac = frac.clamp(0.0, 1.0);
        div()
            .id(ElementId::Name(format!("slider-{id:?}").into()))
            .flex_1()
            .min_w(px(0.0))
            .h(px(20.0))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.drag = Some(Drag::Slider(id));
                    this.slider_to(id, ev.position, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                canvas(
                    move |b, _, cx| {
                        entity.update(cx, |e, _| {
                            e.sliders.insert(id, b);
                        });
                    },
                    move |b, _, window, _| {
                        let cy = b.origin.y + b.size.height / 2.0;
                        let t = Bounds::new(point(b.origin.x, cy - px(2.0)), size(b.size.width, px(4.0)));
                        window.paint_quad(PaintQuad::new(t).corner_radii(px(2.0)).background(track));
                        let x = b.origin.x + px(THUMB / 2.0) + (b.size.width - px(THUMB)) * frac;
                        let tb =
                            Bounds::new(point(x - px(THUMB / 2.0), cy - px(THUMB / 2.0)), size(px(THUMB), px(THUMB)));
                        window.paint_drop_shadows(
                            tb,
                            px(THUMB / 2.0).into(),
                            0.0,
                            &[gpui::BoxShadow::new(px(0.0), px(1.0), gpui::rgba(0x00000059)).blur_radius(px(3.0))],
                        );
                        window.paint_quad(PaintQuad::new(tb).corner_radii(px(THUMB / 2.0)).background(thumb));
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    /// Sets a slider's value from the pointer's x.
    pub(crate) fn slider_to(&mut self, id: SliderId, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(b) = self.sliders.get(&id).copied() else { return };
        let fx = ((f32::from(pos.x - b.origin.x) - THUMB / 2.0) / (f32::from(b.size.width) - THUMB).max(1.0))
            .clamp(0.0, 1.0);
        let fy = (f32::from(pos.y - b.origin.y) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0);
        match id {
            SliderId::Pad | SliderId::PadPrefs => {
                let v = (fx * PAD_MAX as f32).round() as u32;
                self.set_pad(v, cx);
            }
            SliderId::Advanced(key) => {
                let meta = ADVANCED_META.iter().find(|m| m.key == key).expect("meta");
                let raw = meta.min + fx as f64 * (meta.max - meta.min);
                let v = ((raw / meta.step).round() * meta.step * 1000.0).round() / 1000.0;
                self.update_settings(cx, |s| s.kit_mut().advanced.set(key, v));
            }
            SliderId::Hue => {
                if let Some(p) = &mut self.color_picker {
                    p.h = fx * 360.0;
                }
                self.apply_picker(cx);
            }
            SliderId::Sv => {
                let fx = (f32::from(pos.x - b.origin.x) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0);
                if let Some(p) = &mut self.color_picker {
                    p.s = fx;
                    p.v = 1.0 - fy;
                }
                self.apply_picker(cx);
            }
        }
    }

    pub(crate) fn set_pad(&mut self, v: u32, cx: &mut Context<Self>) {
        let v = v.min(PAD_MAX);
        if self.s(cx).style.pad != v {
            self.update_settings(cx, |s| s.style.pad = v);
            self.sync_after_style(cx);
        }
    }

    /// Anything that changes the output size re-reads the width field.
    pub(crate) fn sync_after_style(&mut self, cx: &mut Context<Self>) {
        self.sync_inputs(cx);
        cx.notify();
    }
}
