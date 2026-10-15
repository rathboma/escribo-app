//! The canvas area with its overlays (empty state, emoji picker, crop bar,
//! toast) and the popovers: copy menu, presets, aspect ratio & export size,
//! padding & fill.

use super::widgets::{
    SliderId, Tip, button, field, icon, icon_button, mark, panel, pop_label, pop_note, primary_button, seg_button,
    segmented,
};
use super::{Editor, Field, Popover, PrefsTab, any, set_input_unless_focused};
use crate::app;
use crate::doc::{EMOJIS, Tool};
use crate::settings::{self, GRAD_NAMES, SCALES, SOLID_NAMES, Settings, slug};
use crate::theme::Theme;
use gpui::{
    AnyElement, Context, ExternalPaths, Focusable, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, PromptLevel, StatefulInteractiveElement, Styled, Window, canvas, div, prelude::FluentBuilder as _,
    px,
};
use gpui_ce_elements::editable_text::actions::{Enter, Escape};

impl Editor {
    pub(super) fn render_area(&mut self, theme: &Theme, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let accent = theme.accent;
        div()
            .id("area")
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.canvas)
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_area_mouse_down))
            .on_scroll_wheel(cx.listener(Self::on_area_scroll))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.file_hover = false;
                if let Some(p) = paths.paths().iter().find(|p| app::is_image_path(p)).or(paths.paths().first()) {
                    this.finish_edit(cx);
                    this.load_path(p.clone(), window, cx);
                }
            }))
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_2().border_color(accent).border_dashed())
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                    move |bounds, hitbox, window, cx| {
                        entity.update(cx, |editor, cx| editor.paint_area(bounds, &hitbox, window, cx));
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(self.doc.is_none(), |el| el.child(self.render_empty(theme, cx)))
            .when(self.emoji_open && self.doc.is_some(), |el| el.child(self.render_emoji_picker(theme, cx)))
            .when(self.crop.is_some() && self.tool == Tool::Crop, |el| el.child(self.render_crop_bar(theme, cx)))
            .when_some(self.toast.clone(), |el, msg| {
                el.child(
                    div().absolute().bottom(px(14.0)).left_0().right_0().flex().justify_center().child(
                        super::widgets::panel(theme)
                            .rounded(px(9.0))
                            .px(px(14.0))
                            .py(px(8.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight(600.0))
                            .text_color(theme.text)
                            .child(msg),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_empty(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let hover = theme.hover;
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.0))
            .text_color(theme.muted)
            .child(mark(54.0).opacity(0.9))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight(650.0))
                            .text_color(theme.text)
                            .child(if self.loading { "Opening…" } else { "Drop a screenshot here" }),
                    )
                    .child(div().text_size(px(12.5)).child("or paste from the clipboard with Ctrl+V")),
            )
            .child(
                div()
                    .id("file-open")
                    .h(px(32.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(8.0))
                    .bg(theme.surface)
                    .text_color(theme.text)
                    .text_size(px(12.5))
                    .font_weight(FontWeight(600.0))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx)))
                    .child("Choose an image…"),
            )
            .into_any_element()
    }

    pub(crate) fn emoji_picker_click_away(&mut self, cx: &mut Context<Self>) {
        // A click away closes the picker; with nothing picked, the tool is no
        // longer wanted either.
        if self.emoji_open && self.tool == Tool::Emoji && self.pending_emoji.is_none() {
            self.set_tool(Tool::Select, cx);
        }
    }

    fn render_emoji_picker(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let hover = theme.hover;
        let buttons = EMOJIS.iter().enumerate().map(|(i, e)| {
            div()
                .id(("emoji", i))
                .w(px(32.0))
                .h(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_size(px(17.0))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(*e)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pending_emoji = Some(EMOJIS[i]);
                    this.emoji_open = false;
                    cx.notify();
                }))
        });
        div()
            .absolute()
            .top(px(8.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                panel(theme)
                    .id("emoji-picker")
                    .occlude()
                    .p(px(7.0))
                    .w(px(7.0 * 2.0 + 32.0 * 8.0 + 2.0 * 7.0 + 2.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(2.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(buttons),
            )
            .into_any_element()
    }

    fn render_crop_bar(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let c = self.crop.unwrap_or_default();
        div()
            .absolute()
            .bottom(px(16.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                panel(theme)
                    .id("crop-bar")
                    .occlude()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(14.0))
                    .pr(px(8.0))
                    .py(px(7.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(div().text_size(px(12.0)).text_color(theme.muted).child(format!(
                        "{} × {}",
                        c.w.round(),
                        c.h.round()
                    )))
                    .child(
                        button("crop-cancel", "Cancel", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.set_tool(Tool::Select, cx))),
                    )
                    .child(
                        primary_button("crop-apply", "Apply crop", theme)
                            .on_click(cx.listener(|this, _, window, cx| this.apply_crop(window, cx))),
                    ),
            )
            .into_any_element()
    }

    /* --- Copy menu --------------------------------------------------------- */

    pub(super) fn render_copy_menu(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let hover = theme.hover;
        let item = |id: &'static str, icon_name: &'static str, text: &'static str, keys: &'static str| {
            div()
                .id(id)
                .h(px(30.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .font_weight(FontWeight(550.0))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(icon(icon_name, 13.0, theme.text))
                .child(div().flex_1().child(text))
                .child(div().text_size(px(11.0)).text_color(theme.muted).child(keys))
        };
        div()
            .absolute()
            .top(px(39.0))
            .right(px(-1.0))
            .child(
                panel(theme)
                    .id("copy-menu")
                    .occlude()
                    .rounded(px(10.0))
                    .w(px(216.0))
                    .p(px(5.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_popover_from_outside(Popover::Copy, cx)))
                    .child(
                        item("copy-menu-copy", "copy", "Copy", "Ctrl C")
                            .on_click(cx.listener(|this, _, window, cx| this.copy_image(false, window, cx))),
                    )
                    .child(
                        item("copy-menu-close", "copy-close", "Copy & close", "⇧Ctrl C")
                            .on_click(cx.listener(|this, _, window, cx| this.copy_image(true, window, cx))),
                    ),
            )
            .into_any_element()
    }

    /* --- Ratio & export size ---------------------------------------------- */

    pub(super) fn render_ratio_pop(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.s(cx).clone();
        let r = s.style.ratio();
        let ratio_note = if r.value.is_none() {
            "Matches the screenshot — no extra canvas."
        } else if s.style.backdrop != 0 {
            "Extra canvas is filled with the chosen fill."
        } else {
            "Extra canvas stays transparent in the PNG."
        };
        let ratios = super::chrome::ratios().iter().enumerate().map(|(i, ratio)| {
            let id = ratio.id;
            seg_button(("ratio", i), ratio.label, s.style.ratio == ratio.id, theme).tooltip_text(ratio.hint).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.update_settings(cx, |s| s.style.ratio = id.to_string());
                    this.sync_after_style(cx);
                }),
            )
        });
        let layout = self.doc.as_ref().map(|d| crate::render::layout_for(&s, &d.shot));
        let out_h = layout.map(|l| {
            let (w, _) = crate::render::export_size(&s, &l);
            ((l.out_h * w as f64) / l.out_w).round().max(1.0).to_string()
        });
        let scales = SCALES.iter().enumerate().map(|(i, (label, k))| {
            let k = *k;
            let pressed = s.style.out_w.is_none() && s.style.out_scale == k;
            seg_button(("scale", i), *label, pressed, theme)
                .when_some(layout, |el, l| el.tooltip_text(format!("{}px wide", (l.out_w * k).round())))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.update_settings(cx, |s| {
                        s.style.out_scale = k;
                        s.style.out_w = None;
                    });
                    this.sync_after_style(cx);
                }))
        });
        div()
            .absolute()
            .bottom(px(31.0))
            .right_0()
            .child(
                panel(theme)
                    .id("ratio-pop")
                    .occlude()
                    .w(px(252.0))
                    .p(px(13.0))
                    .on_mouse_down_out(
                        cx.listener(|this, _, _, cx| this.close_popover_from_outside(Popover::Ratio, cx)),
                    )
                    .child(pop_label("Aspect ratio", theme))
                    .child(segmented(theme).mb(px(11.0)).children(ratios))
                    .child(pop_note(ratio_note, theme))
                    .child(pop_label("Export size", theme).mt(px(15.0)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .mb(px(9.0))
                            .child(
                                div()
                                    .w(px(76.0))
                                    .flex_none()
                                    .flex()
                                    .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                                        this.commit_out_w(cx);
                                        window.focus(&this.focus, cx);
                                        cx.stop_propagation();
                                    }))
                                    .child(field("out-w", &self.inputs.out_w, "", theme)),
                            )
                            .child(div().text_size(px(11.5)).text_color(theme.muted).child("×"))
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(28.0))
                                    .flex()
                                    .items_center()
                                    .px(px(9.0))
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded(px(7.0))
                                    .bg(theme.sunken)
                                    .text_color(theme.muted)
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight(600.0))
                                    .child(out_h.unwrap_or_else(|| "—".into())),
                            )
                            .child(div().text_size(px(11.0)).text_color(theme.muted).child("px")),
                    )
                    .child(segmented(theme).children(scales)),
            )
            .into_any_element()
    }

    /* --- Padding & fill ---------------------------------------------------- */

    pub(crate) fn pad_note(s: &Settings) -> &'static str {
        if s.style.pad == 0 {
            "No margin around the screenshot."
        } else if s.style.backdrop != 0 {
            "Margin uses the fill below."
        } else {
            "Margin stays transparent in the PNG."
        }
    }

    pub(crate) fn pad_row(&self, id: SliderId, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let pad = self.s(cx).style.pad;
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .mb(px(6.0))
            .child(div().text_size(px(11.5)).text_color(theme.muted).child("Padding"))
            .child(self.slider(id, pad as f32 / settings::PAD_MAX as f32, theme, cx))
            .child(
                div()
                    .w(px(30.0))
                    .flex_none()
                    .text_size(px(11.5))
                    .font_weight(FontWeight(600.0))
                    .text_color(theme.text)
                    .text_right()
                    .child(format!("{pad}%")),
            )
            .into_any_element()
    }

    pub(crate) fn pick_fill(&mut self, index: u8, cx: &mut Context<Self>) {
        self.update_settings(cx, |s| {
            s.style.backdrop = index;
            // A fill with no margin has nowhere to show on "Orig" — give it some.
            if index != 0 && s.style.pad == 0 {
                s.style.pad = 6;
            }
        });
        self.sync_after_style(cx);
    }

    pub(super) fn render_backdrop_pop(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.s(cx).clone();
        let chip = |i: u8, cx: &mut Context<Self>| {
            let pressed = s.style.backdrop == i;
            let name = match i {
                0 => "None",
                1..=4 => SOLID_NAMES[i as usize - 1],
                _ => GRAD_NAMES[i as usize - 5],
            };
            div()
                .id(("fill", i as usize))
                .cursor_pointer()
                .tooltip_text(name)
                .rounded(px(7.0))
                .border_2()
                .border_color(if pressed { theme.text } else { gpui::transparent_black() })
                .child(self.fill_chip(i, 22.0, 5.0, 1.0, theme, cx))
                .on_click(cx.listener(move |this, _, _, cx| this.pick_fill(i, cx)))
        };
        let solids: Vec<_> = (0..=4).map(|i| chip(i, cx)).collect();
        let grads: Vec<_> = (5..=8).map(|i| chip(i, cx)).collect();
        div()
            .absolute()
            .bottom(px(31.0))
            .right_0()
            .child(
                panel(theme)
                    .id("backdrop-pop")
                    .occlude()
                    .w(px(252.0))
                    .p(px(13.0))
                    .on_mouse_down_out(
                        cx.listener(|this, _, _, cx| this.close_popover_from_outside(Popover::Backdrop, cx)),
                    )
                    .child(self.pad_row(SliderId::Pad, theme, cx))
                    .child(pop_note(Self::pad_note(&s), theme).mb(px(15.0)))
                    .child(pop_label("Fill — solid", theme))
                    .child(div().flex().gap(px(7.0)).mb(px(14.0)).children(solids))
                    .child(pop_label("Fill — gradient", theme))
                    .child(div().flex().gap(px(7.0)).children(grads)),
            )
            .into_any_element()
    }

    /* --- Presets ----------------------------------------------------------- */

    pub(super) fn render_presets_pop(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.s(cx).clone();
        let profile = s.profile().clone();
        let active = s.active_preset.clone();
        let hover = theme.hover;
        let rows = profile.presets.iter().enumerate().map(|(i, p)| {
            let is_active = active.as_deref() == Some(p.id.as_str());
            let modified = is_active && s.preset_is_modified(p);
            let (id_apply, id_update, id_export, id_delete) = (p.id.clone(), p.id.clone(), p.id.clone(), p.id.clone());
            let name = p.name.clone();
            div()
                .id(("preset-row", i))
                .h(px(32.0))
                .pl(px(8.0))
                .pr(px(4.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(7.0))
                .hover(move |st| st.bg(hover))
                .child(div().size(px(6.0)).flex_none().rounded_full().when(is_active, |d| d.bg(theme.accent)))
                .child(
                    div()
                        .id(("preset-name", i))
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(550.0))
                        .text_color(theme.text)
                        .cursor_pointer()
                        .tooltip_text(format!("Apply “{}”", p.name))
                        .child(p.name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let mut applied = None;
                            this.update_settings(cx, |s| applied = s.apply_preset(&id_apply));
                            if let Some(name) = applied {
                                this.sync_after_style(cx);
                                this.show_toast(format!("Applied “{name}”"), cx);
                            }
                        })),
                )
                .when(modified, |el| el.child(div().text_size(px(10.5)).text_color(theme.muted).child("modified")))
                .child(
                    icon_button(("preset-update", i), "update", 12.0, 22.0, theme)
                        .tooltip_text("Update with the current style")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let mut done = None;
                            this.update_settings(cx, |s| done = s.update_preset(&id_update));
                            if let Some(name) = done {
                                this.show_toast(format!("Updated “{name}”"), cx);
                            }
                        })),
                )
                .child(
                    icon_button(("preset-export", i), "export", 12.0, 22.0, theme)
                        .tooltip_text("Export this preset")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let list: Vec<_> =
                                this.s(cx).profile().presets.iter().filter(|p| p.id == id_export).cloned().collect();
                            if let Some(p) = list.first() {
                                let name = format!("{}.escribo-preset.json", slug(&p.name));
                                this.export_json(Settings::presets_file(&list), name, "Preset", window, cx);
                            }
                        })),
                )
                .child(icon_button(("preset-delete", i), "close", 11.0, 22.0, theme).tooltip_text("Delete").on_click(
                    cx.listener(move |this, _, window, cx| {
                        let id = id_delete.clone();
                        this.confirm(format!("Delete preset “{name}”?"), window, cx, move |this, cx| {
                            this.update_settings(cx, |s| s.delete_preset(&id));
                        });
                    }),
                ))
        });
        let empty = profile.presets.is_empty();
        let presets_for_export = profile.presets.clone();
        let profile_name = profile.name.clone();

        div()
            .absolute()
            .bottom(px(31.0))
            .left_0()
            .child(
                panel(theme)
                    .id("presets-pop")
                    .occlude()
                    .w(px(272.0))
                    .p(px(13.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_popover_from_outside(Popover::Presets, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .mb(px(8.0))
                            .child(pop_label("Presets", theme).mb(px(0.0)))
                            .child(
                                div()
                                    .id("presets-profile")
                                    .max_w(px(150.0))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(5.0))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight(600.0))
                                    .text_color(theme.accent)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .cursor_pointer()
                                    .hover(move |st| st.bg(hover))
                                    .tooltip_text(format!("Profile “{}” — manage profiles", profile.name))
                                    .child(profile.name.clone())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.popover = None;
                                        this.open_prefs(Some(PrefsTab::Profiles), cx);
                                    })),
                            ),
                    )
                    .when(!empty, |el| el.child(div().flex().flex_col().gap(px(2.0)).mb(px(8.0)).children(rows)))
                    .when(empty, |el| {
                        el.child(pop_note(
                            "No presets in this profile yet. Save the current style — ratio, size, padding, fill, watermark — to reuse it, or share it with your team.",
                            theme,
                        ).mb(px(10.0)))
                    })
                    .when(self.preset_form, |el| {
                        el.child(
                            div()
                                .flex()
                                .gap(px(6.0))
                                .mb(px(10.0))
                                .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                                    this.submit_preset_form(window, cx);
                                    cx.stop_propagation();
                                }))
                                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                                    this.preset_form = false;
                                    window.focus(&this.focus, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .child(field("preset-name", &self.inputs.preset_name, "Preset name", theme))
                                .child(primary_button("preset-save", "Save", theme).on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_preset_form(window, cx)
                                })))
                                .child(button("preset-cancel", "Cancel", theme).on_click(cx.listener(|this, _, window, cx| {
                                    this.preset_form = false;
                                    window.focus(&this.focus, cx);
                                    cx.notify();
                                }))),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .pt(px(10.0))
                            .border_t_1()
                            .border_color(theme.border)
                            .child(
                                button("preset-new", "Save current as preset…", theme)
                                    .w_full()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.preset_form = true;
                                        set_input_unless_focused(&this.inputs.preset_name, "", false, cx);
                                        let h = this.inputs.preset_name.read(cx).focus_handle(cx);
                                        window.focus(&h, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(button("presets-import", "Import…", theme).on_click(cx.listener(|this, _, window, cx| {
                                this.import_file(window, cx)
                            })))
                            .child(
                                button("presets-export", "Export all", theme)
                                    .when(empty, |el| el.opacity(0.45))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if presets_for_export.is_empty() {
                                            this.show_toast("No presets to export", cx);
                                            return;
                                        }
                                        let label = if presets_for_export.len() == 1 { "Preset" } else { "Presets" };
                                        let name = format!("{}.escribo-presets.json", slug(&profile_name));
                                        this.export_json(Settings::presets_file(&presets_for_export), name, label, window, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn submit_preset_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.inputs.preset_name.read(cx).as_str().trim().to_string();
        if name.is_empty() {
            let h = self.inputs.preset_name.read(cx).focus_handle(cx);
            window.focus(&h, cx);
            return;
        }
        let mut saved = None;
        self.update_settings(cx, |s| saved = s.save_preset(&name));
        self.preset_form = false;
        window.focus(&self.focus, cx);
        if let Some(name) = saved {
            self.show_toast(format!("Saved “{name}”"), cx);
        }
    }

    /* --- Files ------------------------------------------------------------- */

    pub(crate) fn export_json(
        &mut self,
        text: String,
        name: String,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let result = app::save_or_fallback(cx, app::documents_dir(), name, text.into_bytes()).await;
            this.update(cx, |this, cx| match result {
                Ok(Some((_, true))) => this.show_toast(format!("{label} exported"), cx),
                Ok(Some((path, false))) => this.show_toast(format!("{label} exported to {}", app::tilde(&path)), cx),
                Ok(None) => {}
                Err(err) => {
                    log::error!("export failed: {err:#}");
                    this.show_toast(format!("Could not export {}", label.to_lowercase()), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Reads any escribo file: a profile with its presets, or presets.
    pub(crate) fn import_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let picked = app::open_with_dialog(cx, "Import presets or a profile").await;
            let text = match picked {
                Ok(Some(path)) => match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(_) => {
                        this.update(cx, |this, cx| this.show_toast("Could not read that file", cx)).ok();
                        return;
                    }
                },
                Ok(None) => return,
                Err(err) => {
                    log::warn!("{err:#}");
                    this.update(cx, |this, cx| this.show_toast("Could not open the file dialog", cx)).ok();
                    return;
                }
            };
            this.update(cx, |this, cx| {
                let mut outcome = None;
                this.update_settings(cx, |s| outcome = Some(s.import_text(&text)));
                if let Some(o) = outcome {
                    if o.profile_changed {
                        this.profile_changed(cx);
                    }
                    this.show_toast(o.message, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Asks before something destructive; `then` runs on "Delete".
    pub(crate) fn confirm(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let answer = window.prompt(PromptLevel::Warning, &message, None, &["Delete", "Cancel"], cx);
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                this.update(cx, |this, cx| then(this, cx)).ok();
            }
        })
        .detach();
    }

    pub(crate) fn focus_field(&self, f: Field, window: &mut Window, cx: &mut Context<Self>) {
        let h = self.inputs.get(f).read(cx).focus_handle(cx);
        window.focus(&h, cx);
    }

    #[allow(dead_code)]
    fn _unused(&self) -> AnyElement {
        any(div())
    }
}
