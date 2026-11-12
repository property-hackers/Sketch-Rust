//! Command palette (S12-028..070, S12-074) - the sidebar-redesign layer's
//! `Ctrl+K` / QAT-magnifier search (refs/sidebar-redesign.txt:235, CSS in
//! refs/ui-markup.html:163).
//!
//! `CATALOG` is the original's 42 entries in order, each `[id, label,
//! hint]`; the filter is a case-insensitive substring of the label. Arrow
//! keys move the highlight (wrapping), Enter / a row click runs it, Escape /
//! a backdrop press / Ctrl+K again closes. A run closes the palette first,
//! then goes through the native command map. All catalog entries run their
//! corresponding app handlers, including the file commands omitted by the
//! older browser command map.
//!
//! Measurements: the panel is `min(520px, 90vw)` at `top: 12%`, centred, on
//! `--bg-panel` with a 1px `--border`, radius 4 and a big shadow; the
//! backdrop is `rgba(0,0,0,.35)`; the input is 13px with 12/14 padding on
//! `--bg-app` over a 1px bottom rule; the list has 6px padding and a
//! `46vh` cap; rows are 8/10 padded, radius 3, 12.5px, label left and an
//! 11px muted hint right; the active row is `rgba(0,122,204,.16)` with
//! `--accent` text; the empty note is 12px muted, centred, 14px padded.

use iced::widget::{
    button, column, container, mouse_area, opaque, row, scrollable, text, text_input, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Shadow, Vector};

use crate::theme::*;
use crate::Message;

pub const INPUT_ID: &str = "command-palette-input";

/// `CATALOG`: (id, label, hint).
pub const CATALOG: &[(&str, &str, &str)] = &[
    ("save", "Save", "F2"),
    ("save-as", "Save As...", ""),
    ("new", "New sketch", ""),
    ("open", "Open...", ""),
    ("import", "Import...", ""),
    ("export-png", "Export as PNG", ""),
    ("export-svg", "Export as SVG", ""),
    ("export-dxf", "Export as DXF", ""),
    ("print", "Print", "Ctrl+P"),
    ("print-preview", "Print preview", ""),
    ("page-layout", "Page layout", ""),
    ("options", "Options", ""),
    ("undo", "Undo", "Ctrl+Z"),
    ("redo", "Redo", "Ctrl+Y"),
    ("undo-history", "Undo history", ""),
    ("cut", "Cut", "Ctrl+X"),
    ("copy", "Copy", "Ctrl+C"),
    ("paste", "Paste", "Ctrl+V"),
    ("select-all", "Select all", "Ctrl+A"),
    ("select-by-type", "Select by type", ""),
    ("delete-selected", "Delete selection", "Del"),
    ("rotate-sketch", "Rotate sketch", ""),
    ("flip-sketch", "Flip sketch", ""),
    ("draw-color", "Draw color", ""),
    ("subject-info", "Subject info", ""),
    ("area-attributes", "Area attributes", ""),
    ("mini-map", "Mini map", ""),
    ("toggle-calcs", "View calcs", ""),
    ("page-prev", "Previous page", ""),
    ("page-next", "Next page", ""),
    ("page-add", "Add page", ""),
    ("page-delete", "Delete page", ""),
    ("toggle-canvas-background", "Canvas background", ""),
    ("interface-theme", "Interface theme", ""),
    ("toggle-grid", "Toggle grid", ""),
    ("filter", "Filter selection", ""),
    ("disto-connect", "Connect Bluetooth device", ""),
    ("area-breakdown", "Area breakdown", ""),
    ("area-code-settings", "Area codes", ""),
    ("shortcuts", "Keyboard shortcuts", "F1"),
    ("zoom-fence", "Zoom fence", ""),
    ("email", "Email", ""),
    ("version-history", "Version history", ""),
];

/// What a command id does in this port.
pub enum Action {
    /// A message that performs it.
    Run(Message),
    /// `executeCommand`'s fallthrough: "<id> is not available.".
    Unavailable,
}

