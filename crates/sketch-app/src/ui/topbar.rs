//! Top bar, page tabs, tab context menu, export menu, and pages drawer overlay for `Shell::Next`.

use iced::widget::{button, column, container, mouse_area, opaque, row, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::calcs;
use crate::guide;
use crate::icons;
use crate::pages_nav::{self, NavMsg};
use crate::statusbar::StatusMsg;
use crate::theme::over;
use crate::ui::fonts::{BARLOW, BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_MEDIUM};
use crate::ui::kit::{self, MenuEntry};
use crate::ui::layout::{SizeClass, PAGE_BAR_H};
use crate::ui::tokens::{
    self, Density, RADIUS_CONTROL, RADIUS_DIALOG, SPACE_1, SPACE_2, SPACE_3, TEXT_11, TEXT_12,
    TEXT_14,
};
use crate::ui::Msg as UiMsg;
use crate::{App, Message};

const CLOSE_HOVER: Color = Color::from_rgb(0.77, 0.17, 0.12);
const COMPACT_CAPTION_BTN: f32 = 26.0;
const DESKTOP_CAPTION_BTN: f32 = 30.0;

/// Next top bar save-state text derived from `App::save_state()`.
pub fn save_state_label(app: &App) -> &'static str {
    match app.save_state() {
        "Unsaved" => "Edited",
        "New document" => "Not saved",
        other => other,
    }
}

/// Anchored popup origin `(x, y)` below the top bar's Undo button for `undo_history::layer`.
pub fn undo_history_anchor(app: &App) -> (f32, f32) {
    let class = SizeClass::of(app.window_size.width);
    let d = app.ui.density;
    let w = app.window_size.width;
    let bar_h = d.bar_h();
    let (x, y) = match class {
        SizeClass::Wide => {
            // Right cluster sits at the right edge; Undo is left of Search + Primary + Window controls.
            let right_cluster = if matches!(d, Density::Touch) {
                615.0
            } else {
                565.0
            };
            ((w - right_cluster).max(8.0), bar_h + 2.0)
        }
        SizeClass::Medium => {
            let right_cluster = if matches!(d, Density::Touch) {
                465.0
            } else {
                420.0
            };
            ((w - right_cluster).max(8.0), bar_h + 2.0)
        }
        SizeClass::Compact => ((w - 230.0).max(8.0), bar_h + PAGE_BAR_H + 2.0),
    };
    (x.min((w - 198.0).max(8.0)), y)
}

