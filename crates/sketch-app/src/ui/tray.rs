//! Floating tool tray (`Wide`/`Medium`), bottom scrollable tool bar (`Compact`), and tool fly-outs for `Shell::Next`.

use iced::widget::{
    button, column, container, mouse_area, opaque, row, scrollable, text, tooltip::Position, Stack,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};
use sketch_core::model::WallType;

use crate::drawing::WallShape;
use crate::icons;
use crate::shapes::ShapesMsg;
use crate::statusbar::StatusMsg;
use crate::theme::over;
use crate::ui::fonts::{BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_SEMIBOLD};
use crate::ui::kit::{self, MenuEntry};
use crate::ui::layout::{self, DrawingChrome, SizeClass, CHROME_INSET, TRAY_GAP, TRAY_PAD};
use crate::ui::tokens::{
    self, Density, RADIUS_CONTROL, RADIUS_SHEET, SPACE_1, SPACE_2, SPACE_3, TEXT_11, TEXT_12,
};
use crate::ui::Msg as UiMsg;
use crate::view_state::{Mode, RailTool};
use crate::{App, Message};

/// Drawing and editing tools shown in the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayTool {
    Select,
    Wall,
    Inner,
    Freeform,
    Erase,
    Area,
    Shape,
    Curve,
    Measure,
    Label,
    Insert,
    Pan,
}

/// Active tab of the Insert drawer (`02-architecture.md` `DrawerMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerTab {
    Symbols,
    Labels,
}

impl TrayTool {
    pub const ALL: [TrayTool; 12] = [
        TrayTool::Select,
        TrayTool::Wall,
        TrayTool::Inner,
        TrayTool::Area,
        TrayTool::Freeform,
        TrayTool::Erase,
        TrayTool::Shape,
        TrayTool::Curve,
        TrayTool::Measure,
        TrayTool::Label,
        TrayTool::Insert,
        TrayTool::Pan,
    ];