/// `UIManager.executeCommand(id)` for the catalog's ids.
pub fn action(id: &str) -> Action {
    use crate::statusbar::StatusMsg as S;
    match id {
        "save" => Action::Run(Message::SaveFile),
        "save-as" => Action::Run(Message::SaveFileAs),
        "new" => Action::Run(Message::New),
        "open" => Action::Run(Message::OpenFile),
        "import" => Action::Run(Message::ImportSketch),
        "export-png" => Action::Run(Message::ExportPng),
        "export-svg" => Action::Run(Message::ExportSvg),
        "export-dxf" => Action::Run(Message::ExportDxf),
        "print" => Action::Run(Message::Print),
        "print-preview" => Action::Run(Message::PrintPreviewOpen),
        "page-layout" => Action::Run(Message::PageLayout(crate::page_layout::LayoutMsg::Open)),
        "options" => Action::Run(Message::Options(crate::options_dialog::OptMsg::Open)),
        "undo" => Action::Run(Message::Undo),
        "redo" => Action::Run(Message::Redo),
        "undo-history" => Action::Run(Message::UndoHistory),
        "cut" => Action::Run(Message::Cut),
        "copy" => Action::Run(Message::Copy),
        "paste" => Action::Run(Message::Paste),
        "select-all" => Action::Run(Message::SelectAll),
        "select-by-type" => Action::Run(Message::OpenSelectMenu),
        "delete-selected" => Action::Run(Message::Delete),
        "rotate-sketch" => Action::Run(Message::RotateSketch),
        "draw-color" => Action::Run(Message::OpenColorDialog),
        "subject-info" => Action::Run(Message::OpenSubjectInfo),
        "area-attributes" => Action::Run(Message::OpenAreaAttributes(false)),
        "mini-map" => Action::Run(Message::ToggleMiniMap),
        "toggle-calcs" => Action::Run(Message::ToggleCalcs),
        "page-prev" => Action::Run(Message::Status(S::PagePrev)),
        "page-next" => Action::Run(Message::Status(S::PageNext)),
        "page-add" => Action::Run(Message::Status(S::PageAdd)),
        "page-delete" => Action::Run(Message::Status(S::PageDelete)),
        "toggle-grid" => Action::Run(Message::Status(S::ToggleGrid)),
        "filter" => Action::Run(Message::Status(S::ToggleFilter)),
        "area-breakdown" => Action::Run(Message::OpenBreakdown),
        "area-code-settings" => Action::Run(Message::OpenCodesEditor),
        "shortcuts" => Action::Run(Message::Shortcuts(crate::shortcuts::ShortcutsMsg::Open)),
        "zoom-fence" => Action::Run(Message::Status(S::ZoomFence)),
        "email" => Action::Run(Message::Email),
        "version-history" => Action::Run(Message::Versions(crate::versions::VersionsMsg::Show)),
        "flip-sketch" => Action::Run(Message::PaletteMenu(crate::popup_menu::Kind::Flip)),
        "toggle-canvas-background" => Action::Run(Message::PaletteMenu(
            crate::popup_menu::Kind::CanvasBackground,
        )),
        "interface-theme" => Action::Run(Message::PaletteMenu(
            crate::popup_menu::Kind::InterfaceTheme,
        )),
        "disto-connect" => Action::Run(Message::Disto(crate::disto::DistoMsg::Connect)),
        _ => Action::Unavailable,
    }
}

/// `render(q)`'s filter: the catalog indexes whose label contains the trimmed,
/// lower-cased query.
pub fn matches(query: &str) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    CATALOG
        .iter()
        .enumerate()
        .filter(|(_, (_, label, _))| needle.is_empty() || label.to_lowercase().contains(&needle))
        .map(|(i, _)| i)
        .collect()
}

/// `move(d)`: the highlight wraps around the matches.
pub fn step(active: usize, count: usize, d: i32) -> usize {
    if count == 0 {
        return active;
    }
    ((active as i64 + d as i64).rem_euclid(count as i64)) as usize
}

/// `--accent-soft`: the accent at 16% (14% on the light theme).
pub fn accent_soft() -> Color {
    let a = accent();
    let alpha = if crate::theme::interface_theme() == crate::theme::InterfaceTheme::Light {
        0.14
    } else {
        0.16
    };
    Color { a: alpha, ..a }
}

