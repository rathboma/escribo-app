//! Preferences: the active profile's palette, fills, font, advanced style and
//! watermark, the profiles themselves — and the colour picker the colour
//! wells open.

use super::widgets::{
    SliderId, Tip, button, field, icon, icon_button, pop_label, primary_button, seg_button, segmented,
};
use super::{ColorPicker, ColorTarget, Editor, Field, PrefsTab, ProfileForm, any};
use crate::doc::{Rgb, rgb};
use crate::settings::{ADVANCED_META, FONTS, GRAD_NAMES, SOLID_NAMES, Settings, WmMode, format_advanced, plural, slug};
use crate::theme::{self, Theme};
use gpui::{
    AnyElement, Bounds, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    PaintQuad, ParentElement, Pixels, Point, StatefulInteractiveElement, Styled, Window, canvas, deferred, div, point,
    prelude::FluentBuilder as _, px, size,
};
use gpui_ce_elements::editable_text::actions::{Enter, Escape};

const TABS: [(PrefsTab, &str, &str); 5] = [
    (PrefsTab::Annotate, "arrow", "Annotations"),
    (PrefsTab::Backdrop, "tab-backdrop", "Padding fill"),
    (PrefsTab::Advanced, "tab-advanced", "Advanced"),
    (PrefsTab::Watermark, "tab-watermark", "Watermark"),
    (PrefsTab::Profiles, "presets", "Profiles"),
];

fn title(text: &str, theme: &Theme) -> gpui::Div {
    div().text_size(px(12.5)).font_weight(FontWeight(650.0)).text_color(theme.text).mb(px(4.0)).child(text.to_string())
}

fn note(text: &str, theme: &Theme) -> gpui::Div {
    div().text_size(px(11.5)).line_height(px(17.0)).text_color(theme.muted).mb(px(14.0)).child(text.to_string())
}

