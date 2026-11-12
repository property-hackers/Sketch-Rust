//! The app's modal dialog — UIManager.showDialog (alertDialog /
//! confirmDialog / promptDialog), measured from the live original:
//! backdrop `rgba(0,0,0,.46)`; dialog 380 wide, centred (510,348 for a
//! prompt), 1px border, radius 5, `--bg-panel`, padding 14/16, gap 10,
//! shadow `0 16px 46px rgba(0,0,0,.5)`; title 13px/600 (h17), message
//! 12px line 1.45 (h17); input 16px, padding 6/8, h35, radius 2,
//! `--bg-app`; actions right-aligned, gap 8, margin-top 2; buttons 16px,
//! padding 5/12, radius 2, min-width 72 (Cancel 73 wide, OK 72),
//! Cancel `--bg-hover`, OK `--bg-active` + white.
//!
//! Keys (document capture listener): Escape = Cancel (OK when there is no
//! Cancel), Enter = OK; a press on the backdrop = the same as Escape.

use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, mouse_area, opaque, row, text, text_input, Space};
use iced::{
    alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow, Vector,
};

use crate::snap::{nudge, nudge_y};
use crate::theme::*;
use crate::Message;

/// `.app-dialog-input`'s widget id (focused + selected on open).
pub const INPUT_ID: &str = "app-dialog-input";

/// Chrome paints the prompt's pre-selected value with this box in a
/// hidden/unfocused tab (the only state a reference can capture).
pub const SELECTION: Color = Color::from_rgb(110.0 / 255.0, 110.0 / 255.0, 110.0 / 255.0);

/// What OK does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// An alert: nothing.
    None,
    DeleteAreaType(String),
    /// `newSketch()` after "Start a new sketch? Unsaved changes will be
    /// lost." (Ctrl+N / File > New).
    New,
    /// `setNorthAngle(value)`.
    SetNorth,
    /// `addPage()` (Next on the last page).
    AddPage,
    /// `deletePage()` after "Delete page N?".
    DeletePage,
    /// A text label's "Text:" prompt (double-click): the label's id.
    EditText(i64),
    /// `deleteSelected()` after "Delete the selected sketch items?"
    /// (`options.promptOnDeletion`).
    DeleteSelected,
    /// A wall-shape prompt's value (showWallShapeMenu's chain).
    ShapeValue,
    /// The reference image's Set Scale prompt (the known feet value).
    PhotoScale,
    /// The Orient Guide's Horizontal/Vertical prompt (the input decides).
    PhotoOrient,
    /// Remove the background image's confirm.
    PhotoRemove,
    /// The Geo prompts (`showGeoReferenceDialog`).
    GeoCoordinate,
    GeoAddress,
    /// Save the linked sketch to the report and close after acknowledgment.
    ReturnToReport,
    /// Closing with unsaved changes: Save (then close), Don't save, Cancel.
    SaveAndExit,
    /// Open the sketch file dropped on the window, discarding the changes.
    OpenDropped(std::path::PathBuf),
    /// Mark as Final...: the optional note is the prompt's input.
    MarkFinal,
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub title: String,
    pub message: String,
    /// `Some(value)` for a prompt.
    pub input: Option<String>,
    pub has_cancel: bool,
    pub on_ok: Action,
}

impl Dialog {
    pub fn alert(message: &str) -> Self {
        Dialog {
            title: "SketchQuoder".into(),
            message: message.into(),
            input: None,
            has_cancel: false,
            on_ok: Action::None,
        }
    }

    pub fn confirm(message: &str, on_ok: Action) -> Self {
        Dialog {
            has_cancel: true,
            on_ok,
            ..Dialog::alert(message)
        }
    }

    pub fn report_close() -> Self {
        Self {
            title: "Close sketch".into(),
            ..Self::confirm(
                "Save this sketch and its measurements to the report before closing?",
                Action::ReturnToReport,
            )
        }
    }

    pub fn save_before_close(file_name: &str) -> Self {
        Self {
            title: "Close sketch".into(),
            ..Self::confirm(
                &format!("Save changes to \"{file_name}\" before closing?"),
                Action::SaveAndExit,
            )
        }
    }

    pub fn prompt(message: &str, value: &str, on_ok: Action) -> Self {
        Dialog {
            input: Some(value.into()),
            ..Dialog::confirm(message, on_ok)
        }
    }
}

#[derive(Debug, Clone)]
pub enum DialogMsg {
    Input(String),
    Ok,
    Cancel,
    Discard,
    /// Escape or a backdrop press: Cancel, or OK when there is no Cancel.
    Dismiss,
}

