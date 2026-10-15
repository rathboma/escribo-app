//! Confirmation prompts drawn in the window, in the app's own style, in place
//! of gpui's plain fallback (Linux has no native message box to call).

use crate::app::Shared;
use crate::theme::Theme;
use gpui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, PromptButton, PromptHandle, PromptLevel, PromptResponse, Render,
    RenderablePromptHandle, StatefulInteractiveElement, Styled, Window, div, px,
};

pub struct Prompt {
    message: String,
    detail: Option<String>,
    actions: Vec<PromptButton>,
    focus: FocusHandle,
}

pub fn build(
    _level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let view = cx.new(|cx| Prompt {
        message: message.to_string(),
        detail: detail.map(str::to_string),
        actions: actions.to_vec(),
        focus: cx.focus_handle(),
    });
    handle.with_view(view, window, cx)
}

impl EventEmitter<PromptResponse> for Prompt {}

impl Focusable for Prompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Prompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_settings(Shared::get(cx).settings.read(cx).settings.theme);
        let hover = theme.hover;
        let last = self.actions.len().saturating_sub(1);
        let buttons = self.actions.iter().enumerate().map(|(i, action)| {
            // The first answer is the one asked about (Delete); it gets the
            // accent. The last is the way out.
            let primary = i == 0;
            div()
                .id(i)
                .h(px(31.0))
                .px(px(14.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .text_size(px(12.0))
                .font_weight(FontWeight(if primary { 650.0 } else { 600.0 }))
                .cursor_pointer()
                .map(|el| {
                    if primary {
                        el.bg(theme.accent).text_color(crate::theme::WHITE).hover(|s| s.opacity(0.92))
                    } else {
                        el.border_1().border_color(theme.border).text_color(theme.text).hover(move |s| s.bg(hover))
                    }
                })
                .child(action.label().clone())
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(PromptResponse(i));
                    cx.stop_propagation();
                }))
        });
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(move |_, ev: &KeyDownEvent, _, cx| match ev.keystroke.key.as_str() {
                "escape" => cx.emit(PromptResponse(last)),
                "enter" => cx.emit(PromptResponse(0)),
                _ => {}
            }))
            .occlude()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.scrim)
            .font_family(crate::settings::UI_FONT)
            .child(
                div()
                    .w(px(340.0))
                    .p(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .bg(theme.bg)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.0))
                    .shadow(vec![gpui::BoxShadow::new(px(0.0), px(30.0), gpui::rgba(0x0000008c)).blur_radius(px(70.0))])
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight(650.0))
                            .text_color(theme.text)
                            .child(self.message.clone()),
                    )
                    .children(self.detail.clone().map(|d| div().text_size(px(12.0)).text_color(theme.muted).child(d)))
                    .child(div().flex().flex_row_reverse().gap(px(8.0)).children(buttons.rev())),
            )
    }
}

use gpui::prelude::FluentBuilder as _;
