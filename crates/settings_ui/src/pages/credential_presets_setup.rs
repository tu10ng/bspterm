use gpui::{AnyElement, ScrollHandle, Window, prelude::*};
use i18n::t;
use terminal::session_store::{CredentialPreset, SessionStoreEntity, SessionStoreEvent};
use ui::{prelude::*, Button, ButtonStyle, Divider, IconButton, Label, LabelSize, Tooltip, h_flex, v_flex};

use crate::SettingsWindow;

pub(crate) fn render_credential_presets_page(
    settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let session_store = SessionStoreEntity::try_global(cx);
    let presets = session_store
        .as_ref()
        .map(|s| s.read(cx).credential_presets().to_vec())
        .unwrap_or_default();

    if let Some(session_store) = &session_store {
        let weak_settings = cx.entity().downgrade();
        cx.subscribe(session_store, move |_, _, event, cx| {
            if matches!(event, SessionStoreEvent::CredentialPresetChanged) {
                weak_settings
                    .update(cx, |_, cx| {
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    }

    v_flex()
        .id("credential-presets-page")
        .min_w_0()
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .child(
            v_flex()
                .gap_1()
                .child(Label::new(t("credential_presets.title")).size(LabelSize::Large))
                .child(
                    Label::new(t("credential_presets.description"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(render_add_preset_section(settings_window, window, cx))
        .when(!presets.is_empty(), |this| {
            this.child(
                v_flex()
                    .mt_4()
                    .gap_2()
                    .child(Label::new(t("credential_presets.saved_presets")))
                    .children(
                        presets.iter().enumerate().flat_map(|(i, preset)| {
                            let mut elements = vec![render_preset_item(preset, window, cx)];
                            if i + 1 < presets.len() {
                                elements.push(Divider::horizontal().into_any_element());
                            }
                            elements
                        }),
                    ),
            )
        })
        .when(presets.is_empty(), |this| {
            this.child(
                v_flex()
                    .mt_4()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_dashed()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        Label::new(t("credential_presets.no_presets"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
        })
        .into_any_element()
}

fn render_add_preset_section(
    _settings_window: &SettingsWindow,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let name_editor = window.use_keyed_state("credential-preset-name", cx, |window, cx| {
        let mut editor = editor::Editor::single_line(window, cx);
        editor.set_placeholder_text(&t("credential_presets.preset_name"), window, cx);
        editor
    });

    let username_editor = window.use_keyed_state("credential-preset-username", cx, |window, cx| {
        let mut editor = editor::Editor::single_line(window, cx);
        editor.set_placeholder_text(&t("credential_presets.username"), window, cx);
        editor
    });

    let password_editor = window.use_keyed_state("credential-preset-password", cx, |window, cx| {
        let mut editor = editor::Editor::single_line(window, cx);
        editor.set_placeholder_text(&t("credential_presets.password"), window, cx);
        editor
    });

    let theme_colors = cx.theme().colors();
    let name_clone = name_editor.clone();
    let username_clone = username_editor.clone();
    let password_clone = password_editor.clone();

    v_flex()
        .mt_4()
        .p_3()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(theme_colors.border)
        .bg(theme_colors.surface_background.opacity(0.3))
        .child(Label::new(t("credential_presets.add_new_preset")).size(LabelSize::Small))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .h_8()
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme_colors.border)
                        .bg(theme_colors.editor_background)
                        .child(name_editor),
                )
                .child(
                    div()
                        .flex_1()
                        .h_8()
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme_colors.border)
                        .bg(theme_colors.editor_background)
                        .child(username_editor),
                )
                .child(
                    div()
                        .flex_1()
                        .h_8()
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme_colors.border)
                        .bg(theme_colors.editor_background)
                        .child(password_editor),
                ),
        )
        .child(
            h_flex().w_full().justify_end().child(
                Button::new("add-credential-preset", t("credential_presets.add_preset"))
                    .style(ButtonStyle::Filled)
                    .size(ButtonSize::Compact)
                    .on_click(cx.listener(move |_, _, window, cx| {
                        let name = name_clone.read(cx).text(cx);
                        let username = username_clone.read(cx).text(cx);
                        let password = password_clone.read(cx).text(cx);

                        if name.is_empty() || username.is_empty() {
                            return;
                        }

                        if let Some(session_store) = SessionStoreEntity::try_global(cx) {
                            let preset = CredentialPreset::new(name, username, password);
                            session_store.update(cx, |store, cx| {
                                store.add_credential_preset(preset, cx);
                            });

                            name_clone.update(cx, |editor, cx| {
                                editor.set_text("", window, cx);
                            });
                            username_clone.update(cx, |editor, cx| {
                                editor.set_text("", window, cx);
                            });
                            password_clone.update(cx, |editor, cx| {
                                editor.set_text("", window, cx);
                            });
                        }
                    })),
            ),
        )
        .into_any_element()
}

fn render_preset_item(
    preset: &CredentialPreset,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let preset_id = preset.id;
    let preset_name = preset.name.clone();
    let username = preset.username.clone();

    h_flex()
        .w_full()
        .py_2()
        .justify_between()
        .child(
            v_flex()
                .gap_0p5()
                .child(Label::new(preset_name))
                .child(
                    Label::new(t("credential_presets.user_label").replace("{}", &username))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(
            h_flex()
                .gap_1()
                .child(
                    IconButton::new(format!("delete-{}", preset_id), IconName::Trash)
                        .icon_size(IconSize::Small)
                        .icon_color(Color::Muted)
                        .tooltip(Tooltip::text(t("credential_presets.delete_preset")))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            if let Some(session_store) = SessionStoreEntity::try_global(cx) {
                                session_store.update(cx, |store, cx| {
                                    store.remove_credential_preset(preset_id, cx);
                                });
                            }
                        })),
                ),
        )
        .into_any_element()
}