/// Primary top header bar (`Wide` single-row; `Medium` & `Compact` upper row).
pub fn top_bar<'a>(
    app: &'a App,
    class: SizeClass,
    d: Density,
    has_selection: bool,
) -> Element<'a, Message> {
    let compact = matches!(class, SizeClass::Compact);
    let menu_btn = menu_trigger(app.ui.menu_open, d, compact);
    let doc_btn = doc_title_button(app, class);

    let mut left = row![app_mark(d), menu_btn]
        .spacing(if compact { SPACE_1 } else { SPACE_2 })
        .align_y(Alignment::Center);

    if app.symbol_edit_session.is_none() {
        left = left
            .push(kit::with_id(
                "open",
                kit::tooltip(
                    kit::icon_button("folder-open", false, Some(Message::OpenFile), d),
                    "Open (Ctrl+O)",
                    iced::widget::tooltip::Position::Bottom,
                ),
            ))
            .push(kit::with_id(
                "save",
                kit::tooltip(
                    kit::icon_button("floppy-disk", false, Some(Message::SaveFile), d),
                    "Save (F2)",
                    iced::widget::tooltip::Position::Bottom,
                ),
            ));
    }
    left = left.push(doc_btn);

    if !compact {
        left = left.push(save_state_chip(app));
    }

    let editing_symbol = app.symbol_edit_session.as_ref();
    let content: Element<'a, Message> = match class {
        SizeClass::Wide => {
            let mut right = row![undo_redo_controls(app, d), search_trigger(class, d)]
                .spacing(SPACE_2)
                .align_y(Alignment::Center);
            if editing_symbol.is_none() {
                for el in primary_actions(app, class, d) {
                    right = right.push(el);
                }
            }
            right = right.push(window_controls(app, false));

            let center = if let Some(session) = editing_symbol {
                symbol_edit_strip(session, d, false)
            } else {
                page_tabs_strip(app, d, false)
            };

            row![
                left,
                Space::new().width(Length::Fill),
                center,
                Space::new().width(Length::Fill),
                right,
            ]
            .spacing(SPACE_2)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into()
        }
        SizeClass::Medium => {
            let mut right = row![undo_redo_controls(app, d), search_trigger(class, d)]
                .spacing(SPACE_2)
                .align_y(Alignment::Center);
            if editing_symbol.is_none() {
                right = right.push(kit::text_button_with(
                    "Areas",
                    Some(Message::Ui(UiMsg::ToggleLedger)),
                    d,
                    app.ui.ledger_open,
                    false,
                ));
            }
            if has_selection {
                right = right.push(kit::text_button_with(
                    "Inspect",
                    Some(Message::Ui(UiMsg::ToggleInspector)),
                    d,
                    app.ui.inspector_open,
                    false,
                ));
            }
            if editing_symbol.is_none() {
                for el in primary_actions(app, class, d) {
                    right = right.push(el);
                }
            }
            right = right.push(window_controls(app, false));

            row![left, Space::new().width(Length::Fill), right]
                .spacing(SPACE_2)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .into()
        }
        SizeClass::Compact => {
            let mut right = row![search_trigger(class, d)]
                .spacing(SPACE_1)
                .align_y(Alignment::Center);
            if editing_symbol.is_none() {
                for el in primary_actions(app, class, d) {
                    right = right.push(el);
                }
            }
            right = right.push(window_controls(app, true));

            row![left, Space::new().width(Length::Fill), right]
                .spacing(SPACE_1)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .into()
        }
    };

    let drag_bar = mouse_area(
        container(content)
            .height(Length::Fixed(d.bar_h()))
            .padding([0.0, if compact { SPACE_2 } else { SPACE_3 }])
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .on_press(Message::TitleDrag)
    .on_double_click(Message::ToggleMaximize);

    container(drag_bar)
        .width(Length::Fill)
        .height(Length::Fixed(d.bar_h()))
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::chrome())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// Second row used in `Medium` and `Compact` containing page tabs, compact tools, and totals.
pub fn page_bar<'a>(
    app: &'a App,
    class: SizeClass,
    d: Density,
    has_selection: bool,
) -> Element<'a, Message> {
    let compact = matches!(class, SizeClass::Compact);
    if let Some(session) = app.symbol_edit_session.as_ref() {
        let mut content = row![]
            .spacing(SPACE_2)
            .align_y(Alignment::Center)
            .width(Length::Fill);
        if compact {
            content = content.push(undo_redo_controls(app, d));
        }
        content = content.push(symbol_edit_strip(session, d, compact));
        return container(content)
            .height(Length::Fixed(PAGE_BAR_H))
            .padding([0.0, if compact { SPACE_2 } else { SPACE_3 }])
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(Background::Color(tokens::panel())),
                border: Border {
                    color: tokens::rule(),
                    width: 1.0,
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into();
    }

    let mut right = row![].spacing(SPACE_1).align_y(Alignment::Center);
    if compact {
        right = right.push(undo_redo_controls(app, d)).push(kit::chip(
            "Areas",
            app.ui.ledger_open,
            Some(Message::Ui(UiMsg::ToggleLedger)),
            d,
        ));
        if has_selection {
            right = right.push(kit::chip(
                "Inspect",
                app.ui.inspector_open,
                Some(Message::Ui(UiMsg::ToggleInspector)),
                d,
            ));
        }
    }
    right = right.push(totals_chip(app, compact));

    container(
        row![
            container(page_tabs_strip(app, d, compact)).width(Length::Fill),
            right,
        ]
        .spacing(SPACE_2)
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .height(Length::Fixed(PAGE_BAR_H))
    .padding([0.0, if compact { SPACE_2 } else { SPACE_3 }])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::panel())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// Overlay layer for top-bar popups (page tab context menu, Export menu, All pages navigator).
pub fn layer<'a>(app: &'a App) -> Option<Element<'a, Message>> {
    let d = app.ui.density;
    if let Some(index) = app.ui.page_menu.filter(|&i| i < app.pages.len()) {
        return Some(page_menu_overlay(app, index, d));
    }
    if app.ui.export_menu {
        return Some(export_menu_overlay(app, d));
    }
    if app.pages_nav_open {
        return Some(pages_nav_overlay(app, d));
    }
    None
}

