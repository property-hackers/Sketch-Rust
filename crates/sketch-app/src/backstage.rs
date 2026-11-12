//! The File backstage (`#file-menu`): a 200px `--bg-panel` column from
//! y=54 to the bottom of the window, over the tab strip's last 4px, the left
//! rail, the canvas and the status bar. Positions are from
//! `refs/ui-spec/chrome-spec-file-backstage.json`; styles from theme.css
//! (`.file-menu*`, `.file-btn`). Opening/closing follows ui-manager.js: the
//! File tab toggles it; a pointer press outside it (and off the File tab)
//! or Escape closes it.
//!
//! Menu items invoke the same native handlers as shortcuts and command search.

use iced::widget::{button, column, container, mouse_area, row, scrollable, stack, Space};
use iced::{mouse, Background, Border, Color, Element, Length};

use crate::icons::icon;
use crate::options_dialog;
use crate::ribbon::{g, Glyph};
use crate::theme::*;
use crate::Message;
/// `.file-menu { top: 54px; width: 200px }`
pub const MENU_Y: f32 = 54.0;
pub const MENU_W: f32 = 200.0;
/// `.file-menu-item` height (8px padding + 20px icon + 8px padding).
const ITEM_H: f32 = 36.0;
/// `.file-menu-footer` height (8px padding + 24px button + 8px + 1px rim).
const FOOTER_H: f32 = 41.0;

/// One `.file-menu-item`: top y (page), main 20px glyph, label, extra
/// glyphs (the overlaid badge / submenu caret).
struct MenuItem {
    y: f32,
    glyph: Glyph,
    text: &'static str,
    extra: Option<Glyph>,
}

const fn item(
    y: f32,
    name: &'static str,
    fill: bool,
    color: Color,
    text: &'static str,
) -> MenuItem {
    MenuItem {
        y,
        // `.file-menu-item i { font-size: 20px }` at padding-left 15px.
        glyph: g(name, fill, 20.0, color, 15.0, y + 8.0),
        text,
        extra: None,
    }
}

const YELLOW: Color = hex(0xf6e05e);
const GREEN: Color = hex(0x5dae84);
const PURPLE: Color = hex(0xb794f4);
const MINT: Color = hex(0x68d391);

fn items() -> Vec<MenuItem> {
    vec![
        item(64.0, "file", false, YELLOW, "New"),
        item(100.0, "folder-open", true, ICON_ORANGE, "Open..."),
        item(136.0, "floppy-disk", true, ICON_BLUE, "Save"),
        MenuItem {
            // pencil badge: `right:-4px; bottom:-4px; font-size:12px`.
            extra: Some(g("pencil-simple", true, 12.0, ICON_ORANGE, 27.0, 192.0)),
            ..item(172.0, "floppy-disk", true, GREEN, "Save As...")
        },
        item(
            208.0,
            "list-numbers",
            false,
            ICON_BLUE,
            "Version History...",
        ),
        item(255.0, "arrow-square-in", true, ICON_ORANGE, "Import..."),
        item(291.0, "blueprint", true, PURPLE, "Import CAD..."),
        item(327.0, "arrow-square-out", true, ICON_BLUE, "Export as PNG"),
        item(363.0, "vector-two", true, MINT, "Export as SVG"),
        item(399.0, "blueprint", true, PURPLE, "Export as DXF"),
        item(435.0, "file-pdf", true, ICON_RED, "Export as PDF"),
        item(482.0, "envelope-simple", true, ICON_ORANGE, "Email..."),
        MenuItem {
            // `margin-left:auto; font-size:14px` caret.
            extra: Some(g("caret-right", false, 14.0, text_main(), 171.0, 540.0)),
            ..item(
                529.0,
                "clock-counter-clockwise",
                true,
                ICON_BLUE,
                "Recent Files",
            )
        },
        item(576.0, "printer", true, ICON_SLATE, "Print..."),
        MenuItem {
            extra: Some(g("eye", true, 12.0, ICON_ORANGE, 27.0, 632.0)),
            ..item(612.0, "printer", true, ICON_SLATE, "Print Preview...")
        },
        item(648.0, "layout", false, ICON_BLUE, "Page Layout"),
        item(684.0, "folder-open", true, ICON_BLUE, "Import Layer..."),
        item(
            720.0,
            "clock-counter-clockwise",
            false,
            ICON_BLUE,
            "Recover Autosave...",
        ),
    ]
}

/// `.file-menu-divider` tops (page y).
const DIVIDERS: [f32; 4] = [249.0, 476.0, 523.0, 570.0];

fn glyph_at<'a>(gl: &Glyph, ox: f32, oy: f32) -> Element<'a, Message> {
    at(
        gl.x - ox,
        gl.y - oy,
        icon(gl.name, gl.fill, gl.size, gl.color),
    )
}

/// Whether an item can be used right now. Only Version History depends on
/// the open file (it needs a `.sketchy` store).
fn item_enabled(text: &str, history_enabled: bool) -> bool {
    text != "Version History..." || history_enabled
}