fn msg(m: DialogMsg) -> Message {
    Message::Dialog(m)
}

fn dialog_button<'a>(label: &str, primary: bool, on: DialogMsg) -> Element<'a, Message> {
    let (bg, border, fg) = if primary {
        (accent(), accent(), WHITE)
    } else {
        (bg_hover(), border(), text_main())
    };
    let caption = text(label.to_string())
        .font(SEGOE)
        .size(16)
        .line_height(LineHeight::Absolute(Pixels(21.0)))
        .color(fg)
        .align_x(alignment::Horizontal::Center)
        .width(Length::Fill);
    // CSS box: 1px border + 5/12 padding; min-width 72.
    let width = if label == "OK" {
        Length::Fixed(72.0)
    } else {
        Length::Shrink
    };
    // A fixed-width (OK) caption is centred at a fractional x, which iced
    // floors and Chrome rounds: nudge it left half a pixel too.
    let dx = if label == "OK" { -0.5 } else { 0.0 };
    button(nudge(dx, 0.5, caption))
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 13.0,
            right: 13.0,
        })
        .width(width)
        .on_press(msg(on))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: Border {
                color: border,
                width: 1.0,
                radius: 2.0.into(),
            },
            ..button::Style::default()
        })
        .into()
}

/// The full-window dialog layer.
pub fn layer<'a>(d: &Dialog) -> Element<'a, Message> {
    // Text rows are nudged 0.5px down so iced's floor(line top) lands where
    // Chrome's round(line top) does (see `snap::nudge_y`); the card itself
    // stays at its fractional centre, whose edges both renderers round. The
    // message's 1.45 line spreads its leading differently in iced, which
    // drew it a row low: it takes the opposite nudge (measured on the
    // prompt, confirm and alert references).
    let mut body = column![
        nudge_y(
            0.5,
            text(d.title.clone())
                .font(SEGOE_SEMIBOLD)
                .size(13)
                .line_height(LineHeight::Absolute(Pixels(17.0)))
                .color(text_main())
        ),
        nudge_y(
            -0.5,
            text(d.message.clone())
                .font(SEGOE)
                .size(12)
                .line_height(LineHeight::Absolute(Pixels(17.4)))
                .color(text_main())
        ),
    ]
    .spacing(10)
    .width(Length::Fill);
    if let Some(value) = &d.input {
        body = body.push(
            text_input("", value)
                .id(INPUT_ID)
                .on_input(|s| msg(DialogMsg::Input(s)))
                .on_submit(msg(DialogMsg::Ok))
                .font(SEGOE)
                .size(16)
                .line_height(LineHeight::Absolute(Pixels(21.0)))
                .padding(Padding {
                    top: 7.0,
                    bottom: 7.0,
                    left: 9.0,
                    right: 9.0,
                })
                .width(Length::Fill)
                .style(|_, _| text_input::Style {
                    background: Background::Color(bg_app()),
                    border: Border {
                        color: border(),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    icon: muted(),
                    placeholder: muted(),
                    value: text_main(),
                    selection: SELECTION,
                }),
        );
    }
    let mut actions = row![Space::new().width(Length::Fill)].spacing(8);
    if d.has_cancel {
        actions = actions.push(dialog_button("Cancel", false, DialogMsg::Cancel));
    }
    let report_close = d.on_ok == Action::ReturnToReport;
    let save_close = d.on_ok == Action::SaveAndExit;
    if report_close {
        actions = actions.push(dialog_button("Discard changes", false, DialogMsg::Discard));
    } else if save_close {
        actions = actions.push(dialog_button("Don't save", false, DialogMsg::Discard));
    }
    actions = actions.push(dialog_button(
        if report_close {
            "Save to report"
        } else if save_close {
            "Save"
        } else {
            "OK"
        },
        true,
        DialogMsg::Ok,
    ));
    body = body.push(column![Space::new().height(2), actions]);

    let card = container(body)
        .padding(Padding {
            top: 15.0,
            bottom: 15.0,
            left: 17.0,
            right: 17.0,
        })
        .width(if report_close { 480 } else { 380 })
        .style(|_| container::Style {
            background: Some(bg_panel().into()),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 5.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                offset: Vector::new(0.0, 16.0),
                blur_radius: 46.0,
            },
            ..container::Style::default()
        });
    // The card swallows its own presses; the backdrop dismisses.
    let centred = container(opaque(card))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.46).into()),
            ..container::Style::default()
        });
    opaque(mouse_area(centred).on_press(msg(DialogMsg::Dismiss)))
}
