//! Responsive Next-shell dialogs and overlays (`docs/ui-redesign/01-design-spec.md`, `03-phases.md` P13).
//!
//! Shared chrome adapts across size classes:
//! - `Compact` (`< 700px`): full-screen with left `arrow-left` back button, 1px rules, scrollable body, >= 44px touch targets.
//! - `Medium` (`700..1079px`): centered modal with `24px` outer margin, max `(w - 48, h - 48)`.
//! - `Wide` (`>= 1080px`): centered modal capped at `<= 1080 x 740` (narrower for prompts/pickers).
//!
//! Zero `theme::at`, `theme::ink`, or `INK_PASSES`; single-pass `icons::mono` and `ui::tokens` only.

use std::fmt;

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{
    checkbox, column, container, image, mouse_area, opaque, pick_list, radio, row, scrollable,
    stack, text, text_editor, text_input, Space,
};
use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Padding, Point, Rectangle,
    Renderer, Size, Theme,
};

use super::fonts::{BARLOW, BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_MEDIUM, BSC_SEMIBOLD};
use super::kit::{self, FieldProps};
use super::layout::SizeClass;
use super::tokens::{
    self, Density, RADIUS_CONTROL, RADIUS_DIALOG, RADIUS_TILE, SPACE_1, SPACE_2, SPACE_3, SPACE_4,
    TEXT_11, TEXT_12, TEXT_13, TEXT_14, TEXT_16,
};
use super::Msg as UiMsg;
use crate::area_dialog::{self, AreaDialog, AreaDialogMsg, PostSwitch};
use crate::attr_dialog::{AttrDialog, AttrMsg, ATTRS};
use crate::breakdown_dialog::{self, BreakdownDialog, BreakdownMsg};
use crate::color_dialog::{
    self, Area as ColorArea, ColorDialog, ColorMsg, RgbField, Tab as ColorTab,
};
use crate::dialog::{Action, Dialog, DialogMsg};
use crate::disto::{Disto, DistoMsg, Phase as DistoPhase};
#[cfg(windows)]
use crate::guide::INTEGRATION_TOPIC;
use crate::guide::{Block as GuideBlock, Guide, GuideMsg, TOPICS};
use crate::icons::mono;
use crate::options::{BoolKey, FormatKey, NumKey, Options, StrKey, FONTS, FONT_SIZES};
use crate::options_dialog::{
    NumChoice, OptMsg, OptionsDialog, Tab as OptTab, ALIGNMENT_STYLES, DRAWING_MODES,
    INK_RECOGNITIONS, INPUT_MODES, POINTERS, PRECISIONS, ROUNDING_MODES,
};
use crate::page_layout::{
    paper_label, Form, LayoutMsg, PageLayoutDialog, CAPTIONS, OPTIONS as LAYOUT_OPTIONS, PAPERS,
};
use crate::print::{PreviewMsg, PrintPreview};
use crate::room_shapes::RoomShapes;
use crate::shortcuts::{Shortcuts, ShortcutsMsg, GROUPS as SHORTCUT_GROUPS};
use crate::subject_info::{
    SubjectDialog, SubjectMsg, Tab as SubjectTab, FIELDS as SUBJECT_FIELDS, GPS_PLACEHOLDER,
    GRID_ROWS, IDX_GPS, IDX_INSPECTION,
};
use crate::text_manager::{visible_labels, TextManager, TextManagerMsg, RANGES as ALPHA_RANGES};
use crate::{App, Message};

const MAX_WIDE_W: f32 = 1280.0;
const MAX_WIDE_H: f32 = 960.0;

#[derive(Debug, Clone, PartialEq)]
struct LabelValue(&'static str, &'static str);

impl Eq for LabelValue {}

impl fmt::Display for LabelValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StringChoice(String, String);

impl fmt::Display for StringChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaperChoice(sketch_core::print_layout::PaperSize);

impl fmt::Display for PaperChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(paper_label(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormChoice(Form, sketch_core::print_layout::PaperSize);

impl fmt::Display for FormChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.label(self.1))
    }
}

/// Returns all currently open Next-shell modal dialog layers in z-order.
pub fn layers<'a>(app: &'a App, size_class: SizeClass, d: Density) -> Vec<Element<'a, Message>> {
    let mut out = Vec::new();
    let win = app.window_size;

    if let Some(layer) = print_preview_layer(&app.print_preview, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = subject_info_layer(&app.subject_dialog, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = options_dialog_layer(&app.options_dialog, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = page_layout_layer(&app.page_layout_dialog, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = breakdown_layer(app, &app.breakdown, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = attr_dialog_layer(&app.attr_dialog, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = area_dialog_layer(&app.area_dialog, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = color_dialog_layer(&app.color, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = room_shapes_layer(&app.room_shapes, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = disto_layer(&app.disto, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = text_manager_layer(app, &app.text_manager, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = undo_history_layer(app, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = shortcuts_layer(&app.shortcuts, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = guide_layer(&app.guide, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = geo_menu_layer(app.geo_menu.open, win, size_class, d) {
        out.push(layer);
    }
    if let Some(layer) = autosave_layer(app, win, size_class, d) {
        out.push(layer);
    }
    if let Some(dlg) = &app.dialog {
        out.push(alert_dialog_layer(dlg, win, size_class, d));
    }

    out
}

// ---------------------------------------------------------------------------
// Shared responsive dialog frame & styling helpers
// ---------------------------------------------------------------------------

struct DialogFrame<'a> {
    title: String,
    subtitle: Option<String>,
    preferred_w: f32,
    preferred_h: Option<f32>,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
    on_close: Option<Message>,
}

fn dialog_frame<'a>(
    frame: DialogFrame<'a>,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Element<'a, Message> {
    let is_compact = size_class == SizeClass::Compact;
    let margin = match size_class {
        SizeClass::Compact => 0.0,
        SizeClass::Medium => 24.0,
        SizeClass::Wide => 32.0,
    };
    let max_w = if size_class == SizeClass::Wide {
        MAX_WIDE_W
    } else {
        (win.width - margin * 2.0).max(320.0)
    };
    let max_h = if size_class == SizeClass::Wide {
        MAX_WIDE_H.min((win.height - margin * 2.0).max(320.0))
    } else {
        (win.height - margin * 2.0).max(320.0)
    };

    let mut header_row = row![].spacing(SPACE_2).align_y(Alignment::Center);
    if is_compact {
        if let Some(close) = frame.on_close.clone() {
            header_row = header_row.push(kit::icon_button("arrow-left", false, Some(close), d));
        }
    }

    let mut title_col = column![text(frame.title)
        .font(BARLOW_SEMIBOLD)
        .size(TEXT_16)
        .color(tokens::text())]
    .spacing(2)
    .width(Length::Fill);

    if let Some(sub) = frame.subtitle {
        title_col = title_col.push(text(sub).font(BARLOW).size(TEXT_12).color(tokens::muted()));
    }
    header_row = header_row.push(title_col);

    if !is_compact {
        if let Some(close) = frame.on_close.clone() {
            header_row = header_row.push(kit::icon_button("x", false, Some(close), d));
        }
    }

    let header = container(header_row)
        .padding(Padding {
            top: SPACE_3,
            bottom: SPACE_3,
            left: SPACE_4,
            right: SPACE_3,
        })
        .width(Length::Fill);

    let mut card_col = column![header, kit::divider()].width(Length::Fill).height(
        if is_compact || frame.preferred_h.is_some() {
            Length::Fill
        } else {
            Length::Shrink
        },
    );

    card_col = card_col.push(
        container(frame.body)
            .width(Length::Fill)
            .height(if is_compact || frame.preferred_h.is_some() {
                Length::Fill
            } else {
                Length::Shrink
            })
            .padding(Padding {
                top: SPACE_3,
                bottom: SPACE_3,
                left: SPACE_4,
                right: SPACE_4,
            }),
    );

    if let Some(footer) = frame.footer {
        card_col = card_col.push(kit::divider()).push(
            container(footer)
                .padding(Padding {
                    top: SPACE_3,
                    bottom: SPACE_3,
                    left: SPACE_4,
                    right: SPACE_4,
                })
                .width(Length::Fill),
        );
    }

    let mut card = container(card_col).style(move |_| container::Style {
        background: Some(Background::Color(tokens::panel())),
        border: Border {
            color: if is_compact {
                Color::TRANSPARENT
            } else {
                tokens::rule()
            },
            width: if is_compact { 0.0 } else { 1.0 },
            radius: if is_compact { 0.0 } else { RADIUS_DIALOG }.into(),
        },
        shadow: if is_compact {
            iced::Shadow::default()
        } else {
            tokens::floating_shadow()
        },
        ..container::Style::default()
    });

    card = if is_compact {
        card.width(Length::Fill).height(Length::Fill)
    } else {
        let w = frame
            .preferred_w
            .min(max_w)
            .min((win.width - margin * 2.0).max(280.0));
        let card = card.width(Length::Fixed(w));
        if let Some(ph) = frame.preferred_h {
            card.height(Length::Fixed(ph.min(max_h)))
        } else {
            card.max_height(max_h)
        }
    };

    let inner = opaque(mouse_area(card).on_press(Message::Ui(UiMsg::Noop)));
    if is_compact {
        return inner;
    }

    let backdrop = container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(margin as u16)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.56))),
            ..container::Style::default()
        });

    if let Some(dismiss) = frame.on_close {
        opaque(mouse_area(backdrop).on_press(dismiss))
    } else {
        opaque(backdrop)
    }
}

fn label_text<'a>(s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .font(BARLOW_MEDIUM)
        .size(TEXT_12)
        .color(tokens::muted())
        .into()
}

fn body_text<'a>(s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .font(BARLOW)
        .size(TEXT_13)
        .color(tokens::text())
        .into()
}

fn section_box<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(SPACE_3)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::field())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_TILE.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn pick_style(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let border_col = match status {
        pick_list::Status::Hovered | pick_list::Status::Opened { .. } => tokens::accent(),
        pick_list::Status::Active => tokens::rule(),
    };
    pick_list::Style {
        text_color: tokens::text(),
        placeholder_color: tokens::muted(),
        handle_color: tokens::muted(),
        background: Background::Color(tokens::input_bg()),
        border: Border {
            color: border_col,
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
    }
}

fn pick_menu_style(_: &Theme) -> iced::overlay::menu::Style {
    iced::overlay::menu::Style {
        background: Background::Color(tokens::panel()),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        text_color: tokens::text(),
        selected_text_color: tokens::on_accent(),
        selected_background: Background::Color(tokens::accent()),
        shadow: tokens::floating_shadow(),
    }
}

fn editor_style(_: &Theme, status: text_editor::Status) -> text_editor::Style {
    let focused = matches!(status, text_editor::Status::Focused { .. });
    let border_col = match status {
        text_editor::Status::Disabled => tokens::rule(),
        _ if focused => tokens::accent(),
        text_editor::Status::Hovered => tokens::muted(),
        _ => tokens::rule(),
    };
    text_editor::Style {
        background: Background::Color(tokens::input_bg()),
        border: Border {
            color: border_col,
            width: if focused { 2.0 } else { 1.0 },
            radius: RADIUS_CONTROL.into(),
        },
        placeholder: tokens::muted(),
        value: tokens::text(),
        selection: tokens::accent_soft(),
    }
}

pub(crate) fn check_row<'a>(
    label: impl Into<String>,
    checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
    d: Density,
) -> Element<'a, Message> {
    container(
        checkbox(checked)
            .label(label.into())
            .on_toggle(on_toggle)
            .font(BARLOW)
            .text_size(TEXT_13)
            .size(if d == Density::Touch { 18.0 } else { 15.0 })
            .spacing(8.0)
            .style(|_, status| {
                let is_checked = matches!(
                    status,
                    checkbox::Status::Active { is_checked: true }
                        | checkbox::Status::Hovered { is_checked: true }
                );
                checkbox::Style {
                    background: Background::Color(if is_checked {
                        tokens::accent()
                    } else {
                        tokens::input_bg()
                    }),
                    icon_color: tokens::on_accent(),
                    border: Border {
                        color: if is_checked {
                            tokens::accent()
                        } else {
                            tokens::rule()
                        },
                        width: 1.0,
                        radius: RADIUS_CONTROL.into(),
                    },
                    text_color: Some(tokens::text()),
                }
            }),
    )
    .height(Length::Fixed(if d == Density::Touch { 38.0 } else { 28.0 }))
    .align_y(Alignment::Center)
    .into()
}

fn radio_row<'a, V: Copy + Eq + 'a>(
    label: impl Into<String>,
    value: V,
    selected: Option<V>,
    on_select: impl FnOnce(V) -> Message + 'a,
) -> Element<'a, Message> {
    radio(label.into(), value, selected, on_select)
        .font(BARLOW)
        .text_size(TEXT_13)
        .size(15.0)
        .spacing(8.0)
        .style(|_, status| {
            let is_selected = matches!(
                status,
                radio::Status::Active { is_selected: true }
                    | radio::Status::Hovered { is_selected: true }
            );
            radio::Style {
                background: Background::Color(tokens::input_bg()),
                dot_color: tokens::accent(),
                border_width: 1.5,
                border_color: if is_selected {
                    tokens::accent()
                } else {
                    tokens::rule()
                },
                text_color: Some(tokens::text()),
            }
        })
        .into()
}

fn labelled_field<'a>(
    label: &'a str,
    value: &'a str,
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    d: Density,
) -> Element<'a, Message> {
    kit::field(
        FieldProps {
            label,
            value,
            placeholder,
            on_input: Some(Box::new(on_input)),
            on_submit: None,
            focused: false,
        },
        d,
    )
}