/// The palette's layer, or None when closed. `window_h` places the panel at
/// `top: 12%`; `window_w` caps its width at 90vw.
pub fn layer(
    open: bool,
    query: &str,
    active: usize,
    window: (f32, f32),
) -> Option<Element<'_, Message>> {
    if !open {
        return None;
    }
    let found = matches(query);
    let active = active.min(found.len().saturating_sub(1));

    let input = text_input("Type a command...", query)
        .id(INPUT_ID)
        .size(13.0)
        .padding([12, 14])
        .width(Length::Fill)
        .on_input(Message::CommandPaletteQuery)
        .on_submit(Message::CommandPaletteRun)
        .style(|_, _| text_input::Style {
            background: Background::Color(bg_app()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            icon: muted(),
            placeholder: muted(),
            value: text_main(),
            selection: crate::dialog::SELECTION,
        });
    let rule = container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(Background::Color(border())),
            ..container::Style::default()
        });

    let list: Element<'_, Message> = if found.is_empty() {
        container(
            text("No matching command.")
                .font(SEGOE)
                .size(12.0)
                .color(muted()),
        )
        .width(Length::Fill)
        .padding(14)
        .center_x(Length::Fill)
        .into()
    } else {
        let mut rows = column![].spacing(0);
        for (pos, &i) in found.iter().enumerate() {
            let (_, label, hint) = CATALOG[i];
            let is_active = pos == active;
            let colour = if is_active { accent() } else { text_main() };
            rows = rows.push(
                button(
                    row![
                        text(label).font(SEGOE).size(12.5).color(colour),
                        Space::new().width(Length::Fill),
                        text(hint).font(SEGOE).size(11.0).color(muted()),
                    ]
                    .spacing(12),
                )
                .width(Length::Fill)
                .padding([8, 10])
                .on_press(Message::CommandPaletteRunAt(pos))
                .style(move |_, _| button::Style {
                    background: is_active.then_some(Background::Color(accent_soft())),
                    text_color: colour,
                    border: Border {
                        radius: 3.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }),
            );
        }
        scrollable(container(rows).padding(6).width(Length::Fill))
            .height(Length::Shrink)
            .into()
    };
    let list = container(list).max_height(window.1 * 0.46);

    let panel = container(column![input, rule, list])
        .width((window.0 * 0.9).min(520.0))
        .clip(true)
        .style(|_| container::Style {
            background: Some(Background::Color(bg_panel())),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.55),
                offset: Vector::new(0.0, 18.0),
                blur_radius: 60.0,
            },
            ..container::Style::default()
        });

    let placed = container(opaque(panel))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(Padding {
            top: window.1 * 0.12,
            ..Padding::ZERO
        });
    let backdrop = container(placed)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.35).into()),
            ..container::Style::default()
        });
    // A press on the backdrop closes (`backdrop.addEventListener('pointerdown')`).
    Some(opaque(
        mouse_area(backdrop).on_press(Message::CommandPaletteClose),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_the_originals_42_entries_plus_version_history() {
        assert_eq!(CATALOG.len(), 43);
        assert_eq!(CATALOG[0], ("save", "Save", "F2"));
        assert_eq!(CATALOG[41], ("email", "Email", ""));
        assert_eq!(CATALOG[42], ("version-history", "Version history", ""));
        let mut ids: Vec<&str> = CATALOG.iter().map(|c| c.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 43, "ids are unique");
    }

    #[test]
    fn the_filter_is_a_case_insensitive_label_substring() {
        assert_eq!(matches("").len(), 43);
        assert_eq!(matches("  MINI ").len(), 1);
        assert_eq!(CATALOG[matches("mini")[0]].0, "mini-map");
        // Labels only: the id `export-png` is not searched.
        assert!(matches("export-png").is_empty());
        assert_eq!(matches("page").len(), 5, "Page layout + the four page ops");
        assert!(matches("zzz").is_empty());
    }

    #[test]
    fn the_highlight_wraps() {
        assert_eq!(step(0, 5, -1), 4);
        assert_eq!(step(4, 5, 1), 0);
        assert_eq!(step(2, 5, 1), 3);
        assert_eq!(step(3, 0, 1), 3);
    }

    #[test]
    fn every_catalog_entry_has_a_working_native_handler() {
        for (id, _, _) in CATALOG {
            assert!(matches!(action(id), Action::Run(_)), "{id}");
        }
        assert!(matches!(action("unknown-command"), Action::Unavailable));
        assert!(matches!(action("save"), Action::Run(_)));
        assert!(matches!(action("flip-sketch"), Action::Run(_)));
        assert!(matches!(action("disto-connect"), Action::Run(_)));
    }
}