fn pref_row(theme: &Theme) -> gpui::Div {
    div()
        .h(px(38.0))
        .pl(px(9.0))
        .pr(px(8.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .border_1()
        .border_color(theme.border)
        .rounded(px(8.0))
        .bg(theme.sunken)
}

fn grid() -> gpui::Div {
    div().flex().flex_wrap().gap(px(8.0))
}

/// Two columns of rows, as the old CSS grid had them.
fn cell() -> gpui::Div {
    div().flex_grow(1.0).flex_shrink(1.0).flex_basis(px(200.0)).min_w(px(0.0))
}

impl Editor {
    pub(super) fn render_prefs(
        &mut self,
        tab: PrefsTab,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = self.s(cx).clone();
        let profile_name = s.profile().name.clone();
        let hover = theme.hover;
        let tabs = TABS
            .iter()
            .enumerate()
            .map(|(i, (t, icon_name, label))| {
                let selected = *t == tab;
                let t = *t;
                let color = if selected { crate::theme::WHITE } else { theme.text };
                div()
                    .id(("prefs-tab", i))
                    .h(px(32.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .rounded(px(7.0))
                    .text_size(px(12.5))
                    .font_weight(FontWeight(550.0))
                    .text_color(color)
                    .cursor_pointer()
                    .map(|el| if selected { el.bg(theme.accent) } else { el.hover(move |st| st.bg(hover)) })
                    .child(icon(icon_name, 14.0, color))
                    .child(*label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.prefs = Some(t);
                        if t != PrefsTab::Profiles {
                            this.profile_form = None;
                        }
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        let content = match tab {
            PrefsTab::Annotate => self.prefs_annotate(&s, theme, cx),
            PrefsTab::Backdrop => self.prefs_backdrop(&s, theme, cx),
            PrefsTab::Advanced => self.prefs_advanced(&s, theme, cx),
            PrefsTab::Watermark => self.prefs_watermark(&s, theme, cx),
            PrefsTab::Profiles => self.prefs_profiles(&s, theme, window, cx),
        };
        let _ = window;

        div()
            .id("prefs")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(20.0))
            .bg(theme.scrim)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.color_picker.is_none() {
                    this.close_prefs(cx);
                }
            }))
            .child(
                div()
                    .id("prefs-panel")
                    .w(px(680.0))
                    .max_w_full()
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .bg(theme.bg)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .shadow(vec![gpui::BoxShadow::new(px(0.0), px(30.0), gpui::rgba(0x0000008c)).blur_radius(px(70.0))])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(px(46.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pl(px(18.0))
                            .pr(px(10.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex()
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight(650.0))
                                    .child("Preferences")
                                    .child(div().text_color(theme.muted).font_weight(FontWeight(500.0)).child(format!(" · {profile_name}"))),
                            )
                            .child(
                                icon_button("prefs-close", "close", 11.0, 26.0, theme)
                                    .tooltip_text("Close")
                                    .on_click(cx.listener(|this, _, _, cx| this.close_prefs(cx))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h(px(0.0))
                            .child(
                                div()
                                    .w(px(158.0))
                                    .flex_none()
                                    .py(px(12.0))
                                    .px(px(10.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .bg(theme.sunken)
                                    .border_r_1()
                                    .border_color(theme.border)
                                    .children(tabs),
                            )
                            .child(
                                div()
                                    .id("prefs-content")
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .h(px(440.0))
                                    .pt(px(18.0))
                                    .px(px(20.0))
                                    .pb(px(20.0))
                                    .overflow_y_scroll()
                                    .child(content),
                            ),
                    )
                    .child(
                        div()
                            .h(px(52.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(14.0))
                            .border_t_1()
                            .border_color(theme.border)
                            .bg(theme.sunken)
                            .child(button("prefs-reset", "Restore defaults", theme).text_color(theme.muted).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.update_settings(cx, |s| s.restore_defaults());
                                    this.profile_changed(cx);
                                    let name = this.s(cx).profile().name.clone();
                                    this.show_toast(
                                        format!("Restored the default colours, font, watermark and advanced style for “{name}”"),
                                        cx,
                                    );
                                },
                            )))
                            .child(
                                primary_button("prefs-done", "Done", theme)
                                    .px(px(18.0))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_prefs(cx))),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// A clickable colour square that opens the picker for `target`.
    fn well(
        &self,
        id: (&'static str, usize),
        color: Rgb,
        target: ColorTarget,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .size(px(24.0))
            .flex_none()
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme::from_rgb(color))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.open_picker(target, ev.position(), cx)))
            .into_any_element()
    }

    fn prefs_annotate(&self, s: &Settings, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let kit = s.kit();
        let hover = theme.hover;
        let rows = kit
            .palette
            .iter()
            .enumerate()
            .map(|(i, hex)| {
                let star = kit.default_idx == i;
                cell().child(
                    pref_row(theme)
                        .child(self.well(("pal-well", i), rgb(hex), ColorTarget::Palette(i), theme, cx))
                        .child(
                            div().flex_1().text_size(px(11.5)).font_weight(FontWeight(600.0)).child(hex.to_uppercase()),
                        )
                        .child(
                            div()
                                .id(("star", i))
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .hover(move |st| st.bg(hover))
                                .tooltip_text("Arm by default")
                                .child(icon("star", 13.0, if star { theme.accent } else { theme.border }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.update_settings(cx, |s| s.kit_mut().default_idx = i);
                                    this.color_idx = i;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .collect::<Vec<_>>();
        let current_font = kit.font.clone();
        let fonts = FONTS
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let id = f.id;
                seg_button(("font", i), f.label, current_font == f.id, theme)
                    .font_family(f.css.split(',').next().unwrap_or("sans-serif").trim().trim_matches('"').to_string())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_font(id, cx)))
            })
            .collect::<Vec<_>>();
        div()
            .child(title("Annotation palette", theme))
            .child(note("Six colours appear in the toolbar. The starred one is armed when a screenshot opens.", theme))
            .child(grid().children(rows))
            .child(title("Label font", theme).mt(px(16.0)))
            .child(note("Used for new text labels; a selected label changes too.", theme))
            .child(segmented(theme).mb(px(8.0)).children(fonts))
            .child(
                div()
                    .flex()
                    .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                        this.commit_font(cx);
                        window.focus(&this.focus, cx);
                        cx.stop_propagation();
                    }))
                    .child(field("font-custom", &self.inputs.font, "Or any installed font family, e.g. Inter", theme)),
            )
            .into_any_element()
    }

    fn prefs_backdrop(&self, s: &Settings, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let kit = s.kit();
        let solids = kit
            .solid_bg
            .iter()
            .enumerate()
            .map(|(i, hex)| {
                cell().child(
                    pref_row(theme)
                        .child(self.well(("solid-well", i), rgb(hex), ColorTarget::Solid(i), theme, cx))
                        .child(div().flex_1().text_size(px(11.5)).font_weight(FontWeight(600.0)).child(SOLID_NAMES[i])),
                )
            })
            .collect::<Vec<_>>();
        let grads = kit
            .grad_bg
            .iter()
            .enumerate()
            .map(|(i, g)| {
                cell().child(
                    pref_row(theme)
                        .gap(px(8.0))
                        .child(self.well(("grad-a", i), rgb(&g.a), ColorTarget::GradA(i), theme, cx))
                        .child(
                            div()
                                .w(px(44.0))
                                .flex_none()
                                .text_size(px(11.5))
                                .font_weight(FontWeight(600.0))
                                .child(GRAD_NAMES[i]),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .h(px(16.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(theme.border)
                                .bg(super::stage::gradient(rgb(&g.a), rgb(&g.b), Bounds::default())),
                        )
                        .child(self.well(("grad-b", i), rgb(&g.b), ColorTarget::GradB(i), theme, cx)),
                )
            })
            .collect::<Vec<_>>();
        div()
            .child(title("Padding fill presets", theme))
            .child(note("Used for the margin and any extra canvas. Pick one from Padding in the status bar.", theme))
            .child(self.pad_row(SliderId::PadPrefs, theme, cx))
            .child(div().text_size(px(11.0)).text_color(theme.muted).mb(px(15.0)).child(Self::pad_note(s)))
            .child(pop_label("Fill — solid", theme))
            .child(grid().mb(px(16.0)).children(solids))
            .child(pop_label("Fill — gradient", theme))
            .child(grid().children(grads))
            .into_any_element()
    }

    fn prefs_advanced(&self, s: &Settings, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let adv = s.kit().advanced;
        let rows = ADVANCED_META.iter().map(|m| {
            let v = adv.get(m.key);
            let frac = ((v - m.min) / (m.max - m.min)) as f32;
            div()
                .id(("adv", m.key as usize))
                .flex()
                .items_center()
                .gap(px(10.0))
                .mb(px(12.0))
                .tooltip_text(m.hint)
                .child(div().w(px(128.0)).flex_none().text_size(px(11.5)).text_color(theme.muted).child(m.label))
                .child(self.slider(SliderId::Advanced(m.key), frac, theme, cx))
                .child(
                    div()
                        .w(px(42.0))
                        .flex_none()
                        .text_right()
                        .text_size(px(11.5))
                        .font_weight(FontWeight(600.0))
                        .child(format_advanced(v, m.unit)),
                )
        });
        div()
            .child(title("Advanced", theme))
            .child(note(
                "Fine control over details that are normally left at their defaults — screenshot corner rounding and drop shadow, and the size of new annotations. Part of the profile, so it travels with it.",
                theme,
            ))
            .children(rows)
            .into_any_element()
    }

    fn prefs_watermark(&self, s: &Settings, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let kit = s.kit();
        let custom = kit.wm_mode == WmMode::Custom;
        div()
            .child(title("Watermark", theme))
            .child(note(
                "Shown in the corner of the export while the Watermark switch in the status bar is on. Saved with the profile, so it travels when you export one.",
                theme,
            ))
            .child(
                segmented(theme)
                    .mb(px(10.0))
                    .child(seg_button("wm-default", "escribo", !custom, theme).on_click(cx.listener(|this, _, _, cx| {
                        this.update_settings(cx, |s| s.kit_mut().wm_mode = WmMode::Default)
                    })))
                    .child(seg_button("wm-custom", "Custom text", custom, theme).on_click(cx.listener(|this, _, window, cx| {
                        this.update_settings(cx, |s| s.kit_mut().wm_mode = WmMode::Custom);
                        this.focus_field(Field::WmText, window, cx);
                    }))),
            )
            .when(custom, |el| {
                el.child(div().flex().mb(px(14.0)).child(field("wm-text", &self.inputs.wm_text, "Your watermark text", theme)))
            })
            .child(pop_label("Colours", theme))
            .child(
                grid()
                    .child(cell().child(
                        pref_row(theme)
                            .child(self.well(("wm-color", 0), rgb(&kit.wm_color), ColorTarget::WmColor, theme, cx))
                            .child(div().text_size(px(11.5)).font_weight(FontWeight(600.0)).child("Text")),
                    ))
                    .child(cell().child(
                        pref_row(theme)
                            .child(self.well(("wm-bg", 0), rgb(&kit.wm_bg), ColorTarget::WmBg, theme, cx))
                            .child(div().text_size(px(11.5)).font_weight(FontWeight(600.0)).child("Background")),
                    )),
            )
            .into_any_element()
    }

    fn prefs_profiles(&self, s: &Settings, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let last = s.profiles.len() <= 1;
        let hover = theme.hover;
        let _ = window;
        let rows = s.profiles.iter().enumerate().map(|(i, p)| {
            let active = p.id == s.active_profile;
            let n = p.presets.len();
            let (id_switch, id_rename, id_dup, id_del) = (p.id.clone(), p.id.clone(), p.id.clone(), p.id.clone());
            let profile_for_export = p.clone();
            let name = p.name.clone();
            div()
                .id(("profile-row", i))
                .h(px(34.0))
                .pl(px(8.0))
                .pr(px(4.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(7.0))
                .hover(move |st| st.bg(hover))
                .child(div().size(px(6.0)).flex_none().rounded_full().when(active, |d| d.bg(theme.accent)))
                .child(
                    div()
                        .id(("profile-name", i))
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(550.0))
                        .cursor_pointer()
                        .tooltip_text(format!("Switch to “{}”", p.name))
                        .child(p.name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let mut switched = None;
                            this.update_settings(cx, |s| switched = s.set_active_profile(&id_switch));
                            if let Some(name) = switched {
                                this.profile_changed(cx);
                                this.show_toast(format!("Switched to “{name}”"), cx);
                            }
                        })),
                )
                .child(
                    div()
                        .mr(px(4.0))
                        .text_size(px(10.5))
                        .text_color(theme.muted)
                        .child(format!("{n} preset{}", plural(n))),
                )
                .child(icon_button(("profile-rename", i), "rename", 12.0, 22.0, theme).tooltip_text("Rename").on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.open_profile_form(ProfileForm::Rename(id_rename.clone()), window, cx)
                    }),
                ))
                .child(
                    icon_button(("profile-dup", i), "duplicate", 12.0, 22.0, theme).tooltip_text("Duplicate").on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.open_profile_form(ProfileForm::Duplicate(id_dup.clone()), window, cx)
                        }),
                    ),
                )
                .child(
                    icon_button(("profile-export", i), "export", 12.0, 22.0, theme)
                        .tooltip_text("Export this profile, presets included")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let name = format!("{}.escribo-profile.json", slug(&profile_for_export.name));
                            this.export_json(Settings::profile_file(&profile_for_export), name, "Profile", window, cx);
                        })),
                )
                .child(
                    icon_button(("profile-delete", i), "close", 11.0, 22.0, theme)
                        .when(last, |el| el.opacity(0.35))
                        .tooltip_text(if last { "Keep at least one profile" } else { "Delete" })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.s(cx).profiles.len() <= 1 {
                                this.show_toast("Keep at least one profile", cx);
                                return;
                            }
                            let id = id_del.clone();
                            let msg = format!("Delete profile “{name}” and its {n} preset{}?", plural(n));
                            this.confirm(msg, window, cx, move |this, cx| {
                                let mut deleted = None;
                                this.update_settings(cx, |s| deleted = s.delete_profile(&id));
                                if let Some(name) = deleted {
                                    this.profile_changed(cx);
                                    this.show_toast(format!("Deleted “{name}”"), cx);
                                }
                            });
                        })),
                )
        });
        let form_label = match &self.profile_form {
            Some(ProfileForm::Rename(_)) => "Rename",
            Some(ProfileForm::Duplicate(_)) => "Duplicate",
            _ => "Create",
        };
        div()
            .child(title("Profiles", theme))
            .child(note(
                "A profile is the global kit — the annotation palette, padding fills, label font, watermark and advanced style — and holds its own presets. The other tabs edit the active one. Export a profile to hand a teammate the whole setup.",
                theme,
            ))
            .child(div().flex().flex_col().gap(px(2.0)).mt(px(4.0)).mb(px(10.0)).children(rows))
            .when(self.profile_form.is_some(), |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .mb(px(10.0))
                        .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                            this.submit_profile_form(window, cx);
                            cx.stop_propagation();
                        }))
                        .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                            this.profile_form = None;
                            window.focus(&this.focus, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .child(field("profile-name", &self.inputs.profile_name, "Profile name", theme))
                        .child(primary_button("profile-form-save", form_label, theme).on_click(cx.listener(|this, _, window, cx| {
                            this.submit_profile_form(window, cx)
                        })))
                        .child(button("profile-form-cancel", "Cancel", theme).on_click(cx.listener(|this, _, window, cx| {
                            this.profile_form = None;
                            window.focus(&this.focus, cx);
                            cx.notify();
                        }))),
                )
            })
            .child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(button("profile-new", "New profile…", theme).on_click(cx.listener(|this, _, window, cx| {
                        this.open_profile_form(ProfileForm::New, window, cx)
                    })))
                    .child(button("profile-import", "Import…", theme).on_click(cx.listener(|this, _, window, cx| {
                        this.import_file(window, cx)
                    }))),
            )
            .into_any_element()
    }

    fn open_profile_form(&mut self, form: ProfileForm, window: &mut Window, cx: &mut Context<Self>) {
        let s = self.s(cx);
        let name_of = |id: &str| s.profiles.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
        let value = match &form {
            ProfileForm::New => String::new(),
            ProfileForm::Rename(id) => name_of(id),
            ProfileForm::Duplicate(id) => format!("{} copy", name_of(id)),
        };
        self.profile_form = Some(form);
        self.inputs.profile_name.update(cx, |st, cx| {
            st.emplace(&value, cx);
            st.select_document(cx);
        });
        self.focus_field(Field::ProfileName, window, cx);
        cx.notify();
    }

    fn submit_profile_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.profile_form.clone() else { return };
        let name = self.inputs.profile_name.read(cx).as_str().trim().to_string();
        if name.is_empty() {
            self.focus_field(Field::ProfileName, window, cx);
            return;
        }
        match form {
            ProfileForm::Rename(id) => {
                self.update_settings(cx, |s| {
                    s.rename_profile(&id, &name);
                });
            }
            ProfileForm::New => {
                let mut made = String::new();
                self.update_settings(cx, |s| made = s.create_profile(&name, None));
                self.profile_changed(cx);
                self.show_toast(format!("Created “{made}”"), cx);
            }
            ProfileForm::Duplicate(id) => {
                let mut made = String::new();
                self.update_settings(cx, |s| made = s.create_profile(&name, Some(&id)));
                self.profile_changed(cx);
                self.show_toast(format!("Duplicated as “{made}”"), cx);
            }
        }
        self.profile_form = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /* ======================================================================
    Colour picker
    ====================================================================== */

    fn target_color(&self, target: ColorTarget, cx: &gpui::App) -> Rgb {
        let kit = self.s(cx).kit();
        rgb(match target {
            ColorTarget::Palette(i) => &kit.palette[i],
            ColorTarget::Solid(i) => &kit.solid_bg[i],
            ColorTarget::GradA(i) => &kit.grad_bg[i].a,
            ColorTarget::GradB(i) => &kit.grad_bg[i].b,
            ColorTarget::WmColor => &kit.wm_color,
            ColorTarget::WmBg => &kit.wm_bg,
        })
    }

    fn open_picker(&mut self, target: ColorTarget, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        let c = self.target_color(target, cx);
        let (h, s, v) = rgb_to_hsv(c);
        self.color_picker = Some(ColorPicker { target, h, s, v, anchor });
        super::set_input_unless_focused(&self.inputs.hex, &c.hex(), false, cx);
        cx.notify();
    }

    fn set_target_color(&mut self, target: ColorTarget, c: Rgb, cx: &mut Context<Self>) {
        let hex = c.hex();
        self.update_settings(cx, |s| {
            let kit = s.kit_mut();
            match target {
                ColorTarget::Palette(i) => kit.palette[i] = hex.clone(),
                ColorTarget::Solid(i) => kit.solid_bg[i] = hex.clone(),
                ColorTarget::GradA(i) => kit.grad_bg[i].a = hex.clone(),
                ColorTarget::GradB(i) => kit.grad_bg[i].b = hex.clone(),
                ColorTarget::WmColor => kit.wm_color = hex.clone(),
                ColorTarget::WmBg => kit.wm_bg = hex.clone(),
            }
        });
    }

    pub(crate) fn apply_picker(&mut self, cx: &mut Context<Self>) {
        let Some(p) = &self.color_picker else { return };
        let (target, c) = (p.target, hsv_to_rgb(p.h, p.s, p.v));
        self.set_target_color(target, c, cx);
        let focused = self.focused_field == Some(Field::Hex);
        super::set_input_unless_focused(&self.inputs.hex, &c.hex(), focused, cx);
        cx.notify();
    }

    pub(crate) fn commit_hex(&mut self, cx: &mut Context<Self>) {
        let Some(p) = &self.color_picker else { return };
        let raw = self.inputs.hex.read(cx).as_str().trim().to_string();
        let hex = if raw.starts_with('#') { raw } else { format!("#{raw}") };
        let target = p.target;
        match Rgb::parse(&hex) {
            Some(c) => {
                let (h, s, v) = rgb_to_hsv(c);
                if let Some(p) = &mut self.color_picker {
                    (p.h, p.s, p.v) = (h, s, v);
                }
                self.set_target_color(target, c, cx);
            }
            None => {
                let c = self.target_color(target, cx);
                super::set_input_unless_focused(&self.inputs.hex, &c.hex(), false, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn render_color_picker(
        &mut self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(p) = &self.color_picker else { return any(div()) };
        let (h, s, v) = (p.h, p.s, p.v);
        let current = hsv_to_rgb(h, s, v);
        let hue_color = hsv_to_rgb(h, 1.0, 1.0);
        let entity = cx.entity();
        let entity2 = cx.entity();
        let viewport = window.viewport_size();
        // Keep the picker inside the window.
        let (pw, ph) = (232.0, 262.0);
        let x = (f32::from(p.anchor.x) - pw / 2.0).clamp(8.0, f32::from(viewport.width) - pw - 8.0);
        let y = (f32::from(p.anchor.y) + 14.0).min(f32::from(viewport.height) - ph - 8.0).max(8.0);
        let swatches = {
            let kit = self.s(cx).kit();
            kit.palette.clone()
        };
        let quick = swatches.into_iter().enumerate().map(|(i, hex)| {
            div()
                .id(("pick-quick", i))
                .size(px(20.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(theme.border)
                .bg(theme::from_rgb(rgb(&hex)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let (Some(p), Some(c)) = (&mut this.color_picker, Rgb::parse(&hex)) {
                        (p.h, p.s, p.v) = rgb_to_hsv(c);
                    }
                    this.apply_picker(cx);
                }))
        });

        deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.commit_hex(cx);
                        this.color_picker = None;
                        window.focus(&this.focus, cx);
                        cx.notify();
                    }),
                )
                .child(
                    super::widgets::panel(theme)
                        .id("color-picker")
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(pw))
                        .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        // Saturation × value.
                        .child(
                            div()
                                .id("pick-sv")
                                .h(px(140.0))
                                .w_full()
                                .rounded(px(6.0))
                                .overflow_hidden()
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                                        this.drag = Some(super::stage::Drag::Slider(SliderId::Sv));
                                        this.slider_to(SliderId::Sv, ev.position, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(
                                    canvas(
                                        move |b, _, cx| {
                                            entity.update(cx, |e, _| {
                                                e.sliders.insert(SliderId::Sv, b);
                                            });
                                        },
                                        move |b, _, window, _| {
                                            window.paint_quad(PaintQuad::new(b).background(theme::from_rgb(hue_color)));
                                            window.paint_quad(PaintQuad::new(b).background(gpui::linear_gradient(
                                                90.0,
                                                gpui::linear_color_stop(gpui::rgba(0xffffffff), 0.0),
                                                gpui::linear_color_stop(gpui::rgba(0xffffff00), 1.0),
                                            )));
                                            window.paint_quad(PaintQuad::new(b).background(gpui::linear_gradient(
                                                180.0,
                                                gpui::linear_color_stop(gpui::rgba(0x00000000), 0.0),
                                                gpui::linear_color_stop(gpui::rgba(0x000000ff), 1.0),
                                            )));
                                            let cx_ = b.origin.x + b.size.width * s;
                                            let cy_ = b.origin.y + b.size.height * (1.0 - v);
                                            let ring = Bounds::new(
                                                point(cx_ - px(6.0), cy_ - px(6.0)),
                                                size(px(12.0), px(12.0)),
                                            );
                                            window.paint_quad(
                                                PaintQuad::new(ring)
                                                    .corner_radii(px(6.0))
                                                    .border_widths(px(2.0))
                                                    .border_color(gpui::rgb(0xffffff)),
                                            );
                                        },
                                    )
                                    .size_full(),
                                ),
                        )
                        // Hue.
                        .child(
                            div()
                                .id("pick-hue")
                                .h(px(14.0))
                                .w_full()
                                .rounded(px(7.0))
                                .overflow_hidden()
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                                        this.drag = Some(super::stage::Drag::Slider(SliderId::Hue));
                                        this.slider_to(SliderId::Hue, ev.position, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(
                                    canvas(
                                        move |b, _, cx| {
                                            entity2.update(cx, |e, _| {
                                                e.sliders.insert(SliderId::Hue, b);
                                            });
                                        },
                                        move |b, _, window, _| {
                                            let seg = b.size.width / 6.0;
                                            for i in 0..6 {
                                                let a = hsv_to_rgb(i as f32 * 60.0, 1.0, 1.0);
                                                let c = hsv_to_rgb((i + 1) as f32 * 60.0, 1.0, 1.0);
                                                let sb = Bounds::new(
                                                    point(b.origin.x + seg * i as f32, b.origin.y),
                                                    size(seg + px(0.5), b.size.height),
                                                );
                                                window.paint_quad(
                                                    PaintQuad::new(sb).background(
                                                        gpui::linear_gradient(
                                                            90.0,
                                                            gpui::linear_color_stop(theme::from_rgb(a), 0.0),
                                                            gpui::linear_color_stop(theme::from_rgb(c), 1.0),
                                                        )
                                                        .color_space(gpui::ColorSpace::Srgb),
                                                    ),
                                                );
                                            }
                                            let x = b.origin.x + px(7.0) + (b.size.width - px(14.0)) * (h / 360.0);
                                            let knob =
                                                Bounds::new(point(x - px(7.0), b.origin.y), size(px(14.0), px(14.0)));
                                            window.paint_quad(
                                                PaintQuad::new(knob)
                                                    .corner_radii(px(7.0))
                                                    .border_widths(px(2.0))
                                                    .border_color(gpui::rgb(0xffffff)),
                                            );
                                        },
                                    )
                                    .size_full(),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .size(px(29.0))
                                        .flex_none()
                                        .rounded(px(7.0))
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme::from_rgb(current)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                                            this.commit_hex(cx);
                                            cx.stop_propagation();
                                        }))
                                        .child(field("pick-hex", &self.inputs.hex, "#RRGGBB", theme)),
                                ),
                        )
                        .child(div().flex().gap(px(6.0)).children(quick)),
                ),
        )
        .priority(3usize)
        .into_any_element()
    }
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let h = (h % 360.0 + 360.0) % 360.0 / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb::new(to(r), to(g), to(b))
}

pub fn rgb_to_hsv(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips() {
        for hex in ["#FF007F", "#12B76A", "#2E7DFF", "#15151A", "#FFFFFF", "#000000", "#EFE7DC"] {
            let c = Rgb::parse(hex).unwrap();
            let (h, s, v) = rgb_to_hsv(c);
            assert_eq!(hsv_to_rgb(h, s, v), c, "{hex}");
        }
    }
}