// ---------------------------------------------------------------------------
// Top bar clusters
// ---------------------------------------------------------------------------

fn app_mark<'a>(d: Density) -> Element<'a, Message> {
    let size = if matches!(d, Density::Touch) {
        24.0
    } else {
        20.0
    };
    container(icons::qat_logo())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn menu_trigger<'a>(open: bool, d: Density, compact: bool) -> Element<'a, Message> {
    let btn = if compact {
        kit::tooltip(
            kit::icon_button("list", open, Some(Message::Ui(UiMsg::ToggleMenu)), d),
            "Menu",
            iced::widget::tooltip::Position::Bottom,
        )
    } else {
        kit::text_button_with("Menu", Some(Message::Ui(UiMsg::ToggleMenu)), d, open, false)
    };
    kit::with_id("menu", btn)
}

fn doc_title_button<'a>(app: &'a App, class: SizeClass) -> Element<'a, Message> {
    let label_text = app.file_name.as_str();
    let max_w = match class {
        SizeClass::Wide => 200.0,
        SizeClass::Medium => 180.0,
        SizeClass::Compact => 112.0,
    };
    let btn = button(
        container(
            text(label_text)
                .font(BARLOW_SEMIBOLD)
                .size(TEXT_14)
                .color(tokens::text()),
        )
        .padding([3, 6])
        .max_width(max_w)
        .clip(true),
    )
    .padding(0)
    .on_press(Message::Ui(UiMsg::PromptRenameDocument))
    .style(|_, status| {
        let bg = match status {
            button::Status::Hovered => Some(Background::Color(tokens::hover())),
            button::Status::Pressed => Some(Background::Color(tokens::pressed())),
            _ => None,
        };
        button::Style {
            background: bg,
            text_color: tokens::text(),
            border: Border {
                radius: RADIUS_CONTROL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    });

    kit::tooltip(
        btn,
        "Click to rename document",
        iced::widget::tooltip::Position::Bottom,
    )
}

fn save_state_chip<'a>(app: &'a App) -> Element<'a, Message> {
    let label = save_state_label(app);
    container(
        text(label)
            .font(BARLOW_MEDIUM)
            .size(TEXT_12)
            .color(tokens::muted()),
    )
    .padding([2, 8])
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::field())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn symbol_edit_strip<'a>(
    session: &'a crate::SymbolEditSession,
    d: Density,
    compact: bool,
) -> Element<'a, Message> {
    let locked_note = (!session.locked_entities.is_empty()).then(|| {
        let count = session.locked_entities.len();
        container(
            text(format!(
                "{count} non-editable {}",
                if count == 1 {
                    "entity locked"
                } else {
                    "entities locked"
                }
            ))
            .font(BARLOW_MEDIUM)
            .size(TEXT_11)
            .color(tokens::muted()),
        )
        .padding([2, 7])
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::field())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: 999.0.into(),
            },
            ..container::Style::default()
        })
    });

    let mut r = row![text(format!("Editing symbol: {}", session.symbol_name))
        .font(BARLOW_SEMIBOLD)
        .size(if compact { TEXT_12 } else { TEXT_14 })
        .color(tokens::text()),]
    .spacing(SPACE_2)
    .align_y(Alignment::Center);
    if !compact {
        if let Some(note) = locked_note {
            r = r.push(note);
        }
        r = r.push(
            text("Inch grid (1' = 12\")")
                .font(BARLOW_MEDIUM)
                .size(TEXT_11)
                .color(tokens::muted()),
        );
    }
    r = r
        .push(kit::text_button(
            "Cancel",
            Some(Message::CancelSymbolShape),
            d,
        ))
        .push(kit::primary_button(
            "Save shape",
            Some(Message::SaveSymbolShape),
            d,
        ));
    r.into()
}