fn menu_item<'a>(it: &MenuItem, enabled: bool) -> Element<'a, Message> {
    let dim = over(text_main(), 0.38, bg_panel());
    let mut glyph = it.glyph;
    let text_color = if enabled { text_main() } else { dim };
    if !enabled {
        glyph.color = dim;
    }
    let mut body = stack![
        glyph_at(&glyph, 0.0, it.y),
        // Span: x=45 (15 + 20 + gap 10), y = top + 11, 13px, 15px line.
        at(45.0, 11.0, label(it.text, 13.0, ARIAL, text_color, 15.0)),
    ]
    .width(MENU_W)
    .height(ITEM_H);
    if let Some(extra) = &it.extra {
        body = body.push(glyph_at(extra, 0.0, it.y));
    }
    button(body)
        .width(MENU_W)
        .height(ITEM_H)
        .padding(0)
        .on_press_maybe(enabled.then(|| item_action(it.text)))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed if enabled => {
                    Some(bg_hover().into())
                }
                _ => None,
            },
            ..button::Style::default()
        })
        .into()
}

/// `UIManager.bindFileMenuItems`: route every displayed item to its native handler.
fn item_action(text: &str) -> crate::Message {
    match text {
        "Import Layer..." => Message::ImportLayer,
        "Recover Autosave..." => Message::RecoveryOpen,
        "New" => crate::Message::New,
        "Open..." => Message::OpenFile,
        "Save" => Message::SaveFile,
        "Save As..." => Message::SaveFileAs,
        "Version History..." => Message::Versions(crate::versions::VersionsMsg::Show),
        "Import..." => crate::Message::ImportSketch,
        "Email..." => crate::Message::Email,
        "Print..." => crate::Message::Print,
        "Print Preview..." => crate::Message::PrintPreviewOpen,
        "Export as PNG" => crate::Message::ExportPng,
        "Export as SVG" => crate::Message::ExportSvg,
        "Export as DXF" => crate::Message::ExportDxf,
        "Export as PDF" => crate::Message::ExportPdf,
        "Import CAD..." => crate::Message::ImportCad,
        "Page Layout" => crate::Message::PageLayout(crate::page_layout::LayoutMsg::Open),
        // The Recent Files item toggles the submenu (ui-manager.js:1010).
        "Recent Files" => crate::Message::RecentToggle,
        _ => crate::Message::Noop,
    }
}

/// `.file-btn` — 24px tall, 1px `--border`, radius 2, padding 4px 10px,
/// 12px icon + gap 6 + 12px label; hover `rgba(255,255,255,.1)`.
fn file_btn<'a>(
    w: f32,
    name: &'static str,
    color: Color,
    text: &'static str,
    msg: Message,
) -> Element<'a, Message> {
    let body = stack![
        at(11.0, 6.0, icon(name, true, 12.0, color)),
        at(29.0, 5.0, label(text, 12.0, ARIAL, text_main(), 14.0)),
    ]
    .width(w)
    .height(24);
    button(body)
        .width(w)
        .height(24)
        .padding(0)
        .on_press(msg)
        .style(|_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(over(WHITE, 0.1, bg_panel()).into())
                }
                _ => None,
            },
            border: Border {
                color: border(),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..button::Style::default()
        })
        .into()
}

/// The backstage menu column itself (200 wide, fills its height).
fn menu<'a>(connected: Option<&str>, history_enabled: bool) -> Element<'a, Message> {
    let mut list = stack![].width(MENU_W).height(Length::Fill);
    for it in items() {
        let enabled = item_enabled(it.text, history_enabled);
        list = list.push(at(0.0, it.y - MENU_Y, menu_item(&it, enabled)));
    }
    for y in DIVIDERS {
        list = list.push(at(0.0, y - MENU_Y, rect(MENU_W, 1.0, border())));
    }
    let mut footer = column![
        rect(MENU_W, 1.0, border()),
        stack![
            at(
                10.0,
                8.0,
                file_btn(
                    81.0,
                    "gear",
                    ICON_BLUE,
                    "Options",
                    Message::Options(options_dialog::OptMsg::Open)
                )
            ),
            at(
                99.0,
                8.0,
                file_btn(60.0, "power", ICON_RED, "Exit", Message::Close)
            ),
        ]
        .width(MENU_W)
        .height(FOOTER_H - 1.0),
    ];
    if let Some(target) = connected {
        footer = column![
            crate::tooltips::tip_below(
                button(label("Return to host", 12.0, SEGOE, text_main(), 16.0))
                    .padding([10, 8])
                    .width(MENU_W)
                    .on_press(Message::ApexReturn),
                format!("Return to the originating report: {target}")
            ),
            footer
        ];
    }
    let panel = fill_bg(column![list, footer], bg_panel())
        .width(MENU_W)
        .height(Length::Fill);
    // Swallow presses on the menu's empty areas and keep the layers below
    // (rail, canvas, status bar) from seeing the cursor.
    mouse_area(panel)
        .on_press(Message::Noop)
        .interaction(mouse::Interaction::Idle)
        .into()
}