    /// Stable iced widget id for agents (`#tool-wall`, …).
    pub fn mcp_id(self) -> &'static str {
        match self {
            TrayTool::Select => "tool-select",
            TrayTool::Wall => "tool-wall",
            TrayTool::Inner => "tool-inner",
            TrayTool::Area => "tool-area",
            TrayTool::Freeform => "tool-freeform",
            TrayTool::Erase => "tool-erase",
            TrayTool::Shape => "tool-shape",
            TrayTool::Curve => "tool-curve",
            TrayTool::Measure => "tool-measure",
            TrayTool::Label => "tool-label",
            TrayTool::Insert => "tool-insert",
            TrayTool::Pan => "tool-pan",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            TrayTool::Select => "Select",
            TrayTool::Wall => "Wall",
            TrayTool::Inner => "Inner",
            TrayTool::Freeform => "Freeform",
            TrayTool::Erase => "Erase",
            TrayTool::Area => "Area",
            TrayTool::Shape => "Shape",
            TrayTool::Curve => "Curve",
            TrayTool::Measure => "Measure",
            TrayTool::Label => "Label",
            TrayTool::Insert => "Insert",
            TrayTool::Pan => "Pan",
        }
    }

    pub const fn icon(self) -> &'static str {
        match self {
            TrayTool::Select => "cursor",
            TrayTool::Wall => "polygon",
            TrayTool::Inner => "square-split-vertical",
            TrayTool::Freeform => "pencil-simple",
            TrayTool::Erase => "eraser",
            TrayTool::Area => "bounding-box",
            TrayTool::Shape => "square",
            TrayTool::Curve => "bezier-curve",
            TrayTool::Measure => "ruler",
            TrayTool::Label => "text-t",
            TrayTool::Insert => "couch",
            TrayTool::Pan => "arrows-out-cardinal",
        }
    }

    pub const fn has_flyout(self) -> bool {
        matches!(
            self,
            TrayTool::Select
                | TrayTool::Wall
                | TrayTool::Inner
                | TrayTool::Curve
                | TrayTool::Measure
                | TrayTool::Pan
        )
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            TrayTool::Select => {
                "Select and edit (P) — click active or right-click for Fence / Lasso"
            }
            TrayTool::Wall => "Draw exterior walls (E) — click active or right-click for shapes",
            TrayTool::Inner => "Draw interior lines (I) — click active or right-click for shapes",
            TrayTool::Freeform => "Freeform draw: drag to draw without snapping",
            TrayTool::Erase => {
                "Erase: click or drag across lines, labels, or symbols (Ctrl+Z to undo)"
            }
            TrayTool::Area => "Area tool: click inside an outline",
            TrayTool::Shape => "Insert a room shape (M)",
            TrayTool::Curve => "Curve wall — click active or right-click for Insert vertex",
            TrayTool::Measure => "Measure runs — click active or right-click for Chain dimension",
            TrayTool::Label => "Open Insert drawer on Room labels (T)",
            TrayTool::Insert => "Open Insert drawer on Symbols (S)",
            TrayTool::Pan => {
                "Drag to pan (Space) — click active or right-click for Zoom to rectangle"
            }
        }
    }

    pub fn primary_message(self) -> Message {
        match self {
            TrayTool::Select => Message::Rail(RailTool::Select),
            TrayTool::Wall => Message::Rail(RailTool::DrawExterior),
            TrayTool::Inner => Message::Rail(RailTool::DrawInterior),
            TrayTool::Freeform => Message::Rail(RailTool::Freeform),
            TrayTool::Erase => Message::Rail(RailTool::Erase),
            TrayTool::Area => Message::Rail(RailTool::DefineArea),
            TrayTool::Shape => Message::Rail(RailTool::Shapes),
            TrayTool::Curve => Message::Rail(RailTool::Curve),
            TrayTool::Measure => Message::Rail(RailTool::Measure),
            TrayTool::Label => Message::Rail(RailTool::Text),
            TrayTool::Insert => Message::Rail(RailTool::Symbols),
            TrayTool::Pan => Message::Rail(RailTool::Pan),
        }
    }

    pub fn is_active(self, app: &App) -> bool {
        let mode = app.view.mode;
        match self {
            TrayTool::Select => matches!(mode, Mode::Select | Mode::Fence | Mode::Lasso),
            TrayTool::Wall => mode == Mode::DrawExterior,
            TrayTool::Inner => mode == Mode::DrawInterior,
            TrayTool::Freeform => mode == Mode::Freeform,
            TrayTool::Erase => mode == Mode::Erase,
            TrayTool::Area => mode == Mode::DefineArea,
            TrayTool::Shape => mode == Mode::PlaceShape || app.room_shapes.open,
            TrayTool::Curve => matches!(mode, Mode::Curve | Mode::InsertVertex),
            TrayTool::Measure => matches!(mode, Mode::Measure | Mode::ChainDim),
            TrayTool::Label => mode == Mode::PlaceText || app.ui.drawer == Some(DrawerTab::Labels),
            TrayTool::Insert => {
                mode == Mode::PlaceSymbol || app.ui.drawer == Some(DrawerTab::Symbols)
            }
            TrayTool::Pan => matches!(mode, Mode::Pan | Mode::ZoomFence),
        }
    }
}

/// Tray card placed in the same dock row as the drawing controls. Its width
/// includes the surface padding and is bounded by the actual canvas width.
pub fn dock_card<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let chrome = DrawingChrome::for_app(app);
    let mut strip = row![].spacing(TRAY_GAP).align_y(Alignment::Center);
    for tool in TrayTool::ALL {
        strip = strip.push(tray_tile(app, tool, d));
    }

    let scroll = scrollable(strip)
        .id("drawing-tool-tray")
        .on_scroll(|viewport| Message::Ui(UiMsg::TrayScrolled(viewport.absolute_offset().x)))
        .width(Length::Fixed((chrome.tray_width - TRAY_PAD * 2.0).max(1.0)))
        .height(Length::Fixed(d.tray_button().1))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ));
    kit::tray_surface(scroll)
}