fn page_tabs_strip<'a>(app: &'a App, d: Density, compact: bool) -> Element<'a, Message> {
    let count = app.pages.len();
    let current = app.pages.current.min(count.saturating_sub(1));
    let h = if matches!(d, Density::Touch) {
        36.0
    } else {
        28.0
    };

    let step = |icon: &'static str, label: &'static str, target: Option<usize>| {
        kit::tooltip(
            kit::icon_button(
                icon,
                false,
                target.map(|i| Message::PagesNav(NavMsg::Go(i))),
                d,
            ),
            label,
            iced::widget::tooltip::Position::Bottom,
        )
    };
    let prev = step("caret-left", "Previous page", current.checked_sub(1));
    let next = step(
        "caret-right",
        "Next page",
        (current + 1 < count).then_some(current + 1),
    );

    // One control for the current page: its name, "n / total", and a caret.
    // Press opens the page list; right-click or long-press opens the page menu.
    let name_w = if compact { 120.0 } else { 280.0 };
    let body = row![
        container(
            text(app.pages.page_name(current))
                .font(BARLOW_SEMIBOLD)
                .size(TEXT_12)
                .color(tokens::text())
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .max_width(name_w)
        .clip(true),
        text(format!("{} / {}", current + 1, count))
            .font(BARLOW_MEDIUM)
            .size(TEXT_11)
            .color(tokens::muted()),
        icons::mono("caret-down", 12.0, tokens::muted()),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let open = app.pages_nav_open;
    let switcher_btn = button(
        container(body)
            .height(Length::Fixed(h))
            .padding([0, 10])
            .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press(Message::PagesNav(NavMsg::Toggle))
    .style(move |_, status| {
        let fill = match status {
            button::Status::Hovered => tokens::hover(),
            button::Status::Pressed => tokens::pressed(),
            _ => tokens::field(),
        };
        button::Style {
            background: Some(Background::Color(fill)),
            text_color: tokens::text(),
            border: Border {
                color: if open {
                    tokens::accent()
                } else {
                    tokens::rule()
                },
                width: 1.0,
                radius: RADIUS_CONTROL.into(),
            },
            ..button::Style::default()
        }
    });
    let switcher = kit::long_press(
        mouse_area(switcher_btn).on_right_press(Message::Ui(UiMsg::TogglePageMenu(Some(current)))),
        Message::Ui(UiMsg::TogglePageMenu(Some(current))),
    );

    let page_menu_btn = kit::tooltip(
        kit::icon_button(
            "dots-three",
            app.ui.page_menu == Some(current),
            Some(Message::Ui(UiMsg::TogglePageMenu(Some(current)))),
            d,
        ),
        "Page options",
        iced::widget::tooltip::Position::Bottom,
    );

    let add_btn = kit::tooltip(
        kit::icon_button("plus", false, Some(Message::Status(StatusMsg::PageAdd)), d),
        "Add page",
        iced::widget::tooltip::Position::Bottom,
    );

    row![prev, switcher, next, page_menu_btn, add_btn]
        .spacing(SPACE_1)
        .align_y(Alignment::Center)
        .into()
}

fn undo_redo_controls<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let can_undo = app.undo.can_undo();
    let can_redo = app.undo.can_redo();

    let undo_btn = kit::long_press(
        mouse_area(kit::tooltip(
            kit::icon_button(
                "arrow-u-up-left",
                app.history.open.is_some(),
                can_undo.then_some(Message::Undo),
                d,
            ),
            "Undo (Ctrl+Z)",
            iced::widget::tooltip::Position::Bottom,
        ))
        .on_right_press(Message::UndoHistory),
        Message::UndoHistory,
    );

    let history_fg = if can_undo {
        tokens::text()
    } else {
        tokens::disabled()
    };
    let mut history_btn = button(
        container(icons::mono("caret-down", 10.0, history_fg))
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(d.min_target()))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .style(move |_, status| button::Style {
        background: match status {
            button::Status::Hovered if can_undo => Some(Background::Color(tokens::hover())),
            button::Status::Pressed if can_undo => Some(Background::Color(tokens::pressed())),
            _ => None,
        },
        text_color: history_fg,
        border: Border {
            radius: RADIUS_CONTROL.into(),
            ..Border::default()
        },
        ..button::Style::default()
    });
    if can_undo {
        history_btn = history_btn.on_press(Message::UndoHistory);
    }

    let history_tip = kit::tooltip(
        history_btn,
        "Undo history",
        iced::widget::tooltip::Position::Bottom,
    );

    let redo_btn = kit::tooltip(
        kit::icon_button(
            "arrow-u-up-right",
            false,
            can_redo.then_some(Message::Redo),
            d,
        ),
        "Redo (Ctrl+Y)",
        iced::widget::tooltip::Position::Bottom,
    );

    row![undo_btn, history_tip, redo_btn]
        .spacing(2)
        .align_y(Alignment::Center)
        .into()
}

fn search_trigger<'a>(class: SizeClass, d: Density) -> Element<'a, Message> {
    if !matches!(class, SizeClass::Wide) {
        return kit::tooltip(
            kit::icon_button(
                "magnifying-glass",
                false,
                Some(Message::Ui(UiMsg::ToggleMenu)),
                d,
            ),
            "Search commands (Ctrl+K)",
            iced::widget::tooltip::Position::Bottom,
        );
    }

    let h = if matches!(d, Density::Touch) {
        40.0
    } else {
        30.0
    };
    button(
        container(
            row![
                icons::mono("magnifying-glass", 14.0, tokens::muted()),
                text("Search commands")
                    .font(BARLOW)
                    .size(TEXT_12)
                    .color(tokens::muted())
                    .width(Length::Fill),
                container(
                    text("Ctrl K")
                        .font(BSC_MEDIUM)
                        .size(TEXT_11)
                        .color(tokens::muted()),
                )
                .padding([1, 5])
                .style(|_| container::Style {
                    background: Some(Background::Color(tokens::field())),
                    border: Border {
                        color: tokens::rule(),
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..container::Style::default()
                }),
            ]
            .spacing(SPACE_2)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::Fixed(204.0))
        .height(Length::Fixed(h))
        .padding([0, 8])
        .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press(Message::Ui(UiMsg::ToggleMenu))
    .style(|_, status| {
        let border_c = match status {
            button::Status::Hovered | button::Status::Pressed => tokens::accent(),
            _ => tokens::rule(),
        };
        button::Style {
            background: Some(Background::Color(tokens::input_bg())),
            text_color: tokens::text(),
            border: Border {
                color: border_c,
                width: 1.0,
                radius: RADIUS_CONTROL.into(),
            },
            ..button::Style::default()
        }
    })
    .into()
}

fn primary_actions<'a>(app: &'a App, class: SizeClass, d: Density) -> Vec<Element<'a, Message>> {
    if matches!(class, SizeClass::Compact) {
        return vec![kit::with_id(
            "export",
            kit::tooltip(
                kit::icon_button(
                    if cfg!(windows) && !app.ui.prefer_export_primary {
                        "arrow-square-in"
                    } else {
                        "export"
                    },
                    app.ui.export_menu,
                    Some(if cfg!(windows) && !app.ui.prefer_export_primary {
                        Message::Guide(guide::GuideMsg::Open(guide::INTEGRATION_TOPIC))
                    } else {
                        Message::Ui(UiMsg::ToggleExportMenu)
                    }),
                    d,
                ),
                if cfg!(windows) && !app.ui.prefer_export_primary {
                    "Send to report"
                } else {
                    "Export"
                },
                iced::widget::tooltip::Position::Bottom,
            ),
        )];
    }

    if cfg!(windows) && !app.ui.prefer_export_primary {
        let send_btn = kit::primary_button(
            "Send to report",
            Some(Message::Guide(guide::GuideMsg::Open(
                guide::INTEGRATION_TOPIC,
            ))),
            d,
        );
        let export_more = kit::with_id(
            "export",
            kit::tooltip(
                kit::icon_button(
                    "export",
                    app.ui.export_menu,
                    Some(Message::Ui(UiMsg::ToggleExportMenu)),
                    d,
                ),
                "Export (PNG / SVG / DXF / PDF)",
                iced::widget::tooltip::Position::Bottom,
            ),
        );
        vec![send_btn, export_more]
    } else {
        vec![kit::with_id(
            "export",
            kit::primary_button("Export", Some(Message::Ui(UiMsg::ToggleExportMenu)), d),
        )]
    }
}

fn window_controls<'a>(app: &'a App, compact: bool) -> Element<'a, Message> {
    let size = if compact {
        COMPACT_CAPTION_BTN
    } else {
        DESKTOP_CAPTION_BTN
    };
    let icon_sz = if compact { 12.0 } else { 14.0 };
    let caption_btn =
        |icon_name: &str, msg: Message, hover_bg: Color, icon_color: Color, tip: &'static str| {
            let b = button(
                container(icons::mono(icon_name, icon_sz, icon_color))
                    .width(Length::Fixed(size))
                    .height(Length::Fixed(size))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .on_press(msg)
            .style(move |_, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(hover_bg)),
                    button::Status::Pressed => {
                        Some(Background::Color(over(Color::BLACK, 0.14, hover_bg)))
                    }
                    _ => None,
                },
                border: Border {
                    radius: RADIUS_CONTROL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            });
            kit::tooltip(b, tip, iced::widget::tooltip::Position::Bottom)
        };

    let close_fg = if app.close_hover {
        Color::WHITE
    } else {
        tokens::text()
    };
    let close_btn = mouse_area(caption_btn(
        "x",
        Message::Close,
        CLOSE_HOVER,
        close_fg,
        "Close",
    ))
    .on_enter(Message::CloseHover(true))
    .on_exit(Message::CloseHover(false));

    row![
        caption_btn(
            "minus",
            Message::Minimize,
            tokens::hover(),
            tokens::text(),
            "Minimize"
        ),
        caption_btn(
            if app.maximized { "copy" } else { "square" },
            Message::ToggleMaximize,
            tokens::hover(),
            tokens::text(),
            if app.maximized { "Restore" } else { "Maximize" },
        ),
        close_btn,
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

fn totals_chip<'a>(app: &'a App, compact: bool) -> Element<'a, Message> {
    let model = calcs::build(&app.drawing.polygons, &app.computed);
    let total: f64 = model.groups.iter().map(|g| g.total).sum();
    let unit = if app.drawing.options.units.metric {
        "m²"
    } else {
        app.drawing.options.units.area_label_suffix.as_str()
    };
    let label = if total > 0.05 {
        format!("{:.0} {}", total, unit)
    } else if compact {
        "Totals".to_owned()
    } else {
        format!("Totals: 0 {}", unit)
    };
    kit::chip(
        label,
        false,
        Some(Message::Ui(UiMsg::ToggleLedger)),
        app.ui.density,
    )
}