fn footer_pair<'a>(
    left: Element<'a, Message>,
    right: Element<'a, Message>,
    compact: bool,
) -> Element<'a, Message> {
    if compact {
        column![right, left]
            .spacing(SPACE_2)
            .width(Length::Fill)
            .into()
    } else {
        row![left, Space::new().width(Length::Fill), right]
            .spacing(SPACE_2)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into()
    }
}

// ---------------------------------------------------------------------------
// 1. Alert / Confirm / Prompt / Close-sketch
// ---------------------------------------------------------------------------

fn alert_dialog_layer<'a>(
    dlg: &'a Dialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Element<'a, Message> {
    let msg = |m: DialogMsg| Message::Dialog(m);
    let mut body = column![text(dlg.message.clone())
        .font(BARLOW)
        .size(TEXT_13)
        .color(tokens::text())]
    .spacing(SPACE_3)
    .width(Length::Fill);

    if let Some(val) = &dlg.input {
        body = body.push(
            text_input("", val)
                .id(crate::dialog::INPUT_ID)
                .on_input(move |s| msg(DialogMsg::Input(s)))
                .on_submit(msg(DialogMsg::Ok))
                .font(BARLOW)
                .size(TEXT_14)
                .padding([((d.field_h() - 18.0).max(6.0)) * 0.5, SPACE_3])
                .width(Length::Fill)
                .style(|_, status| {
                    let focused = matches!(status, text_input::Status::Focused { .. });
                    text_input::Style {
                        background: Background::Color(tokens::input_bg()),
                        border: Border {
                            color: if focused {
                                tokens::accent()
                            } else {
                                tokens::rule()
                            },
                            width: if focused { 2.0 } else { 1.0 },
                            radius: RADIUS_CONTROL.into(),
                        },
                        icon: tokens::muted(),
                        placeholder: tokens::muted(),
                        value: tokens::text(),
                        selection: tokens::accent_soft(),
                    }
                }),
        );
    }

    let save_close = dlg.on_ok == Action::SaveAndExit;
    let mut actions = row![Space::new().width(Length::Fill)].spacing(SPACE_2);
    if dlg.has_cancel {
        actions = actions.push(kit::text_button("Cancel", Some(msg(DialogMsg::Cancel)), d));
    }
    if save_close {
        actions = actions.push(kit::text_button(
            "Don't save",
            Some(msg(DialogMsg::Discard)),
            d,
        ));
    }
    actions = actions.push(kit::primary_button(
        if save_close { "Save" } else { "OK" },
        Some(msg(DialogMsg::Ok)),
        d,
    ));

    let title = if dlg.title == "SketchQuoder" {
        match &dlg.on_ok {
            Action::RenameDocument => "Rename sketch".to_string(),
            Action::RenamePage(_) => "Rename page".to_string(),
            _ => "Sketch Rust".to_string(),
        }
    } else {
        dlg.title.clone()
    };

    dialog_frame(
        DialogFrame {
            title,
            subtitle: None,
            preferred_w: 420.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(actions.into()),
            on_close: Some(msg(DialogMsg::Dismiss)),
        },
        win,
        size_class,
        d,
    )
}

// ---------------------------------------------------------------------------
// 2. Autosave recovery
// ---------------------------------------------------------------------------

fn autosave_layer<'a>(
    app: &'a App,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !app.autosave_prompt {
        return None;
    }
    let title = app
        .autosave
        .recovery
        .as_ref()
        .and_then(|(_, m)| m.get("fileName"))
        .and_then(|v| v.as_str())
        .unwrap_or("sketch");

    let body = column![
        text(format!("Recover autosaved sketch \"{title}\"?"))
            .font(BARLOW_MEDIUM)
            .size(TEXT_14)
            .color(tokens::text()),
        body_text("Recovery stays available until you recover and manually save it."),
    ]
    .spacing(SPACE_2);

    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Later", Some(Message::RecoverAutosave(false)), d),
        kit::primary_button("Recover", Some(Message::RecoverAutosave(true)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Recover autosave".into(),
            subtitle: None,
            preferred_w: 440.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(Message::RecoverAutosave(false)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 3. Preferences / Options (6 tabs)
// ---------------------------------------------------------------------------

fn opt_msg(m: OptMsg) -> Message {
    Message::Options(m)
}

fn opt_bool<'a>(
    label: &'static str,
    draft: &Options,
    key: BoolKey,
    d: Density,
) -> Element<'a, Message> {
    check_row(
        label,
        draft.get_bool(key),
        move |_| opt_msg(OptMsg::Toggle(key)),
        d,
    )
}

fn owned_field<'a>(
    label: &str,
    value: &str,
    placeholder: &str,
    on_input: impl Fn(String) -> Message + 'a,
    d: Density,
) -> Element<'a, Message> {
    let input = text_input(placeholder, value)
        .on_input(on_input)
        .font(BARLOW)
        .size(TEXT_13)
        .padding([((d.field_h() - 18.0).max(4.0)) * 0.5, SPACE_2 + 2.0])
        .width(Length::Fill)
        .style(|_, status| {
            let focused = matches!(status, text_input::Status::Focused { .. });
            text_input::Style {
                background: Background::Color(tokens::input_bg()),
                border: Border {
                    color: if focused {
                        tokens::accent()
                    } else {
                        tokens::rule()
                    },
                    width: if focused { 2.0 } else { 1.0 },
                    radius: RADIUS_CONTROL.into(),
                },
                icon: tokens::muted(),
                placeholder: tokens::muted(),
                value: tokens::text(),
                selection: tokens::accent_soft(),
            }
        });
    if label.is_empty() {
        return input.into();
    }
    column![label_text(label.to_owned()), input]
        .spacing(SPACE_1)
        .width(Length::Fill)
        .into()
}

fn opt_num<'a>(
    label: &'static str,
    dlg: &OptionsDialog,
    key: NumKey,
    d: Density,
) -> Element<'a, Message> {
    let val = dlg.num_text(key);
    owned_field(
        label,
        &val,
        "",
        move |s| opt_msg(OptMsg::NumTyped(key, s)),
        d,
    )
}

fn opt_str<'a>(
    label: &'static str,
    draft: &Options,
    key: StrKey,
    d: Density,
) -> Element<'a, Message> {
    let val = draft.get_str(key);
    owned_field(label, &val, "", move |s| opt_msg(OptMsg::Str(key, s)), d)
}

fn opt_choice_pick<'a>(
    label: &'static str,
    list: &'static [(&'static str, &'static str)],
    key: StrKey,
    current: &str,
) -> Element<'a, Message> {
    let opts: Vec<LabelValue> = list.iter().map(|&(v, l)| LabelValue(v, l)).collect();
    let selected = opts
        .iter()
        .find(|c| c.0 == current)
        .cloned()
        .or_else(|| opts.first().cloned());
    column![
        label_text(label),
        pick_list(opts, selected, move |c: LabelValue| opt_msg(OptMsg::Str(
            key,
            c.0.to_string()
        )))
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BARLOW)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill)
    .into()
}

fn opt_num_pick<'a>(
    label: &'static str,
    list: &[f64],
    key: NumKey,
    current: f64,
) -> Element<'a, Message> {
    let opts: Vec<NumChoice> = list.iter().copied().map(NumChoice).collect();
    let selected = opts
        .iter()
        .find(|c| (c.0 - current).abs() < 1e-9)
        .copied()
        .or_else(|| opts.first().copied());
    column![
        label_text(label),
        pick_list(opts, selected, move |c: NumChoice| opt_msg(
            OptMsg::NumPicked(key, c)
        ))
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BSC_MEDIUM)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill)
    .into()
}

fn opt_font_pick<'a>(label: &'static str, key: StrKey, current: &str) -> Element<'a, Message> {
    let opts: Vec<StringChoice> = FONTS
        .iter()
        .map(|f| StringChoice((*f).to_string(), (*f).to_string()))
        .collect();
    let selected = opts
        .iter()
        .find(|c| c.0.eq_ignore_ascii_case(current))
        .cloned()
        .or_else(|| opts.first().cloned());
    column![
        label_text(label),
        pick_list(opts, selected, move |c: StringChoice| opt_msg(OptMsg::Str(
            key, c.0
        )))
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BARLOW)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill)
    .into()
}

fn two_up<'a>(
    a: Element<'a, Message>,
    b: Element<'a, Message>,
    compact: bool,
) -> Element<'a, Message> {
    if compact {
        column![a, b].spacing(SPACE_2).width(Length::Fill).into()
    } else {
        row![
            container(a).width(Length::FillPortion(1)),
            container(b).width(Length::FillPortion(1)),
        ]
        .spacing(SPACE_3)
        .width(Length::Fill)
        .into()
    }
}

fn three_up<'a>(
    a: Element<'a, Message>,
    b: Element<'a, Message>,
    c: Element<'a, Message>,
    compact: bool,
) -> Element<'a, Message> {
    if compact {
        column![a, b, c].spacing(SPACE_2).width(Length::Fill).into()
    } else {
        row![
            container(a).width(Length::FillPortion(1)),
            container(b).width(Length::FillPortion(1)),
            container(c).width(Length::FillPortion(1)),
        ]
        .spacing(SPACE_2)
        .width(Length::Fill)
        .into()
    }
}