/// Bottom horizontally scrollable tool bar for `Compact` (`< 700px`).
pub fn compact_bar<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let mut strip = row![].spacing(SPACE_1).align_y(Alignment::Center);
    for tool in TrayTool::ALL {
        strip = strip.push(compact_pill(app, tool, d));
    }

    let scroll = scrollable(strip)
        .id("drawing-compact-tray")
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ))
        .width(Length::Fill);

    container(scroll)
        .height(Length::Fixed(d.bar_h()))
        .padding([0.0, SPACE_2])
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::bar())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: iced::border::Radius {
                    top_left: RADIUS_SHEET,
                    top_right: RADIUS_SHEET,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
            },
            ..container::Style::default()
        })
        .into()
}

/// Pop-up overlay for open tray fly-outs.
pub fn layer<'a>(app: &'a App) -> Option<Element<'a, Message>> {
    if let Some(tool) = app.ui.tray_flyout.filter(|t| t.has_flyout()) {
        return Some(flyout_overlay(app, tool));
    }
    None
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

fn tray_tile<'a>(app: &'a App, tool: TrayTool, d: Density) -> Element<'a, Message> {
    let active = tool.is_active(app);
    let flyout_open = app.ui.tray_flyout == Some(tool);
    let has_fly = tool.has_flyout();

    let on_press = if active && has_fly {
        Message::Ui(UiMsg::ToggleTrayFlyout(Some(tool)))
    } else {
        tool.primary_message()
    };

    let tile = kit::tile(
        tool.icon(),
        tool.label(),
        active || flyout_open,
        false,
        Some(on_press),
        d,
    );

    let with_mark: Element<'a, Message> = if has_fly {
        let mark_color = if active || flyout_open {
            tokens::on_accent()
        } else {
            tokens::muted()
        };
        let badge = container(icons::mono("caret-up", 9.0, mark_color))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([3, 4])
            .align_x(Alignment::End)
            .align_y(iced::alignment::Vertical::Top);
        let (w, h) = d.tray_button();
        Stack::with_children([tile, badge.into()])
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .into()
    } else {
        tile
    };

    let interactive: Element<'a, Message> = if has_fly {
        kit::context_press(with_mark, Message::Ui(UiMsg::ToggleTrayFlyout(Some(tool))))
    } else {
        kit::tooltip(with_mark, tool.tooltip(), Position::Top)
    };

    kit::with_id(tool.mcp_id(), interactive)
}

fn compact_pill<'a>(app: &'a App, tool: TrayTool, d: Density) -> Element<'a, Message> {
    let active = tool.is_active(app);
    let flyout_open = app.ui.tray_flyout == Some(tool);
    let highlighted = active || flyout_open;
    let has_fly = tool.has_flyout();

    let on_press = if active && has_fly {
        Message::Ui(UiMsg::ToggleTrayFlyout(Some(tool)))
    } else {
        tool.primary_message()
    };

    let fg = if highlighted {
        tokens::on_accent()
    } else {
        tokens::text()
    };
    let bg = if highlighted {
        tokens::accent()
    } else {
        tokens::field()
    };
    let hover_bg = if highlighted {
        over(Color::WHITE, 0.10, tokens::accent())
    } else {
        tokens::hover()
    };
    let h = if matches!(d, Density::Touch) {
        40.0
    } else {
        32.0
    };

    let mut content = row![
        if highlighted {
            icons::mono(tool.icon(), 15.0, fg)
        } else {
            icons::tinted(tool.icon(), false, 15.0, fg)
        },
        text(tool.label())
            .font(if highlighted {
                BARLOW_SEMIBOLD
            } else {
                BARLOW_MEDIUM
            })
            .size(TEXT_12)
            .color(fg),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    if has_fly {
        content = content.push(icons::mono(
            "caret-up",
            9.0,
            if highlighted {
                tokens::on_accent()
            } else {
                tokens::muted()
            },
        ));
    }

    let btn = button(
        container(content)
            .height(Length::Fixed(h))
            .padding([0, 10])
            .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press(on_press)
    .style(move |_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => hover_bg,
            button::Status::Pressed => tokens::pressed(),
            _ => bg,
        })),
        text_color: fg,
        border: Border {
            color: if highlighted {
                tokens::accent()
            } else {
                tokens::rule()
            },
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..button::Style::default()
    });

    let interactive: Element<'a, Message> = if has_fly {
        kit::context_press(btn, Message::Ui(UiMsg::ToggleTrayFlyout(Some(tool))))
    } else {
        kit::tooltip(btn, tool.tooltip(), Position::Top)
    };

    kit::with_id(tool.mcp_id(), interactive)
}