// ---------------------------------------------------------------------------
// Top bar overlays (tab menu, Export menu, All pages navigator)
// ---------------------------------------------------------------------------

fn page_menu_overlay<'a>(app: &'a App, index: usize, d: Density) -> Element<'a, Message> {
    let len = app.pages.len();
    let current = index == app.pages.current;
    let is_underlay = app.pages.underlay == Some(index);
    let underlay_label = if current && app.pages.underlay.is_some() {
        "Clear underlay"
    } else {
        "Use as underlay"
    };
    let underlay_msg = if current && app.pages.underlay.is_some() {
        Some(Message::PagesNav(NavMsg::Underlay(None)))
    } else if !current {
        Some(Message::PagesNav(NavMsg::Ghost(index)))
    } else {
        None
    };

    let entries = vec![
        MenuEntry::Item {
            icon: Some("pencil-simple"),
            label: "Rename page",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::PromptRenamePage(index))),
        },
        MenuEntry::Item {
            icon: Some("copy-simple"),
            label: underlay_label,
            shortcut: None,
            checked: is_underlay,
            focused: false,
            on_press: underlay_msg,
        },
        MenuEntry::Item {
            icon: Some("arrow-line-up"),
            label: "Move first",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: (index > 0).then_some(Message::Ui(UiMsg::PageMoveFirst(index))),
        },
        MenuEntry::Item {
            icon: Some("arrow-line-down"),
            label: "Move last",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: (index + 1 < len).then_some(Message::Ui(UiMsg::PageMoveLast(index))),
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: Some("files"),
            label: "All pages",
            shortcut: None,
            checked: app.pages_nav_open,
            focused: false,
            on_press: Some(Message::PagesNav(NavMsg::Toggle)),
        },
        MenuEntry::Item {
            icon: Some("trash"),
            label: "Delete page",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: (len > 1).then_some(Message::PagesNav(NavMsg::Delete(index))),
        },
    ];

    let title = app.pages.page_name(index);
    let header = container(
        text(title)
            .font(BSC_MEDIUM)
            .size(TEXT_11)
            .color(tokens::muted()),
    )
    .width(Length::Fixed(212.0))
    .padding([6, 10])
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::panel())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        shadow: tokens::floating_shadow(),
        ..container::Style::default()
    });
    let card = column![header, kit::menu(entries, 212.0, d)]
        .width(Length::Fixed(212.0))
        .spacing(4);

    let top_pad = match SizeClass::of(app.window_size.width) {
        SizeClass::Wide => d.bar_h() + 4.0,
        SizeClass::Medium | SizeClass::Compact => d.bar_h() + PAGE_BAR_H + 4.0,
    };

    let positioned = container(opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: top_pad,
            left: SPACE_3,
            right: SPACE_3,
            bottom: SPACE_3,
        })
        .align_x(
            if matches!(SizeClass::of(app.window_size.width), SizeClass::Wide) {
                Alignment::Center
            } else {
                Alignment::Start
            },
        );

    opaque(mouse_area(positioned).on_press(Message::Ui(UiMsg::CloseOverlays)))
}