/// An Options tab's sections: one column on a phone, two side by side
/// otherwise (the first `split` sections on the left), so every tab fits a
/// 1382 x 816 window (a 3456 x 2160 screen at 250% scaling) without
/// scrolling.
fn opt_columns<'a>(
    compact: bool,
    split: usize,
    sections: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    if compact {
        return iced::widget::Column::with_children(sections)
            .spacing(SPACE_3)
            .into();
    }
    let mut left = column![].spacing(SPACE_3).width(Length::FillPortion(1));
    let mut right = column![].spacing(SPACE_3).width(Length::FillPortion(1));
    for (i, sec) in sections.into_iter().enumerate() {
        if i < split {
            left = left.push(sec);
        } else {
            right = right.push(sec);
        }
    }
    row![left, right]
        .spacing(SPACE_3)
        .align_y(Alignment::Start)
        .into()
}

/// Interface theme, drawing background and density. These used to be
/// submenus of the Menu's View group; they are preferences, so they live
/// here now. Unlike the rest of Options they apply as soon as they are
/// picked (Cancel does not undo them), which the hint line says.
fn appearance_section<'a>(d: Density) -> Element<'a, Message> {
    use crate::canvas_theme::{self, CanvasTheme};
    use crate::popup_menu::PopupMsg;
    use crate::theme::{self, InterfaceTheme};
    let ui = theme::interface_theme();
    let theme_seg = kit::segmented(
        [
            (InterfaceTheme::Light, "Light"),
            (InterfaceTheme::Blue, "Blue"),
            (InterfaceTheme::Dark, "Dark"),
        ]
        .into_iter()
        .map(|(t, label)| kit::Segment {
            label,
            selected: ui == t,
            focused: false,
            on_press: Some(Message::PopupMenu(PopupMsg::Interface(t))),
        })
        .collect(),
        d,
    );
    let canvas = canvas_theme::current();
    let canvas_seg = kit::segmented(
        [
            (CanvasTheme::Light, "Light"),
            (CanvasTheme::Blue, "Blue"),
            (CanvasTheme::Dark, "Dark"),
        ]
        .into_iter()
        .map(|(t, label)| kit::Segment {
            label,
            selected: canvas == t,
            focused: false,
            on_press: Some(Message::PopupMenu(PopupMsg::Canvas(t))),
        })
        .collect(),
        d,
    );
    let density_seg = kit::segmented(
        [(Density::Comfortable, "Mouse"), (Density::Touch, "Touch")]
            .into_iter()
            .map(|(den, label)| kit::Segment {
                label,
                selected: d == den,
                focused: false,
                on_press: Some(Message::Ui(UiMsg::SetDensity(den))),
            })
            .collect(),
        d,
    );
    let line = |label: &'static str, control: Element<'a, Message>| -> Element<'a, Message> {
        row![
            text(label)
                .font(BARLOW_MEDIUM)
                .size(TEXT_13)
                .color(tokens::text())
                .width(Length::Fixed(150.0)),
            control,
        ]
        .spacing(SPACE_2)
        .align_y(Alignment::Center)
        .into()
    };
    section_box(
        column![
            label_text("Appearance"),
            line("Interface theme", theme_seg),
            line("Drawing background", canvas_seg),
            line("Controls sized for", density_seg),
            text("These apply as soon as you pick them.")
                .font(BARLOW)
                .size(TEXT_12)
                .color(tokens::muted()),
        ]
        .spacing(SPACE_2),
    )
}

