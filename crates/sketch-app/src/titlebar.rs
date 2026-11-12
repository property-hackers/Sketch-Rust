//! Title bar (0,0 1400x30, `#1e1e1e`, 1px `--border` bottom): QAT logo +
//! buttons + caret, the save-state pill ("New document" / "Unsaved" / "Saved"), the centered window
//! title, and the window controls. Empty title-bar space drags the window.

use iced::widget::{button, container, mouse_area, row, stack, Space};
use iced::{alignment, Background, Border, Color, Element, Length, Padding};

use crate::icons::{ph, ph_fill, qat_logo};
use crate::theme::*;
use crate::Message;

const BAR_H: f32 = 29.0;

/// `.qat-btn` — 28x28, icon 13px at (7,7); hover `--bg-hover`.
/// A quick-access button. The press rides the button itself: a `button`
/// captures the press, so an outer `mouse_area` would never see it.
fn qat_btn<'a>(
    icon: Element<'a, Message>,
    action: Option<Message>,
    title: &'static str,
) -> Element<'a, Message> {
    let b = button(container(icon).padding(Padding {
        top: 7.0,
        left: 7.0,
        ..Padding::ZERO
    }))
    .width(28)
    .height(28)
    .padding(0)
    .style(|_, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
            _ => None,
        },
        ..button::Style::default()
    });
    let b: Element<'a, Message> = match action {
        Some(msg) => b.on_press(msg).into(),
        None => b.into(),
    };
    crate::tooltips::tip_below(b, title)
}

/// `.window-controls button` — 46x29, icon 13px at (16,8).
fn control<'a>(
    icon_name: &str,
    msg: Message,
    hover_bg: Color,
    icon_color: Color,
    title: &'static str,
) -> Element<'a, Message> {
    let b: Element<'a, Message> =
        button(container(ph(icon_name, 13.0, icon_color)).padding(Padding {
            top: 8.0,
            left: 16.0,
            ..Padding::ZERO
        }))
        .width(46)
        .height(BAR_H)
        .padding(0)
        .on_press(msg)
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => Some(hover_bg.into()),
                _ => None,
            },
            ..button::Style::default()
        })
        .into();
    crate::tooltips::tip_below(b, title)
}

pub fn view<'a>(
    close_hover: bool,
    maximized: bool,
    save_state: &str,
    save_final: bool,
    undo_enabled: bool,
    redo_enabled: bool,
    title: String,
) -> Element<'a, Message> {
    let dim = over(text_main(), 0.38, bg_app());
    // Save, Undo, Redo, Undo History, Print, Zoom. Undo/Redo follow the
    // stacks (`updateUndoRedoControls`); the history button is always live
    // because it reports an empty stack itself.
    let qat = [
        (
            ph_fill("floppy-disk", 13.0, text_main()),
            true,
            Some(Message::SaveFile),
            "Save (F2)",
        ),
        (
            ph(
                "arrow-u-up-left",
                13.0,
                if undo_enabled { text_main() } else { dim },
            ),
            undo_enabled,
            Some(Message::Undo),
            "Undo (Ctrl+Z)",
        ),
        (
            ph(
                "arrow-u-up-right",
                13.0,
                if redo_enabled { text_main() } else { dim },
            ),
            redo_enabled,
            Some(Message::Redo),
            "Redo (Ctrl+Y)",
        ),
        (
            ph("list-bullets", 13.0, text_main()),
            true,
            Some(Message::UndoHistory),
            "Undo History",
        ),
        (
            ph("printer", 13.0, text_main()),
            true,
            Some(Message::Print),
            "Print",
        ),
        (
            ph("magnifying-glass", 13.0, text_main()),
            true,
            Some(Message::CommandPaletteOpen),
            "Command search (Ctrl+K)",
        ),
    ];

    let mut left = stack![at(0.0, 5.0, qat_logo())].width(318).height(BAR_H);
    for (i, (icon, enabled, action, title)) in qat.into_iter().enumerate() {
        // A disabled button (an empty undo stack) takes no press.
        let action = if enabled { action } else { None };
        left = left.push(at(
            20.0 + 28.0 * i as f32,
            1.0,
            qat_btn(icon, action, title),
        ));
    }
    // `.qat-more`: the caret (10px at 192,10) opens the quick-access menu.
    let caret: Element<'a, Message> = button(
        container(ph("caret-down", 10.0, text_main())).padding(Padding {
            top: 9.0,
            left: 4.0,
            ..Padding::ZERO
        }),
    )
    .width(18)
    .height(28)
    .padding(0)
    .on_press(Message::PopupMenu(crate::popup_menu::PopupMsg::Open(
        crate::popup_menu::Kind::QuickAccess,
        188.0,
        BAR_H,
    )))
    .style(|_, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
            _ => None,
        },
        ..button::Style::default()
    })
    .into();
    left = left
        .push(at(
            188.0,
            1.0,
            crate::tooltips::tip_below(caret, "More commands"),
        ))
        // `.doc-save-state` pill: 96x21, radius 999, padding 2px 10px.
        .push(at(
            214.0,
            4.0,
            boxed(
                container(label(
                    save_state,
                    11.0,
                    SEGOE,
                    if save_final { hex(0x5dae84) } else { muted() },
                    15.0,
                )).padding(Padding {
                    top: 3.0,
                    left: 10.0,
                    ..Padding::ZERO
                }),
                96,
                21,
                Some(bg_panel()),
                border(),
                10.5,
            ),
        ));

    let title = container(label(&title, 12.0, SEGOE, text_main(), 16.0))
        .width(Length::Fill)
        .height(BAR_H)
        .align_x(alignment::Horizontal::Center)
        .padding(Padding {
            top: 7.0,
            ..Padding::ZERO
        });

    let close_icon = if close_hover { WHITE } else { text_main() };
    let controls = row![
        control(
            "minus",
            Message::Minimize,
            bg_hover(),
            text_main(),
            "Minimize"
        ),
        // Two overlapping squares while maximized, as on a native caption.
        control(
            if maximized { "copy" } else { "square" },
            Message::ToggleMaximize,
            bg_hover(),
            text_main(),
            if maximized { "Restore" } else { "Maximize" }
        ),
        mouse_area(control(
            "x",
            Message::Close,
            CLOSE_HOVER,
            close_icon,
            "Close"
        ))
        .on_enter(Message::CloseHover(true))
        .on_exit(Message::CloseHover(false)),
    ];

    let bar = mouse_area(row![left, title, controls].height(BAR_H))
        .on_press(Message::TitleDrag)
        .on_double_click(Message::ToggleMaximize);

    container(iced::widget::column![
        bar,
        container(Space::new().width(Length::Fill).height(1)).style(|_| container::Style {
            background: Some(Background::Color(border())),
            ..container::Style::default()
        }),
    ])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(bg_app().into()),
        border: Border::default(),
        ..container::Style::default()
    })
    .into()
}