fn export_menu_overlay<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let mut entries = vec![
        MenuEntry::Item {
            icon: Some("file-image"),
            label: "Export PNG",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::ExportPng),
        },
        MenuEntry::Item {
            icon: Some("bezier-curve"),
            label: "Export SVG",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::ExportSvg),
        },
        MenuEntry::Item {
            icon: Some("shapes"),
            label: "Export DXF",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::ExportDxf),
        },
        MenuEntry::Item {
            icon: Some("file"),
            label: "Export PDF",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::ExportPdf),
        },
    ];

    if cfg!(windows) {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::Item {
            icon: Some("arrow-square-in"),
            label: "Send to report",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::Guide(guide::GuideMsg::Open(
                guide::INTEGRATION_TOPIC,
            ))),
        });
        entries.push(MenuEntry::Item {
            icon: Some("sliders"),
            label: "Prefer Export as primary",
            shortcut: None,
            checked: app.ui.prefer_export_primary,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::TogglePreferExportPrimary)),
        });
    }

    let positioned = container(opaque(kit::menu(entries, 228.0, d)))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: d.bar_h() + 4.0,
            right: SPACE_3,
            left: SPACE_3,
            bottom: SPACE_3,
        })
        .align_x(Alignment::End);

    opaque(mouse_area(positioned).on_press(Message::Ui(UiMsg::CloseOverlays)))
}

fn pages_nav_overlay<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let max_h =
        (app.window_size.height - d.bar_h() - PAGE_BAR_H - SPACE_3 * 2.0).clamp(220.0, 480.0);
    let panel = container(pages_nav::view(&app.pages, app.pages_nav_hover))
        .height(Length::Fixed(max_h))
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::panel())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_DIALOG.into(),
            },
            shadow: tokens::floating_shadow(),
            ..container::Style::default()
        });

    let top_pad = match SizeClass::of(app.window_size.width) {
        SizeClass::Wide => d.bar_h() + 4.0,
        SizeClass::Medium | SizeClass::Compact => d.bar_h() + PAGE_BAR_H + 4.0,
    };

    let positioned = container(opaque(panel))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: top_pad,
            left: SPACE_3,
            right: SPACE_3,
            bottom: SPACE_3,
        })
        .align_x(
            if matches!(SizeClass::of(app.window_size.width), SizeClass::Wide) {
                Alignment::Center
            } else {
                Alignment::Start
            },
        )
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::scrim())),
            ..container::Style::default()
        });

    opaque(mouse_area(positioned).on_press(Message::PagesNav(NavMsg::Close)))
}