fn options_dialog_layer<'a>(
    dlg: &'a OptionsDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let draft = &dlg.draft;

    let tab_body: Element<'a, Message> = match dlg.tab {
        OptTab::General => opt_columns(
            compact,
            1,
            vec![
                section_box(
                    column![
                        label_text("Input"),
                        opt_choice_pick(
                            "Input mode",
                            INPUT_MODES,
                            StrKey::InputMode,
                            &draft.input_mode
                        ),
                        opt_num_pick(
                            "Ink recognition pause (s)",
                            INK_RECOGNITIONS,
                            NumKey::InkRecognition,
                            draft.ink_recognition,
                        ),
                        two_up(
                            opt_bool(
                                "Confirm ink before placing",
                                draft,
                                BoolKey::InkConfirmation,
                                d
                            ),
                            opt_bool("Prompt on deletion", draft, BoolKey::PromptOnDeletion, d),
                            compact,
                        ),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Units & Dimensions"),
                        row![
                            radio_row("Imperial (ft / in)", false, Some(draft.units.metric), |m| {
                                opt_msg(OptMsg::UnitMetric(m))
                            }),
                            radio_row("Metric (m / cm)", true, Some(draft.units.metric), |m| {
                                opt_msg(OptMsg::UnitMetric(m))
                            }),
                        ]
                        .spacing(SPACE_4),
                        two_up(
                            opt_num_pick(
                                "Decimal precision",
                                PRECISIONS,
                                NumKey::DecimalPrecision,
                                draft.units.decimal_precision,
                            ),
                            opt_str("Area suffix", draft, StrKey::AreaLabelSuffix, d),
                            compact,
                        ),
                        check_row(
                            "Strip trailing zeroes",
                            draft.units.strip_trailing_zeroes,
                            |_| opt_msg(OptMsg::ToggleStripZeroes),
                            d,
                        ),
                        row![
                            label_text("Step unit:"),
                            radio_row("Whole", "whole", Some(draft.step_unit.as_str()), |v| {
                                opt_msg(OptMsg::Str(StrKey::StepUnit, v.into()))
                            }),
                            radio_row("Half", "half", Some(draft.step_unit.as_str()), |v| opt_msg(
                                OptMsg::Str(StrKey::StepUnit, v.into())
                            )),
                            radio_row("Tenths", "tenths", Some(draft.step_unit.as_str()), |v| {
                                opt_msg(OptMsg::Str(StrKey::StepUnit, v.into()))
                            }),
                        ]
                        .spacing(SPACE_3)
                        .wrap(),
                        two_up(
                            opt_num("Dimension scale factor", dlg, NumKey::DimensionOption, d),
                            opt_num("Ortho snap angle (deg)", dlg, NumKey::OrthoAngle, d),
                            compact,
                        ),
                        two_up(
                            opt_bool("Auto-place dimensions", draft, BoolKey::AutoPlacement, d),
                            opt_bool(
                                "Dimension matches wall colour",
                                draft,
                                BoolKey::SameColorAsLine,
                                d
                            ),
                            compact,
                        ),
                    ]
                    .spacing(SPACE_2),
                ),
            ],
        ),

        OptTab::Display => opt_columns(
            compact,
            2,
            vec![
                appearance_section(d),
                section_box(
                    column![
                        label_text("Guides & Indicators"),
                        two_up(
                            opt_bool("Alignment lines", draft, BoolKey::AlignmentLines, d),
                            opt_bool("Alignment points", draft, BoolKey::AlignmentPoints, d),
                            compact,
                        ),
                        two_up(
                            opt_bool(
                                "Close-corner indicators",
                                draft,
                                BoolKey::TestCloseIndicators,
                                d
                            ),
                            opt_bool("Show background grid", draft, BoolKey::GridEnabled, d),
                            compact,
                        ),
                        two_up(
                            opt_str("Alignment line colour", draft, StrKey::AlignmentColor, d),
                            opt_choice_pick(
                                "Alignment line style",
                                ALIGNMENT_STYLES,
                                StrKey::AlignmentStyle,
                                &draft.alignment_style
                            ),
                            compact,
                        ),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Filtering & Selection"),
                        two_up(
                            opt_bool(
                                "Minimum dimension filter",
                                draft,
                                BoolKey::MinimumDimensionEnabled,
                                d
                            ),
                            opt_num("Minimum dimension length", dlg, NumKey::MinimumDimension, d),
                            compact,
                        ),
                        two_up(
                            opt_bool("Auto-shade closed areas", draft, BoolKey::AutoShade, d),
                            opt_num("Auto-shade opacity (%)", dlg, NumKey::AutoShadePercent, d),
                            compact,
                        ),
                        opt_bool("Highlight selection", draft, BoolKey::SelectionHighlight, d),
                    ]
                    .spacing(SPACE_2),
                ),
            ],
        ),

        OptTab::Hardware => column![section_box(
            column![
                label_text("Laser / Leica DISTO"),
                two_up(
                    column![
                        label_text("Discovery"),
                        row![
                            radio_row(
                                "Automatic",
                                "automatic",
                                Some(draft.disto_discovery.as_str()),
                                |v| opt_msg(OptMsg::Str(StrKey::DistoDiscovery, v.into()))
                            ),
                            radio_row(
                                "Manual",
                                "manual",
                                Some(draft.disto_discovery.as_str()),
                                |v| opt_msg(OptMsg::Str(StrKey::DistoDiscovery, v.into()))
                            ),
                        ]
                        .spacing(SPACE_3),
                    ]
                    .spacing(SPACE_1)
                    .into(),
                    opt_str("COM port", draft, StrKey::DistoComPort, d),
                    compact,
                ),
                two_up(
                    opt_choice_pick(
                        "Measurement rounding",
                        ROUNDING_MODES,
                        StrKey::DistoRounding,
                        &draft.disto_rounding
                    ),
                    opt_choice_pick(
                        "Drawing mode",
                        DRAWING_MODES,
                        StrKey::DistoDrawingMode,
                        &draft.disto_drawing_mode
                    ),
                    compact,
                ),
            ]
            .spacing(SPACE_2),
        )]
        .spacing(SPACE_3)
        .into(),

        OptTab::System => opt_columns(
            compact,
            1,
            vec![
                section_box(
                    column![
                        label_text("Autosave & Recovery"),
                        two_up(
                            opt_bool(
                                "Enable automatic recovery snapshots",
                                draft,
                                BoolKey::AutoSaveEnabled,
                                d
                            ),
                            opt_num("Autosave interval (min)", dlg, NumKey::AutoSaveInterval, d),
                            compact,
                        ),
                        two_up(
                            opt_bool("Auto-heal loaded files", draft, BoolKey::AutoHealFiles, d),
                            opt_bool(
                                "Hide video tutorial links",
                                draft,
                                BoolKey::DisableVideoTutorial,
                                d
                            ),
                            compact,
                        ),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Export Image Defaults"),
                        two_up(
                            {
                                let list = crate::options_dialog::size_list();
                                let opts: Vec<StringChoice> = list
                                    .iter()
                                    .map(|&(v, l)| StringChoice(v.to_string(), l.to_string()))
                                    .collect();
                                let sel = opts
                                    .iter()
                                    .find(|c| c.0 == draft.image_size)
                                    .cloned()
                                    .or_else(|| opts.first().cloned());
                                column![
                                    label_text("Image dimensions"),
                                    pick_list(opts, sel, |c: StringChoice| opt_msg(OptMsg::Str(
                                        StrKey::ImageSize,
                                        c.0
                                    )))
                                    .width(Length::Fill)
                                    .text_size(TEXT_13)
                                    .font(BARLOW)
                                    .padding([6, 10])
                                    .style(pick_style)
                                    .menu_style(pick_menu_style),
                                ]
                                .spacing(SPACE_1)
                                .into()
                            },
                            column![
                                label_text("Colour depth"),
                                row![
                                    radio_row("8-bit", 8i32, Some(draft.image_depth as i32), |v| {
                                        opt_msg(OptMsg::NumPicked(
                                            NumKey::ImageDepth,
                                            NumChoice(f64::from(v)),
                                        ))
                                    }),
                                    radio_row(
                                        "24-bit",
                                        24i32,
                                        Some(draft.image_depth as i32),
                                        |v| opt_msg(OptMsg::NumPicked(
                                            NumKey::ImageDepth,
                                            NumChoice(f64::from(v))
                                        ))
                                    ),
                                ]
                                .spacing(SPACE_3),
                            ]
                            .spacing(SPACE_1)
                            .into(),
                            compact,
                        ),
                        opt_bool(
                            "Append page number to image exports",
                            draft,
                            BoolKey::AppendPageNumber,
                            d
                        ),
                        row![
                            label_text("Formats:"),
                            check_row(
                                "PNG",
                                draft.format_enabled(FormatKey::Png),
                                |_| opt_msg(OptMsg::ToggleFormat(FormatKey::Png)),
                                d
                            ),
                            check_row(
                                "JPG",
                                draft.format_enabled(FormatKey::Jpg),
                                |_| opt_msg(OptMsg::ToggleFormat(FormatKey::Jpg)),
                                d
                            ),
                            check_row(
                                "BMP",
                                draft.format_enabled(FormatKey::Bmp),
                                |_| opt_msg(OptMsg::ToggleFormat(FormatKey::Bmp)),
                                d
                            ),
                        ]
                        .spacing(SPACE_3)
                        .align_y(Alignment::Center)
                        .wrap(),
                    ]
                    .spacing(SPACE_2),
                ),
            ],
        ),

        OptTab::Fonts => opt_columns(
            compact,
            2,
            vec![
                section_box(
                    column![
                        label_text("Dimension Labels"),
                        two_up(
                            opt_font_pick(
                                "Font family",
                                StrKey::DimensionFont,
                                &draft.dimension_font
                            ),
                            opt_num_pick(
                                "Font size",
                                FONT_SIZES,
                                NumKey::DimensionFontSize,
                                draft.dimension_font_size
                            ),
                            compact,
                        ),
                        row![
                            opt_bool("Bold", draft, BoolKey::DimensionBold, d),
                            opt_bool("Italic", draft, BoolKey::DimensionItalic, d),
                            opt_bool("Underline", draft, BoolKey::DimensionUnderline, d),
                        ]
                        .spacing(SPACE_3)
                        .wrap(),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Room & Note Text"),
                        three_up(
                            opt_font_pick("Font family", StrKey::TextFont, &draft.text_font),
                            opt_num_pick(
                                "Font size",
                                FONT_SIZES,
                                NumKey::TextFontSize,
                                draft.text_font_size
                            ),
                            opt_str("Text colour", draft, StrKey::TextColor, d),
                            compact,
                        ),
                        row![
                            opt_bool("Bold", draft, BoolKey::TextBold, d),
                            opt_bool("Italic", draft, BoolKey::TextItalic, d),
                            opt_bool("Underline", draft, BoolKey::TextUnderline, d),
                        ]
                        .spacing(SPACE_3)
                        .wrap(),
                        two_up(
                            opt_str("Label backdrop colour", draft, StrKey::TextBackground, d),
                            opt_num(
                                "Backdrop opacity (%)",
                                dlg,
                                NumKey::TextBackgroundOpacity,
                                d
                            ),
                            compact,
                        ),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Auto-Post Area Labels"),
                        two_up(
                            opt_font_pick(
                                "Font family",
                                StrKey::AutoPostFont,
                                &draft.auto_post_font
                            ),
                            opt_num_pick(
                                "Font size",
                                FONT_SIZES,
                                NumKey::AutoPostFontSize,
                                draft.auto_post_font_size
                            ),
                            compact,
                        ),
                        row![
                            opt_bool("Bold", draft, BoolKey::AutoPostBold, d),
                            opt_bool("Italic", draft, BoolKey::AutoPostItalic, d),
                            opt_bool("Underline", draft, BoolKey::AutoPostUnderline, d),
                            opt_bool("Circle area totals", draft, BoolKey::CircleAreaCalcs, d),
                        ]
                        .spacing(SPACE_3)
                        .wrap(),
                    ]
                    .spacing(SPACE_2),
                ),
            ],
        ),

        OptTab::Drawing => opt_columns(
            compact,
            2,
            vec![
                section_box(
                    column![
                        label_text("Walls"),
                        three_up(
                            opt_str("Exterior colour", draft, StrKey::ExteriorColor, d),
                            opt_num("Exterior width (px)", dlg, NumKey::ExteriorWidth, d),
                            opt_num("Exterior thickness (in)", dlg, NumKey::ExteriorThickness, d),
                            compact,
                        ),
                        three_up(
                            opt_str("Interior colour", draft, StrKey::InteriorColor, d),
                            opt_num("Interior width (px)", dlg, NumKey::InteriorWidth, d),
                            opt_num("Interior thickness (in)", dlg, NumKey::InteriorThickness, d),
                            compact,
                        ),
                        opt_bool("Fill wall solids", draft, BoolKey::WallSolids, d),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Witness & Labels"),
                        three_up(
                            opt_str("Witness base colour", draft, StrKey::WitnessBaseColor, d),
                            opt_str(
                                "Witness interior colour",
                                draft,
                                StrKey::WitnessInteriorColor,
                                d
                            ),
                            opt_str("Witness curve colour", draft, StrKey::WitnessCurveColor, d),
                            compact,
                        ),
                        two_up(
                            opt_num("Area label opacity (%)", dlg, NumKey::LabelOpacity, d),
                            opt_num_pick(
                                "Crosshair pointer size (px)",
                                POINTERS,
                                NumKey::PointerSize,
                                draft.pointer_size
                            ),
                            compact,
                        ),
                    ]
                    .spacing(SPACE_2),
                ),
                section_box(
                    column![
                        label_text("Behaviour"),
                        two_up(
                            opt_bool(
                                "Define area after closing",
                                draft,
                                BoolKey::DisplayOnClose,
                                d
                            ),
                            opt_bool("Define area before drawing", draft, BoolKey::DefineFirst, d),
                            compact,
                        ),
                        two_up(
                            opt_bool(
                                "Auto-subtract overlapping areas",
                                draft,
                                BoolKey::AutoSubtractOverlapped,
                                d
                            ),
                            opt_bool(
                                "Quick shape auto-define",
                                draft,
                                BoolKey::QuickShapeDefine,
                                d
                            ),
                            compact,
                        ),
                        two_up(
                            opt_bool("Allow redefine closed areas", draft, BoolKey::Redefine, d),
                            opt_bool(
                                "Split walls at T-junctions",
                                draft,
                                BoolKey::SplitWallsAtJunctions,
                                d
                            ),
                            compact,
                        ),
                        two_up(
                            opt_bool(
                                "Allow pan/zoom during delete",
                                draft,
                                BoolKey::PanZoomDelete,
                                d
                            ),
                            opt_bool(
                                "Embed reference photo in .skx",
                                draft,
                                BoolKey::SaveImageWithSketch,
                                d
                            ),
                            compact,
                        ),
                        opt_num("Embedded image quality (%)", dlg, NumKey::ImageQuality, d),
                    ]
                    .spacing(SPACE_2),
                ),
            ],
        ),
    };

    let nav_items: Element<'a, Message> = if compact {
        let mut r = row![].spacing(SPACE_1);
        for &tab in OptTab::ALL {
            r = r.push(kit::chip(
                tab.label(),
                dlg.tab == tab,
                Some(opt_msg(OptMsg::Tab(tab))),
                d,
            ));
        }
        scrollable(container(r).padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 12.0,
            left: 0.0,
        }))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill)
        .into()
    } else {
        let mut col = column![].spacing(SPACE_1).width(Length::Fixed(158.0));
        for &tab in OptTab::ALL {
            col = col.push(kit::list_row(
                kit::ListRowProps {
                    swatch: None,
                    title: tab.label(),
                    value: None,
                    selected: dlg.tab == tab,
                    flagged: false,
                    focused: false,
                    on_press: Some(opt_msg(OptMsg::Tab(tab))),
                },
                d,
            ));
        }
        col.into()
    };

    let body: Element<'a, Message> = if compact {
        column![
            nav_items,
            kit::divider(),
            scrollable(tab_body)
                .height(Length::Fill)
                .width(Length::Fill),
        ]
        .spacing(SPACE_2)
        .height(Length::Fill)
        .into()
    } else {
        row![
            nav_items,
            container(Space::new().width(1.0))
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(tokens::rule())),
                    ..container::Style::default()
                }),
            scrollable(container(tab_body).padding(Padding {
                right: SPACE_2,
                ..Padding::ZERO
            }))
            .height(Length::Fill)
            .width(Length::Fill),
        ]
        .spacing(SPACE_3)
        .height(Length::Fill)
        .into()
    };

    let footer_left = row![
        kit::text_button(
            "Restore Defaults",
            Some(opt_msg(OptMsg::RestoreDefaults)),
            d
        ),
        kit::text_button("Save as Default", Some(opt_msg(OptMsg::SaveDefaults)), d),
    ]
    .spacing(SPACE_2)
    .wrap();
    let footer_right = row![
        kit::text_button("Cancel", Some(opt_msg(OptMsg::Cancel)), d),
        kit::primary_button("OK", Some(opt_msg(OptMsg::Ok)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Preferences".into(),
            subtitle: Some(
                "General, display, laser hardware, system, fonts, and wall drawing defaults".into(),
            ),
            preferred_w: 1120.0,
            preferred_h: Some(720.0),
            body,
            footer: Some(footer_pair(
                footer_left.into(),
                footer_right.into(),
                compact,
            )),
            on_close: Some(opt_msg(OptMsg::Cancel)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 4. Subject / Property Info (3 tabs)
// ---------------------------------------------------------------------------

fn subj_msg(m: SubjectMsg) -> Message {
    Message::SubjectInfo(m)
}

fn subject_info_layer<'a>(
    dlg: &'a SubjectDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;

    let tabs = row![
        kit::chip(
            "Parcel Info",
            dlg.tab == SubjectTab::Parcel,
            Some(subj_msg(SubjectMsg::Tab(SubjectTab::Parcel))),
            d
        ),
        kit::chip(
            "General Comments",
            dlg.tab == SubjectTab::Comments,
            Some(subj_msg(SubjectMsg::Tab(SubjectTab::Comments))),
            d
        ),
        kit::chip(
            "Comment Tables",
            dlg.tab == SubjectTab::Tables,
            Some(subj_msg(SubjectMsg::Tab(SubjectTab::Tables))),
            d
        ),
    ]
    .spacing(SPACE_2)
    .wrap();

    let autofill_bar = section_box(
        row![
            column![
                text("Appraise-It Pro")
                    .font(BARLOW_SEMIBOLD)
                    .size(TEXT_13)
                    .color(tokens::text()),
                text("Pull parcel and subject details from an open Appraise-It Pro report.")
                    .font(BARLOW)
                    .size(TEXT_12)
                    .color(tokens::muted()),
            ]
            .spacing(2)
            .width(Length::Fill),
            kit::text_button(
                "Auto-Fill from Report",
                Some(subj_msg(SubjectMsg::AutoFill)),
                d
            ),
        ]
        .spacing(SPACE_3)
        .align_y(Alignment::Center),
    );

    let tab_content: Element<'a, Message> = match dlg.tab {
        SubjectTab::Parcel => {
            let mut rows_col = column![].spacing(SPACE_2).width(Length::Fill);
            let placeholder_for = |idx: usize| {
                if idx == IDX_INSPECTION {
                    "YYYY-MM-DD"
                } else if idx == IDX_GPS {
                    GPS_PLACEHOLDER
                } else {
                    ""
                }
            };
            // Wider screens: three columns in five rows, so the whole form
            // fits without scrolling. Each entry is (field, column span).
            const WIDE_ROWS: [&[(usize, u16)]; 5] = [
                &[(0, 1), (1, 1), (IDX_INSPECTION, 1)],
                &[(2, 2), (7, 1)],
                &[(3, 1), (4, 1), (5, 1)],
                &[(6, 1), (8, 1), (10, 1)],
                &[(9, 2), (IDX_GPS, 1)],
            ];
            if !compact {
                for cells in WIDE_ROWS {
                    let mut r = row![].spacing(SPACE_3).width(Length::Fill);
                    for &(idx, span) in cells {
                        r = r.push(
                            container(owned_field(
                                SUBJECT_FIELDS[idx],
                                &dlg.fields[idx],
                                placeholder_for(idx),
                                move |s| subj_msg(SubjectMsg::Field(idx, s)),
                                d,
                            ))
                            .width(Length::FillPortion(span)),
                        );
                    }
                    rows_col = rows_col.push(r);
                }
            }
            for indices in GRID_ROWS.iter().filter(|_| compact) {
                if compact || indices.len() == 1 {
                    for &idx in indices.iter() {
                        let placeholder = if idx == IDX_INSPECTION {
                            "YYYY-MM-DD"
                        } else if idx == IDX_GPS {
                            GPS_PLACEHOLDER
                        } else {
                            ""
                        };
                        rows_col = rows_col.push(owned_field(
                            SUBJECT_FIELDS[idx],
                            &dlg.fields[idx],
                            placeholder,
                            move |s| subj_msg(SubjectMsg::Field(idx, s)),
                            d,
                        ));
                    }
                } else {
                    let i0 = indices[0];
                    let i1 = indices[1];
                    let ph1 = if i1 == IDX_INSPECTION {
                        "YYYY-MM-DD"
                    } else {
                        ""
                    };
                    rows_col = rows_col.push(two_up(
                        owned_field(
                            SUBJECT_FIELDS[i0],
                            &dlg.fields[i0],
                            "",
                            move |s| subj_msg(SubjectMsg::Field(i0, s)),
                            d,
                        ),
                        owned_field(
                            SUBJECT_FIELDS[i1],
                            &dlg.fields[i1],
                            ph1,
                            move |s| subj_msg(SubjectMsg::Field(i1, s)),
                            d,
                        ),
                        false,
                    ));
                }
            }

            let meta = section_box(
                row![
                    label_text("Created By: Sketch Rust"),
                    label_text("Modified By: Sketch Rust"),
                    label_text("Format Version: Sketch-Web"),
                ]
                .spacing(SPACE_4)
                .wrap(),
            );

            let extras_toggle = kit::text_button(
                if dlg.extras_open {
                    "Hide additional fields"
                } else {
                    "Expand additional fields"
                },
                Some(subj_msg(SubjectMsg::ToggleExtras)),
                d,
            );

            let mut col = column![autofill_bar, rows_col, meta, extras_toggle]
                .spacing(SPACE_3)
                .width(Length::Fill);

            if dlg.extras_open {
                let mut ex_col = column![two_up(
                    label_text("Field name"),
                    label_text("Value"),
                    compact,
                )]
                .spacing(SPACE_2);
                for (i, (k, v)) in dlg.extra.iter().enumerate() {
                    ex_col = ex_col.push(two_up(
                        labelled_field(
                            "",
                            k,
                            "Name",
                            move |s| subj_msg(SubjectMsg::ExtraName(i, s)),
                            d,
                        ),
                        labelled_field(
                            "",
                            v,
                            "Value",
                            move |s| subj_msg(SubjectMsg::ExtraValue(i, s)),
                            d,
                        ),
                        compact,
                    ));
                }
                col = col.push(section_box(ex_col));
            }
            scrollable(container(col).padding(Padding {
                right: SPACE_2,
                ..Padding::ZERO
            }))
            .height(Length::Fill)
            .into()
        }

        SubjectTab::Comments => column![
            label_text("Comments"),
            text_editor(&dlg.comments)
                .on_action(|a| subj_msg(SubjectMsg::Comments(a)))
                .font(BARLOW)
                .size(TEXT_13)
                .padding(SPACE_2)
                .height(Length::Fill)
                .style(editor_style),
            row![
                Space::new().width(Length::Fill),
                kit::text_button("Save Default", Some(subj_msg(SubjectMsg::SaveDefault)), d),
                kit::text_button("Apply Default", Some(subj_msg(SubjectMsg::ApplyDefault)), d),
            ]
            .spacing(SPACE_2),
        ]
        .spacing(SPACE_2)
        .height(Length::Fill)
        .into(),

        SubjectTab::Tables => {
            let mut tables = Vec::with_capacity(3);
            for i in 0..3 {
                let title = ["Comment Table 1", "Comment Table 2", "Comment Table 3"][i];
                tables.push(
                    column![
                        label_text(title),
                        text_editor(&dlg.tables[i])
                            .on_action(move |a| subj_msg(SubjectMsg::Table(i, a)))
                            .font(BARLOW)
                            .size(TEXT_13)
                            .padding(SPACE_2)
                            .height(Length::Fixed(180.0))
                            .style(editor_style),
                    ]
                    .spacing(SPACE_1)
                    .width(Length::Fill),
                );
            }
            let grid: Element<'a, Message> = if compact {
                let mut col = column![].spacing(SPACE_3);
                for t in tables {
                    col = col.push(t);
                }
                col.into()
            } else {
                let mut r = row![].spacing(SPACE_2);
                for t in tables {
                    r = r.push(container(t).width(Length::FillPortion(1)));
                }
                r.into()
            };
            scrollable(grid).height(Length::Fill).into()
        }
    };

    let body = column![tabs, kit::divider(), tab_content]
        .spacing(SPACE_3)
        .height(Length::Fill);

    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Cancel", Some(subj_msg(SubjectMsg::Close)), d),
        kit::primary_button("Apply", Some(subj_msg(SubjectMsg::Ok)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Property Details".into(),
            subtitle: Some("Parcel metadata, comments, and Appraise-It Pro fields".into()),
            preferred_w: 900.0,
            preferred_h: Some(760.0),
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(subj_msg(SubjectMsg::Close)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 5. Page Layout
// ---------------------------------------------------------------------------

fn page_layout_layer<'a>(
    dlg: &'a PageLayoutDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let lay = |m: LayoutMsg| Message::PageLayout(m);

    let papers: Vec<PaperChoice> = PAPERS.iter().copied().map(PaperChoice).collect();
    let forms: Vec<FormChoice> = Form::ALL
        .iter()
        .copied()
        .map(|f| FormChoice(f, dlg.draft.paper_size))
        .collect();

    let paper_pick = column![
        label_text("Paper size"),
        pick_list(
            papers,
            Some(PaperChoice(dlg.draft.paper_size)),
            move |p: PaperChoice| { lay(LayoutMsg::SetPaper(p.0)) }
        )
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BARLOW)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1);

    let form_pick = column![
        label_text("Addendum form"),
        pick_list(
            forms,
            Some(FormChoice(dlg.draft.form, dlg.draft.paper_size)),
            move |f: FormChoice| lay(LayoutMsg::SetForm(f.0))
        )
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BARLOW)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1);

    let mut opts_col = column![label_text("Options")].spacing(SPACE_1);
    for (key, label) in LAYOUT_OPTIONS {
        opts_col = opts_col.push(check_row(
            label,
            dlg.draft.get_bool(key),
            move |_| lay(LayoutMsg::Toggle(key)),
            d,
        ));
    }

    let mut caps_col = column![label_text("Captions")].spacing(SPACE_2);
    for (label, key) in CAPTIONS {
        caps_col = caps_col.push(labelled_field(
            label,
            dlg.caption_text(key),
            "",
            move |s| lay(LayoutMsg::Caption(key, s)),
            d,
        ));
    }

    // Two columns on wider screens (paper and options, then captions) so
    // the dialog fits without scrolling; one column on a phone.
    let content: Element<'a, Message> = if compact {
        column![
            two_up(paper_pick.into(), form_pick.into(), compact),
            section_box(opts_col),
            section_box(caps_col),
        ]
        .spacing(SPACE_3)
        .into()
    } else {
        row![
            column![paper_pick, form_pick, section_box(opts_col)]
                .spacing(SPACE_3)
                .width(Length::FillPortion(1)),
            container(section_box(caps_col)).width(Length::FillPortion(1)),
        ]
        .spacing(SPACE_4)
        .into()
    };
    let body = scrollable(content).height(Length::Fill);

    let footer_left =
        kit::text_button("Restore Defaults", Some(lay(LayoutMsg::RestoreDefaults)), d);
    let footer_right = row![
        kit::text_button("Cancel", Some(lay(LayoutMsg::Cancel)), d),
        kit::primary_button("Apply", Some(lay(LayoutMsg::Apply)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Page Layout".into(),
            subtitle: Some("Paper size, print addendum form, and headings".into()),
            preferred_w: 900.0,
            preferred_h: Some(640.0),
            body: body.into(),
            footer: Some(footer_pair(footer_left, footer_right.into(), compact)),
            on_close: Some(lay(LayoutMsg::Cancel)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 6. Print Preview
// ---------------------------------------------------------------------------

fn print_preview_layer<'a>(
    preview: &'a PrintPreview,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !preview.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    // Each page is drawn to the visible height of the dialog body, so one
    // whole page is always in view; more pages stack below as a list.
    let dialog_h = MAX_WIDE_H.min(win.height - 64.0);
    let page_h = (dialog_h - 210.0).max(240.0);

    let mut pages = column![].spacing(SPACE_3).align_x(Alignment::Center);
    if preview.rasters.is_empty() {
        pages = pages.push(
            container(body_text("No pages rendered yet."))
                .padding(SPACE_4)
                .width(Length::Fill)
                .center_x(Length::Fill),
        );
    } else {
        for (i, handle) in preview.rasters.iter().enumerate() {
            pages = pages.push(
                container(
                    column![
                        label_text(format!("Page {}", i + 1)),
                        image(handle.clone())
                            .height(Length::Fixed(page_h))
                            .content_fit(iced::ContentFit::Contain),
                    ]
                    .spacing(SPACE_1),
                )
                .padding(SPACE_2)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::WHITE)),
                    border: Border {
                        color: tokens::rule(),
                        width: 1.0,
                        radius: RADIUS_CONTROL.into(),
                    },
                    ..container::Style::default()
                }),
            );
        }
    }

    let body = scrollable(
        container(pages)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(SPACE_2),
    )
    // No "-list" suffix: the fit test checks that one page fits.
    .id("print-pages")
    .height(Length::Fill)
    .width(Length::Fill);

    let footer_left: Element<'a, Message> = text(preview.summary())
        .font(BSC_MEDIUM)
        .size(TEXT_12)
        .color(tokens::muted())
        .into();
    let footer_right = row![
        kit::text_button("Close", Some(Message::PrintPreview(PreviewMsg::Close)), d),
        kit::primary_button(
            "Print / Save PDF",
            Some(Message::PrintPreview(PreviewMsg::Print)),
            d,
        ),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Print Preview".into(),
            subtitle: preview.paper.map(|p| format!("Paper: {}", paper_label(p))),
            preferred_w: 840.0,
            preferred_h: Some(dialog_h),
            body: body.into(),
            footer: Some(footer_pair(footer_left, footer_right.into(), compact)),
            on_close: Some(Message::PrintPreview(PreviewMsg::Close)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 7. Area Breakdown
// ---------------------------------------------------------------------------

fn breakdown_layer<'a>(
    app: &'a App,
    dlg: &'a BreakdownDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let brk = |m: BreakdownMsg| Message::Breakdown(m);

    let report_text = text(app.breakdown_report())
        .font(Font::MONOSPACE)
        .size(TEXT_12)
        .color(tokens::text());

    let report_box = container(
        scrollable(report_text)
            .id("breakdown-list")
            .height(Length::Fill)
            .width(Length::Fill),
    )
    .padding(SPACE_3)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::field())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_TILE.into(),
        },
        ..container::Style::default()
    });

    let preview_box = container(
        column![
            label_text("Sketch Decomposition"),
            container(breakdown_dialog::preview(&app.drawing.polygons))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(tokens::canvas())),
                    border: Border {
                        color: tokens::rule(),
                        width: 1.0,
                        radius: RADIUS_TILE.into(),
                    },
                    ..container::Style::default()
                }),
        ]
        .spacing(SPACE_1),
    )
    .width(if compact {
        Length::Fill
    } else {
        Length::Fixed(340.0)
    })
    .height(if compact {
        Length::Fixed(220.0)
    } else {
        Length::Fill
    });

    let body: Element<'a, Message> = if compact {
        column![preview_box, report_box]
            .spacing(SPACE_3)
            .height(Length::Fill)
            .into()
    } else {
        row![report_box, preview_box]
            .spacing(SPACE_3)
            .height(Length::Fill)
            .into()
    };

    let footer_left = row![
        kit::text_button("Copy for report", Some(brk(BreakdownMsg::Copy)), d),
        kit::text_button("Download JSON", Some(brk(BreakdownMsg::DownloadJson)), d),
        kit::text_button("Download XML", Some(brk(BreakdownMsg::DownloadXml)), d),
    ]
    .spacing(SPACE_2)
    .wrap();
    let footer_right = kit::primary_button("Close", Some(brk(BreakdownMsg::Close)), d);

    Some(dialog_frame(
        DialogFrame {
            title: "Breakdown Details".into(),
            subtitle: Some("Rectangle & bow decomposition report".into()),
            preferred_w: 980.0,
            preferred_h: Some(660.0),
            body,
            footer: Some(footer_pair(footer_left.into(), footer_right, compact)),
            on_close: Some(brk(BreakdownMsg::Close)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 9. Area Attributes
// ---------------------------------------------------------------------------

fn attr_dialog_layer<'a>(
    dlg: &'a AttrDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let am = |m: AttrMsg| Message::AreaAttributes(m);
    let mut rows_col = column![].spacing(SPACE_2);
    for (i, &(_, label)) in ATTRS.iter().enumerate() {
        rows_col = rows_col.push(kit::field(
            FieldProps {
                label,
                value: &dlg.values[i],
                placeholder: "",
                on_input: dlg.editable.then(|| {
                    let f: Box<dyn Fn(String) -> Message> =
                        Box::new(move |s| am(AttrMsg::Value(i, s)));
                    f
                }),
                on_submit: None,
                focused: false,
            },
            d,
        ));
    }

    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Cancel", Some(am(AttrMsg::Cancel)), d),
        kit::primary_button("Apply", dlg.editable.then_some(am(AttrMsg::Ok)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Area Attributes".into(),
            subtitle: if !dlg.editable {
                Some("Select a closed area to edit its attributes.".into())
            } else {
                None
            },
            preferred_w: 460.0,
            preferred_h: None,
            body: scrollable(rows_col).into(),
            footer: Some(footer.into()),
            on_close: Some(am(AttrMsg::Cancel)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 10. Area Properties Dialog
// ---------------------------------------------------------------------------

fn area_dialog_layer<'a>(
    dlg: &'a AreaDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let ad = |m: AreaDialogMsg| Message::AreaDialog(m);

    let code_opts: Vec<StringChoice> = dlg
        .codes
        .iter()
        .map(|c| StringChoice(c.clone(), c.clone()))
        .collect();
    let cur_code = StringChoice(dlg.props.code.clone(), dlg.props.code.clone());
    let code_pick = column![
        label_text("Area type code"),
        pick_list(code_opts, Some(cur_code), move |c: StringChoice| {
            ad(AreaDialogMsg::Code(c.0))
        })
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BARLOW)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1);

    let measure_opts: Vec<StringChoice> = area_dialog::MEASURE
        .iter()
        .map(|&s| StringChoice(s.to_string(), s.to_string()))
        .collect();
    let cur_measure_lbl =
        area_dialog::measure_label(dlg.props.measure_to.as_deref().unwrap_or("center"));
    let cur_measure = StringChoice(cur_measure_lbl.to_string(), cur_measure_lbl.to_string());
    let measure_pick = column![
        label_text("Measure to"),
        pick_list(measure_opts, Some(cur_measure), move |c: StringChoice| {
            ad(AreaDialogMsg::Measure(c.0))
        })
        .width(Length::Fill)
        .text_size(TEXT_13)
        .font(BARLOW)
        .padding([6, 10])
        .style(pick_style)
        .menu_style(pick_menu_style),
    ]
    .spacing(SPACE_1);

    let post = dlg.props.auto_post.clone().unwrap_or_default();
    let auto_sub = if dlg.props.auto_subtract {
        "Automatic"
    } else {
        "Manual"
    };

    let body = scrollable(
        column![
            two_up(
                code_pick.into(),
                labelled_field(
                    "Name",
                    &dlg.props.name,
                    "",
                    move |s| ad(AreaDialogMsg::Name(s)),
                    d
                ),
                compact,
            ),
            two_up(
                labelled_field(
                    "Factor",
                    &dlg.factor_text,
                    "1",
                    move |s| ad(AreaDialogMsg::Factor(s)),
                    d
                ),
                measure_pick.into(),
                compact,
            ),
            section_box(
                column![
                    label_text("Area Sign & Auto-Subtract"),
                    row![
                        radio_row("Positive (+)", true, Some(dlg.positive), move |v| ad(
                            AreaDialogMsg::Positive(v)
                        )),
                        radio_row("Negative (-)", false, Some(dlg.positive), move |v| ad(
                            AreaDialogMsg::Positive(v)
                        )),
                    ]
                    .spacing(SPACE_4),
                    row![
                        label_text("Auto-subtract:"),
                        radio_row("Automatic", "Automatic", Some(auto_sub), move |v| ad(
                            AreaDialogMsg::AutoSubtract(v.into())
                        )),
                        radio_row("Manual", "Manual", Some(auto_sub), move |v| ad(
                            AreaDialogMsg::AutoSubtract(v.into())
                        )),
                    ]
                    .spacing(SPACE_3)
                    .wrap(),
                ]
                .spacing(SPACE_2),
            ),
            section_box(
                column![
                    label_text("Auto-Post Label Parts"),
                    row![
                        check_row(
                            "Code",
                            post.code,
                            move |b| ad(AreaDialogMsg::Post(PostSwitch::Code, b)),
                            d
                        ),
                        check_row(
                            "Name",
                            post.name,
                            move |b| ad(AreaDialogMsg::Post(PostSwitch::Name, b)),
                            d
                        ),
                        check_row(
                            "Dimensions",
                            post.dims,
                            move |b| ad(AreaDialogMsg::Post(PostSwitch::Dims, b)),
                            d
                        ),
                        check_row(
                            "Math",
                            post.calcs,
                            move |b| ad(AreaDialogMsg::Post(PostSwitch::Calcs, b)),
                            d
                        ),
                        check_row(
                            "Base",
                            post.base,
                            move |b| ad(AreaDialogMsg::Post(PostSwitch::Base, b)),
                            d
                        ),
                        check_row(
                            "Factor",
                            post.factor,
                            move |b| ad(AreaDialogMsg::Post(PostSwitch::Factor, b)),
                            d
                        ),
                    ]
                    .spacing(SPACE_3)
                    .wrap(),
                    labelled_field(
                        "Suffix",
                        &dlg.suffix_text,
                        "sf",
                        move |s| ad(AreaDialogMsg::Suffix(s)),
                        d
                    ),
                ]
                .spacing(SPACE_2),
            ),
        ]
        .spacing(SPACE_3),
    )
    .height(Length::Fill);

    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Cancel", Some(ad(AreaDialogMsg::Cancel)), d),
        kit::primary_button("Apply", Some(ad(AreaDialogMsg::Apply)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Area Properties".into(),
            subtitle: None,
            preferred_w: 560.0,
            preferred_h: Some(540.0),
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(ad(AreaDialogMsg::Cancel)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 12. Color Picker
// ---------------------------------------------------------------------------

struct CursorProgram {
    point: Point,
}

impl canvas::Program<Message> for CursorProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.stroke(
            &Path::circle(self.point, 5.5),
            Stroke::default()
                .with_width(2.0)
                .with_color(Color::from_rgba(0.0, 0.0, 0.0, 0.7)),
        );
        frame.stroke(
            &Path::circle(self.point, 4.5),
            Stroke::default().with_width(1.5).with_color(Color::WHITE),
        );
        vec![frame.into_geometry()]
    }
}

fn color_surface<'a>(
    w: f32,
    h: f32,
    img_handle: image::Handle,
    cursor_pt: Point,
    area: ColorArea,
) -> Element<'a, Message> {
    let visual = container(
        stack![
            image(img_handle)
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(iced::ContentFit::Fill),
            Canvas::new(CursorProgram { point: cursor_pt })
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fixed(w))
    .height(Length::Fixed(h))
    .padding(1)
    .style(|_| container::Style {
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    });

    mouse_area(visual)
        .on_press(Message::Color(ColorMsg::Press(area)))
        .on_move(move |p| Message::Color(ColorMsg::Hover(area, p)))
        .on_release(Message::Color(ColorMsg::Release))
        .into()
}

fn color_dialog_layer<'a>(
    dlg: &'a ColorDialog,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !dlg.open {
        return None;
    }
    let cm = |m: ColorMsg| Message::Color(m);
    let compact = size_class == SizeClass::Compact;

    let tabs = row![
        kit::chip(
            "Standard",
            dlg.tab == ColorTab::Standard,
            Some(cm(ColorMsg::Tab(ColorTab::Standard))),
            d
        ),
        kit::chip(
            "Custom",
            dlg.tab == ColorTab::Custom,
            Some(cm(ColorMsg::Tab(ColorTab::Custom))),
            d
        ),
    ]
    .spacing(SPACE_2);

    let (r, g, b) = dlg.hsv.rgb();
    let new_col = Color::from_rgb8(r, g, b);
    let preview = column![
        label_text("New"),
        container(Space::new())
            .width(Length::Fixed(64.0))
            .height(Length::Fixed(48.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(new_col)),
                border: Border {
                    color: tokens::rule(),
                    width: 1.0,
                    radius: RADIUS_CONTROL.into(),
                },
                ..container::Style::default()
            }),
        text(dlg.hsv.hex())
            .font(BSC_MEDIUM)
            .size(TEXT_12)
            .color(tokens::text()),
    ]
    .spacing(SPACE_1)
    .align_x(Alignment::Center);

    let picker_row: Element<'a, Message> = match dlg.tab {
        ColorTab::Standard => {
            let wheel = color_surface(
                202.0,
                202.0,
                color_dialog::WHEEL.clone(),
                dlg.wheel_cursor(),
                ColorArea::Wheel,
            );
            let mut swatches = column![label_text("Presets")].spacing(SPACE_1);
            for chunk in color_dialog::PRESETS.chunks(8) {
                let mut r = row![].spacing(4);
                for &hex_s in chunk {
                    let (sr, sg, sb) = crate::color_dialog::hex_to_rgb(hex_s);
                    let c = Color::from_rgb8(sr, sg, sb);
                    let active = dlg.hsv.hex().eq_ignore_ascii_case(hex_s);
                    r = r.push(
                        mouse_area(
                            container(Space::new())
                                .width(Length::Fixed(28.0))
                                .height(Length::Fixed(22.0))
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(c)),
                                    border: Border {
                                        color: if active {
                                            tokens::accent()
                                        } else {
                                            tokens::rule()
                                        },
                                        width: if active { 2.0 } else { 1.0 },
                                        radius: 2.0.into(),
                                    },
                                    ..container::Style::default()
                                }),
                        )
                        .on_press(cm(ColorMsg::Preset(hex_s))),
                    );
                }
                swatches = swatches.push(r);
            }
            column![
                row![wheel, preview]
                    .spacing(SPACE_4)
                    .align_y(Alignment::Center),
                swatches,
            ]
            .spacing(SPACE_3)
            .into()
        }
        ColorTab::Custom => {
            let sv = color_surface(
                200.0,
                200.0,
                dlg.sv.1.clone(),
                dlg.sv_cursor(),
                ColorArea::Sv,
            );
            let hue = color_surface(
                24.0,
                200.0,
                color_dialog::HUE.clone(),
                dlg.hue_cursor(),
                ColorArea::Hue,
            );
            let rgb_col = three_up(
                labelled_field(
                    "Red",
                    &dlg.rgb[0],
                    "0",
                    move |s| cm(ColorMsg::Rgb(RgbField::R, s)),
                    d,
                ),
                labelled_field(
                    "Green",
                    &dlg.rgb[1],
                    "0",
                    move |s| cm(ColorMsg::Rgb(RgbField::G, s)),
                    d,
                ),
                labelled_field(
                    "Blue",
                    &dlg.rgb[2],
                    "0",
                    move |s| cm(ColorMsg::Rgb(RgbField::B, s)),
                    d,
                ),
                compact,
            );
            column![
                row![sv, hue, preview]
                    .spacing(SPACE_3)
                    .align_y(Alignment::Center),
                rgb_col,
            ]
            .spacing(SPACE_3)
            .into()
        }
    };

    let body = scrollable(column![tabs, picker_row].spacing(SPACE_3));
    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Cancel", Some(cm(ColorMsg::Close)), d),
        kit::primary_button("OK", Some(cm(ColorMsg::Ok)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Colour".into(),
            subtitle: None,
            preferred_w: 390.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(cm(ColorMsg::Close)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 13. Room Shapes
// ---------------------------------------------------------------------------

fn room_shapes_layer<'a>(
    rs: &'a RoomShapes,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !rs.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let is_l = rs.kind == "lshape";
    let fld = |k: &'static str| move |s: String| Message::RoomShapeField(k.into(), s);

    let shape_seg = kit::segmented(
        vec![
            kit::Segment {
                label: "Rectangle",
                selected: !is_l,
                focused: false,
                on_press: Some(Message::RoomShapeField("kind".into(), "rectangle".into())),
            },
            kit::Segment {
                label: "L-Shape",
                selected: is_l,
                focused: false,
                on_press: Some(Message::RoomShapeField("kind".into(), "lshape".into())),
            },
        ],
        d,
    );

    let mut col = column![
        column![label_text("Shape"), shape_seg].spacing(SPACE_1),
        two_up(
            labelled_field("Width", &rs.width, "12", fld("width"), d),
            labelled_field("Depth", &rs.depth, "10", fld("depth"), d),
            compact,
        ),
    ]
    .spacing(SPACE_3);

    if is_l {
        col = col.push(two_up(
            labelled_field("Notch width", &rs.cut_width, "4", fld("cutw"), d),
            labelled_field("Notch depth", &rs.cut_depth, "4", fld("cutd"), d),
            compact,
        ));
    }
    col = col.push(labelled_field(
        "Rotation (deg)",
        &rs.rotation,
        "0",
        fld("rotation"),
        d,
    ));

    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Cancel", Some(Message::RoomShapeCancel), d),
        kit::primary_button("Place on canvas", Some(Message::RoomShapePlace), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Insert Room Shape".into(),
            subtitle: Some("Configure dimensions and place on the sketch".into()),
            preferred_w: 440.0,
            preferred_h: None,
            body: col.into(),
            footer: Some(footer.into()),
            on_close: Some(Message::RoomShapeCancel),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 14. Laser / Disto Picker
// ---------------------------------------------------------------------------

fn disto_layer<'a>(
    disto: &'a Disto,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !disto.open() {
        return None;
    }
    let dm = |m: DistoMsg| Message::Disto(m);
    let status = match disto.phase {
        DistoPhase::Scanning => "Scanning for Bluetooth devices…".to_string(),
        DistoPhase::Connecting => "Connecting…".to_string(),
        _ if disto.devices.is_empty() => "No Bluetooth devices were found.".to_string(),
        _ => format!(
            "{} device{} found",
            disto.devices.len(),
            if disto.devices.len() == 1 { "" } else { "s" }
        ),
    };

    let mut list_col = column![].spacing(SPACE_1);
    for (i, dev) in disto.devices.iter().enumerate() {
        list_col = list_col.push(kit::list_row_owned(
            kit::OwnedListRowProps {
                swatch: None,
                title: dev.display(),
                value: None,
                selected: disto.selected == Some(i),
                flagged: false,
                focused: false,
                on_press: Some(dm(DistoMsg::Pick(i))),
            },
            d,
        ));
    }

    let list_box = container(scrollable(list_col).id("laser-list").height(Length::Fill))
        .height(Length::Fixed(220.0))
        .width(Length::Fill)
        .padding(SPACE_2)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::field())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_TILE.into(),
            },
            ..container::Style::default()
        });

    let busy = matches!(disto.phase, DistoPhase::Scanning | DistoPhase::Connecting);
    let can_connect = disto.phase == DistoPhase::Picking && disto.selected.is_some();

    let body = column![label_text(status), list_box].spacing(SPACE_2);
    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Cancel", Some(dm(DistoMsg::Cancel)), d),
        kit::text_button("Rescan", (!busy).then_some(dm(DistoMsg::Rescan)), d),
        kit::primary_button("Connect", can_connect.then_some(dm(DistoMsg::Choose)), d),
    ]
    .spacing(SPACE_2);

    Some(dialog_frame(
        DialogFrame {
            title: "Select Bluetooth Laser".into(),
            subtitle: Some("Pair a Leica DISTO or compatible laser measure".into()),
            preferred_w: 460.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(dm(DistoMsg::Cancel)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 15. Room Labels Manager
// ---------------------------------------------------------------------------

fn text_manager_layer<'a>(
    app: &'a App,
    tm: &'a TextManager,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !tm.open {
        return None;
    }
    let tmm = |m: TextManagerMsg| Message::TextLibraryManager(m);
    let visible = visible_labels(&app.dock.text_labels, tm.range);

    let mut chips = row![].spacing(SPACE_1);
    for &(label, key) in ALPHA_RANGES {
        chips = chips.push(kit::chip(
            label,
            tm.range == key,
            Some(tmm(TextManagerMsg::Range(key))),
            d,
        ));
    }

    let mut list_col = column![].spacing(SPACE_1);
    if visible.is_empty() {
        list_col = list_col.push(
            container(label_text("No labels in this range"))
                .padding(SPACE_3)
                .width(Length::Fill),
        );
    } else {
        for lbl in &visible {
            let selected = tm.selected.as_deref() == Some(lbl.as_str());
            list_col = list_col.push(kit::list_row_owned(
                kit::OwnedListRowProps {
                    swatch: None,
                    title: lbl.clone(),
                    value: None,
                    selected,
                    flagged: false,
                    focused: false,
                    on_press: Some(tmm(TextManagerMsg::Pick(lbl.clone()))),
                },
                d,
            ));
        }
    }

    let list_box = container(
        scrollable(list_col)
            .id("room-labels-list")
            .height(Length::Fill),
    )
    .height(Length::Fixed(240.0))
    .width(Length::Fill)
    .padding(SPACE_2)
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::field())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_TILE.into(),
        },
        ..container::Style::default()
    });

    let input = kit::field(
        FieldProps {
            label: "Label text",
            value: &tm.buffer,
            placeholder: "e.g. Primary Bedroom",
            on_input: Some(Box::new(move |s| tmm(TextManagerMsg::Input(s)))),
            on_submit: Some(tmm(TextManagerMsg::Add)),
            focused: false,
        },
        d,
    );

    let body = column![
        scrollable(container(chips).padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 12.0,
            left: 0.0,
        }),)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill),
        list_box,
        input,
    ]
    .spacing(SPACE_3);

    let footer = row![
        kit::text_button("Add", Some(tmm(TextManagerMsg::Add)), d),
        kit::text_button(
            "Replace",
            tm.selected
                .is_some()
                .then_some(tmm(TextManagerMsg::Replace)),
            d,
        ),
        kit::text_button(
            "Remove",
            tm.selected.is_some().then_some(tmm(TextManagerMsg::Remove)),
            d,
        ),
        Space::new().width(Length::Fill),
        kit::primary_button("Close", Some(tmm(TextManagerMsg::Close)), d),
    ]
    .spacing(SPACE_2)
    .wrap();

    Some(dialog_frame(
        DialogFrame {
            title: "Room Labels".into(),
            subtitle: Some("Manage reusable room and annotation presets".into()),
            preferred_w: 500.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(tmm(TextManagerMsg::Close)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 16. Undo History
// ---------------------------------------------------------------------------

fn undo_history_layer<'a>(
    app: &'a App,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    app.history.open?;
    let labels = app.undo.labels();
    let mut list_col = column![].spacing(SPACE_1);
    if labels.is_empty() {
        list_col = list_col.push(label_text("Nothing to undo."));
    } else {
        for (i, lbl) in labels.iter().enumerate() {
            let index = labels.len() - 1 - i;
            list_col = list_col.push(kit::list_row_owned(
                kit::OwnedListRowProps {
                    swatch: None,
                    title: format!("{}. {}", index + 1, lbl),
                    value: Some("Rewind".to_string()),
                    selected: false,
                    flagged: false,
                    focused: false,
                    on_press: Some(Message::UndoTo(index)),
                },
                d,
            ));
        }
    }

    let body = container(scrollable(list_col).id("undo-list").height(Length::Fill))
        .max_height(340.0)
        .width(Length::Fill);

    let footer = row![
        Space::new().width(Length::Fill),
        kit::primary_button("Close", Some(Message::Ui(UiMsg::CloseOverlays)), d),
    ];

    Some(dialog_frame(
        DialogFrame {
            title: "Undo History".into(),
            subtitle: Some("Select an action to rewind before it".into()),
            preferred_w: 380.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(Message::Ui(UiMsg::CloseOverlays)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 17. Keyboard Shortcuts (rewritten Next vocabulary)
// ---------------------------------------------------------------------------

fn kbd_pill<'a>(keys: &'static str) -> Element<'a, Message> {
    container(
        text(keys)
            .font(BSC_MEDIUM)
            .size(TEXT_11)
            .color(tokens::text()),
    )
    .padding([2, 8])
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::bar())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn shortcuts_layer<'a>(
    sc: &'a Shortcuts,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !sc.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let sc_close = Message::Shortcuts(ShortcutsMsg::Close);

    let mut groups: Vec<Element<'a, Message>> = Vec::new();
    for &(title, entries) in SHORTCUT_GROUPS {
        let mut items = column![text(title)
            .font(BARLOW_SEMIBOLD)
            .size(TEXT_13)
            .color(tokens::accent())]
        .spacing(SPACE_2);

        for &(keys, action) in entries {
            items = items.push(
                row![
                    text(action)
                        .font(BARLOW)
                        .size(TEXT_13)
                        .color(tokens::text())
                        .width(Length::Fill),
                    kbd_pill(keys),
                ]
                .spacing(SPACE_2)
                .align_y(Alignment::Center),
            );
        }
        groups.push(section_box(items));
    }

    let grid: Element<'a, Message> = if compact {
        let mut col = column![].spacing(SPACE_3);
        for g in groups {
            col = col.push(g);
        }
        col.into()
    } else {
        // Three columns (four once the dialog is 1100 wide, so a short
        // 1382x816 screen still fits), each group going to the shortest
        // column so far (weighted by its row count plus its heading), which
        // keeps the whole sheet on screen.
        let n = if win.width - 64.0 >= 1100.0 { 4 } else { 3 };
        let mut cols: Vec<Vec<Element<'a, Message>>> = (0..n).map(|_| Vec::new()).collect();
        let mut heights = vec![0usize; n];
        for (g, &(_, entries)) in groups.into_iter().zip(SHORTCUT_GROUPS.iter()) {
            let k = (0..n).min_by_key(|&k| heights[k]).unwrap_or(0);
            heights[k] += entries.len() + 2;
            cols[k].push(g);
        }
        let mut r = row![].spacing(SPACE_3).align_y(Alignment::Start);
        for c in cols {
            let mut col = column![].spacing(SPACE_3).width(Length::FillPortion(1));
            for g in c {
                col = col.push(g);
            }
            r = r.push(col);
        }
        r.into()
    };

    let footer = row![
        Space::new().width(Length::Fill),
        kit::primary_button("Close", Some(sc_close.clone()), d),
    ];

    Some(dialog_frame(
        DialogFrame {
            title: "Keyboard Shortcuts".into(),
            subtitle: Some("Quick keys for tools, navigation, and editing".into()),
            preferred_w: 1200.0,
            preferred_h: Some(860.0),
            body: scrollable(grid).height(Length::Fill).into(),
            footer: Some(footer.into()),
            on_close: Some(sc_close),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 18. User Guide + Send to Appraise-It Pro
// ---------------------------------------------------------------------------

fn render_guide_block<'a>(block: &GuideBlock) -> Element<'a, Message> {
    match block {
        GuideBlock::Heading(h) => container(
            text(*h)
                .font(BARLOW_SEMIBOLD)
                .size(TEXT_14)
                .color(tokens::text()),
        )
        .padding(Padding {
            top: SPACE_2,
            ..Padding::ZERO
        })
        .into(),
        GuideBlock::Para(p) => body_text(*p),
        GuideBlock::Points(pts) => {
            let mut col = column![].spacing(SPACE_1);
            for pt in *pts {
                col = col.push(
                    row![
                        text("•")
                            .font(BARLOW_SEMIBOLD)
                            .size(TEXT_13)
                            .color(tokens::accent()),
                        text(*pt)
                            .font(BARLOW)
                            .size(TEXT_13)
                            .color(tokens::text())
                            .width(Length::Fill),
                    ]
                    .spacing(SPACE_2),
                );
            }
            col.into()
        }
        GuideBlock::Steps(steps) => {
            let mut col = column![].spacing(SPACE_1);
            for (idx, step) in steps.iter().enumerate() {
                col = col.push(
                    row![
                        text(format!("{}.", idx + 1))
                            .font(BSC_SEMIBOLD)
                            .size(TEXT_13)
                            .color(tokens::accent()),
                        text(*step)
                            .font(BARLOW)
                            .size(TEXT_13)
                            .color(tokens::text())
                            .width(Length::Fill),
                    ]
                    .spacing(SPACE_2),
                );
            }
            col.into()
        }
        GuideBlock::Note(n) => container(
            row![
                mono("info", 15.0, tokens::accent()),
                text(*n)
                    .font(BARLOW)
                    .size(TEXT_13)
                    .color(tokens::text())
                    .width(Length::Fill),
            ]
            .spacing(SPACE_2)
            .align_y(Alignment::Center),
        )
        .padding(SPACE_3)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::accent_soft())),
            border: Border {
                color: tokens::accent(),
                width: 1.0,
                radius: RADIUS_TILE.into(),
            },
            ..container::Style::default()
        })
        .into(),
    }
}

fn guide_layer<'a>(
    guide_state: &'a Guide,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !guide_state.open {
        return None;
    }
    let compact = size_class == SizeClass::Compact;
    let current = guide_state.topic.min(TOPICS.len() - 1);
    let topic = &TOPICS[current];
    let gm = |m: GuideMsg| Message::Guide(m);

    let topic_nav: Element<'a, Message> = if compact {
        let mut r = row![].spacing(SPACE_1);
        for (i, t) in TOPICS.iter().enumerate() {
            r = r.push(kit::chip(
                t.title,
                i == current,
                Some(gm(GuideMsg::Topic(i))),
                d,
            ));
        }
        scrollable(container(r).padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 12.0,
            left: 0.0,
        }))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill)
        .into()
    } else {
        let mut col = column![].spacing(SPACE_1).width(Length::Fixed(200.0));
        for (i, t) in TOPICS.iter().enumerate() {
            col = col.push(kit::list_row(
                kit::ListRowProps {
                    swatch: None,
                    title: t.title,
                    value: None,
                    selected: i == current,
                    flagged: false,
                    focused: false,
                    on_press: Some(gm(GuideMsg::Topic(i))),
                },
                d,
            ));
        }
        col.into()
    };

    let mut article = column![text(topic.title)
        .font(BARLOW_SEMIBOLD)
        .size(TEXT_16)
        .color(tokens::text())]
    .spacing(SPACE_2);

    #[cfg(windows)]
    if current == INTEGRATION_TOPIC {
        let mut report_col = column![
            body_text("Check for an open Appraise-It Pro report session and preview sketch pages before sending."),
            row![kit::primary_button(
                "Check open report",
                (!guide_state.report_busy).then_some(gm(GuideMsg::CheckReport)),
                d,
            )]
            .spacing(SPACE_2),
        ]
        .spacing(SPACE_2);

        if let Some(status) = &guide_state.report_status {
            report_col = report_col.push(body_text(status.clone()));
        }
        if guide_state.report_target.is_some() && guide_state.sketch_preview.is_empty() {
            report_col = report_col.push(kit::text_button(
                if guide_state.report_busy {
                    "Rendering sketch…"
                } else {
                    "Preview sketch"
                },
                (!guide_state.report_busy).then_some(gm(GuideMsg::PreviewSketch)),
                d,
            ));
        }
        if let Some(report) = guide_state
            .report_target
            .as_ref()
            .filter(|_| !guide_state.sketch_preview.is_empty())
        {
            report_col = report_col.push(body_text(format!(
                "Sending appends these pages to {}.",
                report.sketch_destination()
            )));
            for (number, img_handle) in &guide_state.sketch_preview {
                report_col = report_col
                    .push(label_text(format!("Sketch page {number}")))
                    .push(
                        image(img_handle.clone())
                            .width(Length::Fill)
                            .height(Length::Fixed(220.0)),
                    );
            }
            match &guide_state.area_preview {
                Some(Ok(preview)) => {
                    report_col = report_col.push(check_row(
                        "Include area totals",
                        guide_state.include_areas,
                        move |v| gm(GuideMsg::IncludeAreas(v)),
                        d,
                    ));
                    if guide_state.include_areas {
                        report_col = report_col
                            .push(body_text("Review replacements for the subject living unit before transferring totals."))
                            .push(body_text(preview.summary()));
                    }
                }
                Some(Err(reason)) => {
                    report_col = report_col.push(label_text(reason.clone()));
                }
                None => {}
            }
            let send_lbl = if guide_state.include_areas {
                "Send sketch and area totals"
            } else {
                "Send sketch images"
            };
            report_col = report_col.push(kit::primary_button(
                send_lbl,
                Some(gm(GuideMsg::SendSketch)),
                d,
            ));
        }
        article = article.push(section_box(report_col));
    }

    for b in topic.blocks {
        article = article.push(render_guide_block(b));
    }

    let article_scroll = scrollable(container(article).padding(Padding {
        right: SPACE_2,
        ..Padding::ZERO
    }))
    .id("guide-article-list")
    .height(Length::Fill)
    .width(Length::Fill);

    let body: Element<'a, Message> = if compact {
        column![topic_nav, kit::divider(), article_scroll]
            .spacing(SPACE_2)
            .height(Length::Fill)
            .into()
    } else {
        row![
            topic_nav,
            container(Space::new().width(1.0))
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(tokens::rule())),
                    ..container::Style::default()
                }),
            article_scroll,
        ]
        .spacing(SPACE_3)
        .height(Length::Fill)
        .into()
    };

    let footer = row![
        Space::new().width(Length::Fill),
        kit::primary_button("Close", Some(gm(GuideMsg::Close)), d),
    ];

    Some(dialog_frame(
        DialogFrame {
            title: "User Guide".into(),
            subtitle: Some(
                "Workspace navigation, drawing workflows, and report integration".into(),
            ),
            preferred_w: 840.0,
            preferred_h: Some(640.0),
            body,
            footer: Some(footer.into()),
            on_close: Some(gm(GuideMsg::Close)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// 19. Geo Reference Menu
// ---------------------------------------------------------------------------

fn geo_menu_layer<'a>(
    open: bool,
    win: Size,
    size_class: SizeClass,
    d: Density,
) -> Option<Element<'a, Message>> {
    if !open {
        return None;
    }
    let body = column![
        kit::list_row(
            kit::ListRowProps {
                swatch: None,
                title: "Load Map Image…",
                value: None,
                selected: false,
                flagged: false,
                focused: false,
                on_press: Some(Message::GeoLoadMap),
            },
            d,
        ),
        kit::list_row(
            kit::ListRowProps {
                swatch: None,
                title: "Auto Locate by Coordinate…",
                value: None,
                selected: false,
                flagged: false,
                focused: false,
                on_press: Some(Message::GeoCoordinate),
            },
            d,
        ),
        kit::list_row(
            kit::ListRowProps {
                swatch: None,
                title: "Auto Locate by Address…",
                value: None,
                selected: false,
                flagged: false,
                focused: false,
                on_press: Some(Message::GeoAddress),
            },
            d,
        ),
    ]
    .spacing(SPACE_1);

    let footer = row![
        Space::new().width(Length::Fill),
        kit::text_button("Close", Some(Message::Ui(UiMsg::CloseOverlays)), d),
    ];

    Some(dialog_frame(
        DialogFrame {
            title: "Reference Map & Location".into(),
            subtitle: None,
            preferred_w: 360.0,
            preferred_h: None,
            body: body.into(),
            footer: Some(footer.into()),
            on_close: Some(Message::Ui(UiMsg::CloseOverlays)),
        },
        win,
        size_class,
        d,
    ))
}

// ---------------------------------------------------------------------------
// Native file pickers for area type import and export
// ---------------------------------------------------------------------------

#[cfg(not(test))]
pub async fn pick_codes_import() -> Option<std::path::PathBuf> {
    crate::platform::pick_file_async("Area types JSON".to_owned(), vec!["json".to_owned()]).await
}

#[cfg(not(test))]
pub async fn pick_codes_export() -> Option<std::path::PathBuf> {
    crate::platform::save_file_async(
        "Area types JSON".to_owned(),
        vec!["json".to_owned()],
        "sketch-area-types.json".to_owned(),
    )
    .await
}

#[cfg(test)]
pub async fn pick_codes_import() -> Option<std::path::PathBuf> {
    std::future::pending().await
}

#[cfg(test)]
pub async fn pick_codes_export() -> Option<std::path::PathBuf> {
    std::future::pending().await
}