// ---------------------------------------------------------------------------
// Fly-out menu
// ---------------------------------------------------------------------------

fn flyout_entries(app: &App, tool: TrayTool) -> Vec<MenuEntry<'static>> {
    let mode = app.view.mode;
    match tool {
        TrayTool::Select => vec![
            MenuEntry::Item {
                icon: Some("cursor"),
                label: "Select",
                shortcut: Some("P"),
                checked: mode == Mode::Select,
                focused: false,
                on_press: Some(Message::Rail(RailTool::Select)),
            },
            MenuEntry::Item {
                icon: Some("selection-background"),
                label: "Fence select",
                shortcut: None,
                checked: mode == Mode::Fence,
                focused: false,
                on_press: Some(Message::Rail(RailTool::Fence)),
            },
            MenuEntry::Item {
                icon: Some("lasso"),
                label: "Lasso select",
                shortcut: None,
                checked: mode == Mode::Lasso,
                focused: false,
                on_press: Some(Message::Rail(RailTool::Lasso)),
            },
            MenuEntry::Separator,
            MenuEntry::Item {
                icon: Some("selection-all"),
                label: "Select more",
                shortcut: None,
                checked: app.ui.select_more,
                focused: false,
                on_press: Some(Message::Ui(UiMsg::ToggleSelectMore)),
            },
        ],
        TrayTool::Wall => wall_shape_entries(WallType::Exterior, mode == Mode::DrawExterior),
        TrayTool::Inner => wall_shape_entries(WallType::Interior, mode == Mode::DrawInterior),
        TrayTool::Curve => vec![
            MenuEntry::Item {
                icon: Some("bezier-curve"),
                label: "Curve wall",
                shortcut: None,
                checked: mode == Mode::Curve,
                focused: false,
                on_press: Some(Message::Rail(RailTool::Curve)),
            },
            MenuEntry::Item {
                icon: Some("line-segments"),
                label: "Insert vertex",
                shortcut: Some("Ins"),
                checked: mode == Mode::InsertVertex,
                focused: false,
                on_press: Some(Message::Rail(RailTool::InsertVertex)),
            },
        ],
        TrayTool::Measure => vec![
            MenuEntry::Item {
                icon: Some("ruler"),
                label: "Measure runs",
                shortcut: None,
                checked: mode == Mode::Measure,
                focused: false,
                on_press: Some(Message::Rail(RailTool::Measure)),
            },
            MenuEntry::Item {
                icon: Some("arrows-horizontal"),
                label: "Chain dimension",
                shortcut: None,
                checked: mode == Mode::ChainDim,
                focused: false,
                on_press: Some(Message::Rail(RailTool::ChainDim)),
            },
        ],
        TrayTool::Pan => vec![
            MenuEntry::Item {
                icon: Some("arrows-out-cardinal"),
                label: "Pan",
                shortcut: Some("Space"),
                checked: mode == Mode::Pan,
                focused: false,
                on_press: Some(Message::Rail(RailTool::Pan)),
            },
            MenuEntry::Item {
                icon: Some("corners-out"),
                label: "Zoom to rectangle",
                shortcut: None,
                checked: mode == Mode::ZoomFence,
                focused: false,
                on_press: Some(Message::Status(StatusMsg::ZoomFence)),
            },
        ],
        _ => Vec::new(),
    }
}