/// The overlay layer: transparent except for the menu at (0, 54).
pub fn layer<'a>(
    recent_open: bool,
    recent: &[crate::recent::RecentFile],
    history_enabled: bool,
    connected: Option<&str>,
) -> Element<'a, Message> {
    let mut columns = row![container(menu(connected, history_enabled))
        .width(MENU_W)
        .height(Length::Fill)];
    if recent_open {
        columns = columns.push(submenu(recent));
    }
    column![
        Space::new().width(Length::Fill).height(MENU_Y),
        columns.width(Length::Fill).height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// `.file-menu-submenu`: 300px at the menu's right edge, `--bg-app`, a 1px
/// left border, full height; the header + the list (or the empty note).
fn submenu<'a>(recent: &[crate::recent::RecentFile]) -> Element<'a, Message> {
    let header = row![
        row![
            icon("clock-counter-clockwise", false, 14.0, ICON_BLUE),
            label("Recent Files", 13.0, SEGOE_SEMIBOLD, text_main(), 15.0),
        ]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center),
        Space::new().width(Length::Fill),
        crate::tooltips::tip_below(
            button(container(label("Clear", 11.0, SEGOE, text_main(), 15.0)).padding([2, 8]),)
                .on_press(Message::RecentClear)
                .style(|_, status| button::Style {
                    background: match status {
                        button::Status::Hovered | button::Status::Pressed =>
                            Some(bg_hover().into()),
                        _ => None,
                    },
                    border: Border {
                        color: border(),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..button::Style::default()
                }),
            "Clear recent files",
        ),
    ]
    .padding([11, 14])
    .align_y(iced::alignment::Vertical::Center);

    let list: Element<'a, Message> = if recent.is_empty() {
        // The `<br>` splits the note into two lines.
        container(
            column![
                label("No recent files yet.", 12.0, SEGOE, muted(), 19.2),
                label(
                    "Files you save or open will appear here.",
                    12.0,
                    SEGOE,
                    muted(),
                    19.2
                ),
            ]
            .spacing(2),
        )
        .padding([24, 14])
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
    } else {
        scrollable(column(
            recent
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let date = crate::recent::display_time(&entry.last_opened);
                    recent_row(i, entry.name.clone(), date)
                })
                .collect::<Vec<Element<'a, Message>>>(),
        ))
        .height(Length::Fill)
        .into()
    };

    container(column![header, list].height(Length::Fill))
        .width(300)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(bg_app())),
            border: Border {
                color: border(),
                width: 0.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// One `.recent-file-item`: the fill file icon (#F6E05E, 18px), the name
/// (13px, ellipsis) over the date (11px, muted). The strings are owned so
/// nothing borrows the per-frame list.
fn recent_row<'a>(index: usize, name: String, date: String) -> Element<'a, Message> {
    let body = row![
        icon("file", true, 18.0, hex(0xF6E05E)),
        column![
            label(&name, 13.0, SEGOE, text_main(), 15.0),
            label(&date, 11.0, SEGOE, muted(), 15.0),
        ]
        .spacing(1),
    ]
    .spacing(10)
    .align_y(iced::alignment::Vertical::Center);
    button(container(body).padding([9, 14]).width(Length::Fill))
        .width(Length::Fill)
        .padding(0)
        .on_press(Message::RecentOpen(index))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
                _ => None,
            },
            ..button::Style::default()
        })
        .into()
}

/// Whether a press at `(x, y)` (page coordinates) is inside the menu.
pub fn contains(x: f32, y: f32) -> bool {
    x < MENU_W && y >= MENU_Y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_wiring_audit_file_menu() {
        let rows: Vec<_> = items().into_iter().map(|item| {
            let action = item_action(item.text);
            serde_json::json!({
                "label": item.text, "action": format!("{action:?}"),
                "unwired": matches!(action, Message::Noop), "point": [100., item.y + ITEM_H / 2.]
            })
        }).collect();
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/control-audit");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("file-menu.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
        for row in rows.iter().filter(|r| r["unwired"] == true) {
            println!("UNWIRED {row}");
        }
        println!("Inventoried {} File menu items", rows.len());
        assert!(
            rows.iter().all(|r| r["unwired"] != true),
            "File menu item has no action"
        );
    }

    #[test]
    fn items_follow_the_measured_rhythm() {
        let its = items();
        assert_eq!(its.len(), 18);
        // Consecutive items are 36px apart unless a divider (1px + 5px
        // margins) sits between them.
        for pair in its.windows(2) {
            let gap = pair[1].y - pair[0].y;
            let divided = DIVIDERS.iter().any(|d| *d > pair[0].y && *d < pair[1].y);
            assert_eq!(gap, if divided { ITEM_H + 11.0 } else { ITEM_H });
        }
        assert!(contains(10.0, 60.0) && !contains(10.0, 40.0) && !contains(250.0, 300.0));
    }
}