fn wall_shape_entries(kind: WallType, line_checked: bool) -> Vec<MenuEntry<'static>> {
    vec![
        MenuEntry::Item {
            icon: Some("line-segment"),
            label: "Line",
            shortcut: if kind == WallType::Exterior {
                Some("E")
            } else {
                Some("I")
            },
            checked: line_checked,
            focused: false,
            on_press: Some(Message::Shapes(ShapesMsg::Line(kind))),
        },
        MenuEntry::Item {
            icon: Some("rectangle"),
            label: "Rectangle",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::TrayWallShape(
                kind,
                WallShape::Rectangle,
            ))),
        },
        MenuEntry::Item {
            icon: Some("circle"),
            label: "Circle",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::TrayWallShape(kind, WallShape::Circle))),
        },
        MenuEntry::Item {
            icon: Some("polygon"),
            label: "Polygon",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::TrayWallShape(kind, WallShape::Polygon))),
        },
    ]
}

fn flyout_overlay<'a>(app: &'a App, tool: TrayTool) -> Element<'a, Message> {
    let d = app.ui.density;
    let class = SizeClass::of(app.window_size.width);
    let entries = flyout_entries(app, tool);
    let width = 224.0;

    let header = container(
        text(tool.label())
            .font(BSC_SEMIBOLD)
            .size(TEXT_11)
            .color(tokens::muted()),
    )
    .width(Length::Fixed(width))
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

    let card = column![header, kit::menu(entries, width, d)]
        .width(Length::Fixed(width))
        .spacing(4);

    let bottom_pad = match class {
        SizeClass::Compact => d.bar_h() + 8.0,
        SizeClass::Wide | SizeClass::Medium => DrawingChrome::for_app(app).tray_clearance(),
    };

    let canvas = layout::canvas_rect(app);
    let chrome = DrawingChrome::for_app(app);
    let index = TrayTool::ALL.iter().position(|t| *t == tool).unwrap_or(0) as f32;
    let natural =
        TrayTool::ALL.len() as f32 * (d.tray_button().0 + TRAY_GAP) - TRAY_GAP + TRAY_PAD * 2.0;
    let scroll_x = app
        .ui
        .tray_scroll_x
        .clamp(0.0, (natural - chrome.tray_width).max(0.0));
    let tool_x = canvas.x
        + (canvas.width - chrome.tray_width) / 2.0
        + TRAY_PAD
        + index * (d.tray_button().0 + TRAY_GAP)
        + d.tray_button().0 / 2.0
        - scroll_x;
    let center = if class == SizeClass::Compact {
        canvas.x + canvas.width / 2.0
    } else {
        tool_x
    };
    let left = (center - width / 2.0).clamp(
        canvas.x + CHROME_INSET,
        (canvas.x + canvas.width - width - CHROME_INSET).max(canvas.x + CHROME_INSET),
    );

    let positioned = container(opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: SPACE_3,
            left,
            right: SPACE_3,
            bottom: bottom_pad,
        })
        .align_x(Alignment::Start)
        .align_y(iced::alignment::Vertical::Bottom);

    opaque(mouse_area(positioned).on_press(Message::Ui(UiMsg::ToggleTrayFlyout(None))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_tools_expose_stable_mcp_ids() {
        assert_eq!(TrayTool::Select.mcp_id(), "tool-select");
        assert_eq!(TrayTool::Wall.mcp_id(), "tool-wall");
        assert_eq!(TrayTool::Pan.mcp_id(), "tool-pan");
        let ids: Vec<_> = TrayTool::ALL.iter().map(|t| t.mcp_id()).collect();
        assert_eq!(ids.len(), 12);
        assert!(ids.iter().all(|id| id.starts_with("tool-")));
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "mcp ids must be unique");
    }
}
