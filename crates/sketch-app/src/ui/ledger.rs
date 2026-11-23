//! Left-docked / sheet Ledger (`ui/ledger.rs`):
//! Drawn areas, Area types palette, Layers tree, Totals foot, and `"..."` menu
//! for the Next shell (`docs/ui-redesign/01-design-spec.md` §5.3 & `03-phases.md` P6).

use std::collections::HashMap;

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, mouse_area, opaque, row, scrollable as make_scrollable, stack, text,
    text_input, Column, Space,
};
use iced::{alignment, mouse, Alignment, Background, Border, Color, Element, Length, Padding};
use sketch_core::geometry::{distance, AreaCode};

use crate::area_workflow::{self, Msg as WorkflowMsg};
use crate::dock::{self, DragItem};
use crate::icons;
use crate::layers::{DragPos, Row as LayerRow};
use crate::ui::fonts::{BARLOW, BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_MEDIUM, BSC_SEMIBOLD};
use crate::ui::kit::{self, MenuEntry};
use crate::ui::layout::SizeClass;
use crate::ui::tokens::{
    self, Density, NUM_24, NUM_32, RADIUS_CONTROL, RADIUS_DIALOG, SPACE_1, SPACE_2, SPACE_3,
    TEXT_11, TEXT_12, TEXT_13, TEXT_14,
};
use crate::ui::Msg as UiMsg;
use crate::view_state::DimensionPlacement;
use crate::{calcs, App, Message};

pub const MIN_W: f32 = 220.0;
pub const MAX_W: f32 = 400.0;
pub const CANVAS_MIN_W: f32 = 220.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedgerView {
    #[default]
    Drawn,
    Types,
}

/// Clamps dragged ledger width to `[MIN_W, MAX_W]` while preserving `CANVAS_MIN_W`.
pub fn clamp_width(w: f32, viewport_w: f32) -> f32 {
    let avail = MIN_W.max(viewport_w - CANVAS_MIN_W);
    MIN_W.max(MAX_W.min(w.round()).min(avail))
}

/// Horizontal resize strip straddling the right edge of the Wide docked Ledger.
pub fn wide_resize_handle<'a>(app: &'a App) -> Element<'a, Message> {
    let lit = app.ui.ledger_handle_hover || app.ui.ledger_resizing;
    let strip =
        container(Space::new().width(7).height(Length::Fill)).style(move |_| container::Style {
            background: lit.then_some(Background::Color(Color {
                a: 0.35,
                ..tokens::accent()
            })),
            ..container::Style::default()
        });
    let handle = mouse_area(strip)
        .on_press(Message::Ui(UiMsg::LedgerResizeStart))
        .on_double_click(Message::Ui(UiMsg::LedgerResizeReset))
        .on_enter(Message::Ui(UiMsg::LedgerHandleHover(true)))
        .on_exit(Message::Ui(UiMsg::LedgerHandleHover(false)))
        .interaction(mouse::Interaction::ResizingHorizontally);
    let left_pad = (app.ui.effective_ledger_width() - 3.0).max(0.0);
    row![
        Space::new().width(Length::Fixed(left_pad)),
        handle,
        Space::new().width(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Main Ledger panel for Wide dock, Medium slide-over, or Compact bottom sheet.
pub fn panel<'a>(
    app: &'a App,
    class: SizeClass,
    d: Density,
    overlay: bool,
) -> Element<'a, Message> {
    if app.workflow.advanced {
        return framed_panel(app, d, overlay, advanced_subview(app, d, overlay));
    }

    let calcs_model = calcs::build(&app.drawing.polygons, &app.computed);
    let view = app.ui.ledger_view;

    let header = panel_header(app, class, view, d, overlay);
    let lists: Element<'a, Message> = match view {
        LedgerView::Drawn => drawn_view(app, &calcs_model, d),
        LedgerView::Types => types_view(app, d),
    };

    let content = column![
        header,
        container(lists).width(Length::Fill).height(Length::Fill)
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .height(Length::Fill);

    let content = content.push(totals_foot(app, &calcs_model, class, d));

    framed_panel(app, d, overlay, content.into())
}

fn framed_panel<'a>(
    app: &'a App,
    _d: Density,
    overlay: bool,
    inner: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut box_el = container(inner)
        .padding(SPACE_3)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(tokens::panel())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: if overlay { RADIUS_DIALOG } else { 0.0 }.into(),
            },
            shadow: if overlay {
                tokens::floating_shadow()
            } else {
                iced::Shadow::default()
            },
            ..container::Style::default()
        });

    if !overlay {
        box_el = box_el.width(Length::Fixed(app.ui.effective_ledger_width()));
    } else {
        box_el = box_el.width(Length::Fill);
    }

    box_el.into()
}

fn panel_header<'a>(
    app: &'a App,
    class: SizeClass,
    active_view: LedgerView,
    d: Density,
    overlay: bool,
) -> Element<'a, Message> {
    let mut title_row = row![
        text("Areas")
            .font(BARLOW_SEMIBOLD)
            .size(TEXT_14)
            .color(tokens::text()),
        Space::new().width(Length::Fill),
        kit::tooltip(
            kit::icon_button(
                "folder-simple-plus",
                false,
                Some(Message::LayerNewFolder),
                d
            ),
            "New layer folder",
            iced::widget::tooltip::Position::Bottom,
        ),
        kit::icon_button(
            "dots-three",
            app.ui.ledger_menu,
            Some(Message::Ui(UiMsg::ToggleLedgerMenu)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if overlay {
        title_row = title_row.push(kit::text_button(
            "Close",
            Some(Message::Ui(UiMsg::CloseOverlays)),
            d,
        ));
    }

    let segments = vec![
        kit::Segment {
            label: "Drawn",
            selected: active_view == LedgerView::Drawn,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::SetLedgerView(LedgerView::Drawn))),
        },
        kit::Segment {
            label: "Types",
            selected: active_view == LedgerView::Types,
            focused: false,
            on_press: Some(Message::Ui(UiMsg::SetLedgerView(LedgerView::Types))),
        },
    ];
    let _ = class;

    column![title_row, kit::segmented(segments, d)]
        .spacing(SPACE_2)
        .width(Length::Fill)
        .into()
}

/// Floating `"..."` overflow menu for the Ledger.
pub fn layer<'a>(app: &'a App) -> Option<Element<'a, Message>> {
    if !app.ui.ledger_menu {
        return None;
    }
    let d = app.ui.density;
    let class = SizeClass::of(app.window_size.width);
    let entries = vec![
        MenuEntry::Item {
            icon: Some("arrows-out"),
            label: "Expand all",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::CalcsExpandAll),
        },
        MenuEntry::Item {
            icon: Some("arrows-in"),
            label: "Collapse all",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::CalcsCollapseAll),
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: Some("warning"),
            label: "Find areas that need a type",
            shortcut: None,
            checked: app.workflow.find_undefined,
            focused: false,
            on_press: Some(Message::AreaWorkflow(WorkflowMsg::FindUndefined)),
        },
        MenuEntry::Item {
            icon: Some("bounding-box"),
            label: "Detect regions",
            shortcut: None,
            checked: false,
            focused: false,
            on_press: Some(Message::AreaWorkflow(WorkflowMsg::Detect)),
        },
        MenuEntry::Item {
            icon: Some("sliders"),
            label: "Label defaults & area tools…",
            shortcut: None,
            checked: app.workflow.advanced,
            focused: false,
            on_press: Some(Message::AreaWorkflow(WorkflowMsg::Advanced(true))),
        },
    ];
    let card = kit::popover(kit::menu(entries, 240.0, d), RADIUS_DIALOG);
    let top_pad = d.bar_h()
        + if matches!(class, SizeClass::Wide) {
            SPACE_3 * 2.0 + 24.0
        } else {
            crate::ui::layout::PAGE_BAR_H + SPACE_3 * 2.0 + 24.0
        };
    let left_pad = match class {
        SizeClass::Wide => (app.ui.effective_ledger_width() - 252.0).max(SPACE_3),
        SizeClass::Medium | SizeClass::Compact => SPACE_3,
    };

    Some(opaque(
        mouse_area(
            container(opaque(card))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding {
                    top: top_pad,
                    left: left_pad,
                    right: SPACE_3,
                    bottom: SPACE_3,
                })
                .align_x(alignment::Horizontal::Left)
                .align_y(alignment::Vertical::Top),
        )
        .on_press(Message::Ui(UiMsg::ToggleLedgerMenu)),
    ))
}

// ---------------------------------------------------------------------------
// Areas and layers: one tree
// ---------------------------------------------------------------------------

/// Every drawn area's numbers by polygon id: the typed ones from the calcs
/// model, plus the ones that still need a type.
fn area_rows_by_id(app: &App, model: &calcs::Calcs) -> HashMap<i64, calcs::AreaRow> {
    let mut by_id = HashMap::new();
    for group in &model.groups {
        for area in &group.areas {
            by_id.insert(area.id, area.clone());
        }
    }
    for poly in app
        .drawing
        .polygons
        .iter()
        .filter(|p| p.kind == "UND" || p.kind.is_empty())
    {
        let title = if poly.name.is_empty() {
            "Undefined Area"
        } else {
            poly.name.as_str()
        };
        by_id.insert(
            poly.id,
            calcs::AreaRow {
                id: poly.id,
                name: title.to_owned(),
                base_area: poly.gross,
                factor: poly.factor,
                total: poly.gross * poly.factor,
                subtracted: app.computed.get(&poly.id).map(|c| c.subtracted),
                net_area: poly.area,
                perimeter: app.computed.get(&poly.id).map_or(0.0, |c| c.perimeter),
            },
        );
    }
    by_id
}

/// What a tree row stands for, with the text it shows already formed.
enum RowKind {
    /// A folder and the total of the areas inside it.
    Folder(f64),
    /// A closed area's own layer.
    Area {
        poly: i64,
        name: String,
        /// "CODE · 412.0 sf", on a second line so a long name has room.
        detail: String,
        swatch: Color,
        flagged: bool,
    },
    /// Any other layer: loose walls, symbols and labels.
    Other,
}

/// The Areas tab: folders, area layers and other layers in one list, each
/// with its eye and lock, in the order they paint. Drag a row to reorder it
/// or to move it into a folder.
fn drawn_view<'a>(app: &'a App, model: &calcs::Calcs, d: Density) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();

    if app.drawing.pen_down && !app.drawing.points.is_empty() {
        rows.push(drawing_in_progress_row(app));
    }

    let areas = area_rows_by_id(app, model);
    // Every layer is listed, so a layer left behind by a deleted area (or a
    // reopened one) can be seen and deleted; those show as plain rows.
    let tree = app.layers.visible_rows(|_| true);
    let has_folder = tree.iter().any(|r| r.is_group);

    if rows.is_empty() && areas.is_empty() && !has_folder {
        return container(
            column![
                text(model.empty_note())
                    .font(BARLOW_MEDIUM)
                    .size(TEXT_13)
                    .color(tokens::muted()),
                text("Draw closed walls or drag a type from Types onto an outline.")
                    .font(BARLOW)
                    .size(TEXT_11)
                    .color(tokens::disabled()),
            ]
            .spacing(SPACE_1),
        )
        .padding([SPACE_2, 0.0])
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let folder_total = |id: &str| -> f64 {
        app.layers
            .descendant_layer_ids(id)
            .iter()
            .filter_map(|lid| app.layers.layer(lid))
            .filter_map(|l| l.area_poly_id.as_deref()?.parse::<i64>().ok())
            .filter_map(|poly| areas.get(&poly))
            .map(|a| a.net_area)
            .sum()
    };

    let mut placed: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut active_menu: Option<LayerRow> = None;
    // Layers that hold no area are not areas: they hang under the Layers
    // section at the bottom (folded), so a layer a z-order command or an old
    // workfile left behind (\"Send to Back\", \"Symbols\", \"Layer 1\") never
    // reads as an area row.
    let mut plain: Vec<LayerRow> = Vec::new();
    for r in &tree {
        if app.dock.layer_menu.as_deref() == Some(r.id.as_str()) {
            active_menu = Some(r.clone());
        }
        let area = r
            .area_poly_id
            .as_deref()
            .filter(|_| !r.is_group)
            .and_then(|p| p.parse::<i64>().ok())
            .and_then(|p| areas.get(&p).map(|a| (p, a)));
        match (r.is_group, area) {
            (true, _) => rows.push(layer_row(app, r, RowKind::Folder(folder_total(&r.id)), d)),
            (false, Some((poly, a))) => {
                placed.insert(poly);
                let code = area_code_of(app, poly);
                let flagged = code == "UND";
                let shown = if flagged {
                    "Needs a type"
                } else {
                    code.as_str()
                };
                let kind = RowKind::Area {
                    poly,
                    name: a.name.clone(),
                    detail: format!("{shown} · {:.1} sf", a.net_area),
                    swatch: area_color(&app.area_codes, &code),
                    flagged,
                };
                rows.push(layer_row(app, r, kind, d));
                if app.dock.calcs_expanded.contains(&poly) {
                    rows.push(area_details(app, a, r.depth));
                }
            }
            (false, None) if r.area_poly_id.is_some() => {
                // A dedicated layer whose area is gone: still deletable here.
                rows.push(layer_row(app, r, RowKind::Other, d));
            }
            (false, None) => plain.push(r.clone()),
        }
    }

    // An area whose layer is out of step still shows, without layer tools.
    for poly in &app.drawing.polygons {
        if let (false, Some(a)) = (placed.contains(&poly.id), areas.get(&poly.id)) {
            let code = area_code_of(app, poly.id);
            rows.push(loose_area_row(app, a, &code, d));
        }
    }

    // The Layers section: everything in the tree that is not an area or a
    // folder. Folded by default, so the list reads as areas; open it to
    // hide, lock, rename or delete a plain layer.
    if !plain.is_empty() {
        let open = app.ui.ledger_layers_open;
        rows.push(group_toggle_row(
            "Layers",
            &format!("{}", plain.len()),
            !open,
            false,
            Message::Ui(UiMsg::ToggleLedgerLayers),
            d,
        ));
        if open {
            for r in &plain {
                rows.push(layer_row(app, r, RowKind::Other, d));
            }
        }
    }

    let sc = make_scrollable(Column::with_children(rows).spacing(2).width(Length::Fill))
        .id("ledger-areas")
        .direction(Direction::Vertical(
            Scrollbar::new().width(5).scroller_width(5),
        ))
        .width(Length::Fill)
        .height(Length::Fill);
    if let Some(menu_row) = active_menu {
        stack![sc, inline_layer_menu(app, &menu_row, d)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        sc.into()
    }
}

/// The type code of a polygon; an empty one counts as undefined.
fn area_code_of(app: &App, poly: i64) -> String {
    app.drawing
        .polygons
        .iter()
        .find(|p| p.id == poly)
        .map(|p| p.kind.clone())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| "UND".to_owned())
}

fn drawing_in_progress_row<'a>(app: &'a App) -> Element<'a, Message> {
    let wall_count = app.drawing.line_ids.len();
    let total_len: f64 = app
        .drawing
        .points
        .windows(2)
        .map(|w| distance(w[0], w[1]))
        .sum();
    let detail = format!(
        "{wall_count} wall{} · {}",
        if wall_count == 1 { "" } else { "s" },
        app.drawing.options.units.length(total_len)
    );
    container(
        row![
            icons::mono("pencil-simple", 14.0, tokens::accent()),
            text("Drawing an area")
                .font(BARLOW_MEDIUM)
                .size(TEXT_12)
                .color(tokens::text())
                .width(Length::Fill),
            text(detail)
                .font(BSC_MEDIUM)
                .size(TEXT_12)
                .color(tokens::accent()),
        ]
        .spacing(SPACE_2)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([6, 8])
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::accent_soft())),
        border: Border {
            color: tokens::accent(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn group_toggle_row<'a>(
    title: &str,
    total_text: &str,
    collapsed: bool,
    flagged: bool,
    on_press: Message,
    d: Density,
) -> Element<'a, Message> {
    let fg = if flagged {
        tokens::flag()
    } else {
        tokens::muted()
    };
    let caret = if collapsed {
        "caret-right"
    } else {
        "caret-down"
    };
    button(
        row![
            icons::mono(caret, 12.0, fg),
            text(title.to_owned())
                .font(BSC_SEMIBOLD)
                .size(TEXT_12)
                .color(fg)
                .width(Length::Fill),
            text(total_text.to_owned())
                .font(BSC_SEMIBOLD)
                .size(TEXT_12)
                .color(if flagged {
                    tokens::flag()
                } else {
                    tokens::text()
                }),
        ]
        .spacing(SPACE_1)
        .align_y(Alignment::Center)
        .height(Length::Fixed(if matches!(d, Density::Touch) {
            32.0
        } else {
            26.0
        }))
        .padding([0, 4]),
    )
    .padding(0)
    .width(Length::Fill)
    .on_press(on_press)
    .style(|_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: hovered.then_some(Background::Color(tokens::hover())),
            border: Border {
                radius: RADIUS_CONTROL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    })
    .into()
}

fn swatch_box<'a>(swatch: Color) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(12.0))
        .height(Length::Fixed(12.0))
        .style(move |_| container::Style {
            background: Some(Background::Color(swatch)),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A polygon with no live layer of its own: a plain row that selects it.
fn loose_area_row<'a>(
    app: &'a App,
    area: &calcs::AreaRow,
    code: &str,
    d: Density,
) -> Element<'a, Message> {
    let flagged = code == "UND";
    let selected = app.selection.polys.contains(&area.id);
    let swatch = area_color(&app.area_codes, code);
    let body = row![
        swatch_box(swatch),
        column![
            text(area.name.clone())
                .font(BARLOW_MEDIUM)
                .size(TEXT_13)
                .color(if flagged {
                    tokens::flag()
                } else {
                    tokens::text()
                }),
            text(format!(
                "{} · {:.1} sf",
                if flagged { "Needs a type" } else { code },
                area.net_area
            ))
            .font(BSC_MEDIUM)
            .size(TEXT_11)
            .color(if flagged {
                tokens::flag()
            } else {
                tokens::muted()
            }),
        ]
        .spacing(1)
        .width(Length::Fill),
    ]
    .spacing(SPACE_2)
    .align_y(Alignment::Center);
    let open = Message::AreaWorkflow(WorkflowMsg::Open(area.id));
    kit::with_id(
        format!("area-row-{}", area.id),
        kit::long_press(
            mouse_area(
                button(
                    container(body)
                        .height(Length::Fixed(area_row_h(d)))
                        .padding([0, 8])
                        .align_y(Alignment::Center)
                        .width(Length::Fill),
                )
                .padding(0)
                .width(Length::Fill)
                .on_press(Message::CalcsArea(area.id))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(match status {
                        button::Status::Hovered => tokens::hover(),
                        _ if selected => tokens::accent_soft(),
                        _ => Color::TRANSPARENT,
                    })),
                    border: Border {
                        radius: RADIUS_CONTROL.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }),
            )
            .on_right_press(open.clone()),
            open,
        ),
    )
}

fn area_row_h(d: Density) -> f32 {
    if matches!(d, Density::Touch) {
        56.0
    } else {
        48.0
    }
}

/// The expanded numbers under an area row (Base area ... Perimeter).
fn area_details<'a>(app: &'a App, area: &calcs::AreaRow, depth: usize) -> Element<'a, Message> {
    let sub = area.subtracted.unwrap_or(0.0);
    let sub_text = if sub > 0.0 {
        format!("-{sub:.1} sf")
    } else {
        "0.0 sf".to_owned()
    };
    // One decimal for areas and lengths everywhere (rows, inspector, plan
    // labels, totals); only the factor, a multiplier, keeps two.
    container(
        column![
            detail_line("Base area", format!("{:.1} sf", area.base_area), false),
            detail_line("Factor", format!("{:.2}", area.factor), false),
            detail_line("Total", format!("{:.1} sf", area.total), false),
            detail_line("Subtracted", sub_text, sub > 0.0),
            detail_line("Net area", format!("{:.1} sf", area.net_area), false),
            detail_line(
                "Perimeter",
                app.drawing.options.units.length(area.perimeter),
                false
            ),
        ]
        .spacing(2),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: 6.0,
        right: 10.0,
        bottom: 6.0,
        left: 20.0 + depth as f32 * 14.0,
    })
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::field())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn layer_row<'a>(app: &'a App, r: &LayerRow, kind: RowKind, d: Density) -> Element<'a, Message> {
    let id = r.id.as_str();
    let area_poly = match &kind {
        RowKind::Area { poly, .. } => Some(*poly),
        _ => None,
    };
    let is_folder = matches!(kind, RowKind::Folder(_));
    let dragging = app.dock.layer_drag.as_deref() == Some(id);
    let drop_mark: Option<DragPos> = if dragging {
        None
    } else {
        app.dock
            .layer_over
            .as_ref()
            .filter(|(target, _)| target != id && app.dock.layer_drag.is_some())
            // Only a folder takes a row inside it; elsewhere it lands after.
            .map(|(_, p)| match p {
                DragPos::Inside if !is_folder => DragPos::After,
                other => *other,
            })
    };

    let hovered = app.dock.layer_hover.as_deref() == Some(id);
    let menu_open = app.dock.layer_menu.as_deref() == Some(id);
    let renaming = app
        .dock
        .layer_rename
        .as_ref()
        .filter(|(target, _)| target == id)
        .map(|(_, buf)| buf.clone());

    let touch = matches!(d, Density::Touch);
    let row_h: f32 = match kind {
        RowKind::Area { .. } => area_row_h(d),
        _ if touch => 38.0,
        _ => 30.0,
    };
    let mid_h = (row_h - 12.0).max(16.0);
    let selected = match &kind {
        RowKind::Area { poly, .. } => app.selection.polys.contains(poly),
        RowKind::Other => r.active,
        RowKind::Folder(_) => false,
    };
    let dim = !r.visible || r.locked;
    let fg = if dragging {
        tokens::disabled()
    } else if dim {
        tokens::muted()
    } else {
        tokens::text()
    };
    let name_font = if selected {
        BARLOW_SEMIBOLD
    } else {
        BARLOW_MEDIUM
    };

    let head: Element<'a, Message> = if is_folder {
        let caret = if r.collapsed {
            "caret-right"
        } else {
            "caret-down"
        };
        kit::icon_button(
            caret,
            false,
            Some(Message::LayerToggleCollapsed(id.to_owned())),
            d,
        )
    } else {
        Space::new().width(Length::Fixed(16.0)).into()
    };

    let lead: Element<'a, Message> = match &kind {
        RowKind::Folder(_) => icons::mono("folder-simple", 14.0, tokens::accent()),
        RowKind::Area { swatch, .. } => swatch_box(*swatch),
        RowKind::Other => icons::mono("shapes", 14.0, tokens::muted()),
    };

    let mid_content: Element<'a, Message> = if let Some(buf) = renaming {
        let id_owned = id.to_owned();
        container(
            text_input("", buf.as_str())
                .id(dock::RENAME_ID)
                .size(TEXT_12)
                .font(BARLOW)
                .padding([2, 6])
                .width(Length::Fill)
                .on_input(move |s| Message::LayerRename(id_owned.clone(), s))
                .on_submit(Message::LayerRenameCommit),
        )
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .into()
    } else {
        match kind {
            RowKind::Area {
                name,
                detail,
                flagged,
                ..
            } => column![
                text(name)
                    .font(name_font)
                    .size(TEXT_13)
                    .wrapping(Wrapping::None)
                    .color(if flagged { tokens::flag() } else { fg }),
                text(detail)
                    .font(BSC_MEDIUM)
                    .size(TEXT_11)
                    .wrapping(Wrapping::None)
                    .color(if flagged {
                        tokens::flag()
                    } else {
                        tokens::muted()
                    }),
            ]
            .spacing(1)
            .width(Length::Fill)
            .into(),
            RowKind::Folder(total) => row![
                text(r.name.clone())
                    .font(BARLOW_SEMIBOLD)
                    .size(TEXT_13)
                    .wrapping(Wrapping::None)
                    .color(fg)
                    .width(Length::Fill),
                text(format!("{total:.1} sf"))
                    .font(BSC_MEDIUM)
                    .size(TEXT_12)
                    .wrapping(Wrapping::None)
                    .color(tokens::muted()),
            ]
            .spacing(SPACE_1)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into(),
            RowKind::Other => text(r.name.clone())
                .font(name_font)
                .size(TEXT_13)
                .wrapping(Wrapping::None)
                .color(fg)
                .width(Length::Fill)
                .into(),
        }
    };
    let mid_content = container(mid_content)
        .width(Length::Fill)
        .height(Length::Fixed(mid_h))
        .align_y(Alignment::Center)
        .clip(true);

    let zone = |h: f32, pos: DragPos, content: Element<'a, Message>| -> Element<'a, Message> {
        mouse_area(
            container(content)
                .width(Length::Fill)
                .height(Length::Fixed(h)),
        )
        .on_press(Message::LayerPress(id.to_owned()))
        .on_release(Message::LayerDrop(id.to_owned(), pos))
        .on_enter(Message::LayerZoneHover(id.to_owned(), pos))
        .into()
    };
    let strip = column![
        zone(6.0, DragPos::Before, Space::new().into()),
        zone(mid_h, DragPos::Inside, mid_content.into()),
        zone(6.0, DragPos::After, Space::new().into()),
    ]
    .width(Length::Fill);

    // The eye, the lock and the menu are always there, in the same places, so
    // hiding or locking is one click and nothing shifts when the pointer
    // arrives. The eye and the lock are bright when on (shown, locked) and
    // greyed out when off.
    let tools = row![
        kit::lit_icon_button(
            "eye",
            r.visible,
            Some(Message::LayerToggleVisible(id.to_owned())),
            d,
        ),
        kit::lit_icon_button(
            if r.locked {
                "lock-simple"
            } else {
                "lock-simple-open"
            },
            r.locked,
            Some(Message::LayerToggleLocked(id.to_owned())),
            d,
        ),
        kit::icon_button(
            "dots-three",
            menu_open,
            Some(Message::LayerRowMenu(id.to_owned())),
            d,
        ),
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    // The grip: a narrow handle at the far left that starts the same drag the
    // row body does (a wider target on touch screens). It lights up with the
    // row, and the cursor turns to a grabbing hand while a row is dragged.
    let grip_w = if touch { 28.0 } else { 14.0 };
    let grip_color = if dragging {
        tokens::accent()
    } else if hovered {
        tokens::text()
    } else {
        tokens::muted()
    };
    let grip = mouse_area(
        container(icons::mono("dots-six-vertical", 14.0, grip_color))
            .width(Length::Fixed(grip_w))
            .height(Length::Fixed(row_h))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .on_press(Message::LayerPress(id.to_owned()))
    .on_release(Message::LayerDrop(id.to_owned(), DragPos::Inside))
    .on_enter(Message::LayerZoneHover(id.to_owned(), DragPos::Inside))
    .interaction(if app.dock.layer_drag.is_some() {
        mouse::Interaction::Grabbing
    } else {
        mouse::Interaction::Grab
    });

    let row_body = row![grip, head, lead, strip, tools]
        .spacing(4)
        .padding(Padding {
            left: 2.0 + r.depth as f32 * 14.0,
            right: 4.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fixed(row_h));

    let inside = drop_mark == Some(DragPos::Inside);
    let frame = container(row_body)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(if selected {
                tokens::accent_soft()
            } else if hovered && !dragging {
                tokens::hover()
            } else {
                Color::TRANSPARENT
            })),
            border: Border {
                color: if selected || inside {
                    tokens::accent()
                } else {
                    Color::TRANSPARENT
                },
                width: if selected || inside { 1.0 } else { 0.0 },
                radius: RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        });

    let marked: Element<'a, Message> = match drop_mark {
        Some(DragPos::Before) => column![drop_rule(), frame].width(Length::Fill).into(),
        Some(DragPos::After) => column![frame, drop_rule()].width(Length::Fill).into(),
        _ => frame.into(),
    };

    let layer_menu_msg = Message::LayerRowMenu(id.to_owned());
    let row = kit::long_press(
        mouse_area(marked)
            .on_enter(Message::LayerHoverRow(id.to_owned()))
            .on_exit(Message::LayerHoverRowGone(id.to_owned()))
            .on_right_press(layer_menu_msg.clone()),
        layer_menu_msg,
    );
    match area_poly {
        Some(poly) => kit::with_id(format!("area-row-{poly}"), row),
        None => row,
    }
}

fn drop_rule<'a>() -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(2.0))
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::accent())),
            ..container::Style::default()
        })
        .into()
}

fn inline_layer_menu<'a>(app: &'a App, r: &LayerRow, d: Density) -> Element<'a, Message> {
    let id = r.id.clone();
    let poly = r
        .area_poly_id
        .as_deref()
        .filter(|_| !r.is_group)
        .and_then(|p| p.parse::<i64>().ok());
    let item = |icon: &'static str, label: &'a str, msg: Message| MenuEntry::Item {
        icon: Some(icon),
        label,
        shortcut: None,
        checked: false,
        focused: false,
        on_press: Some(msg),
    };
    let mut entries: Vec<MenuEntry<'a>> = Vec::new();
    match poly {
        Some(poly) => entries.push(item(
            "pencil-simple",
            "Edit area…",
            Message::AreaWorkflow(WorkflowMsg::Open(poly)),
        )),
        None => entries.push(item(
            "text-t",
            "Rename",
            Message::LayerStartRename(id.clone()),
        )),
    }
    entries.push(item(
        if r.visible { "eye-slash" } else { "eye" },
        if r.visible { "Hide" } else { "Show" },
        Message::LayerToggleVisible(id.clone()),
    ));
    entries.push(item(
        if r.locked {
            "lock-simple-open"
        } else {
            "lock-simple"
        },
        if r.locked { "Unlock" } else { "Lock" },
        Message::LayerToggleLocked(id.clone()),
    ));

    if !r.is_group {
        let current = app.layers.layer(&r.id).and_then(|l| l.parent_id.as_deref());
        let folders: Vec<&'a crate::layers::Layer> = app
            .layers
            .items
            .iter()
            .filter(|l| l.is_group && Some(l.id.as_str()) != current)
            .collect();
        if !folders.is_empty() || current.is_some() {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Item {
                icon: None,
                label: "Move to layer",
                shortcut: None,
                checked: false,
                focused: false,
                on_press: None,
            });
            for folder in folders {
                entries.push(item(
                    "folder-simple",
                    folder.name.as_str(),
                    Message::LayerMoveTo(id.clone(), Some(folder.id.clone())),
                ));
            }
            if current.is_some() {
                entries.push(item(
                    "arrow-up",
                    "Top level",
                    Message::LayerMoveTo(id.clone(), None),
                ));
            }
        }
    }
    entries.push(MenuEntry::Separator);
    entries.push(item(
        "trash",
        if poly.is_some() {
            "Delete area"
        } else {
            "Delete layer"
        },
        Message::LayerDelete(id.clone()),
    ));
    entries.push(item("x", "Close menu", Message::LayerMenuClose));

    container(opaque(kit::popover(
        kit::menu(entries, 190.0, d),
        RADIUS_DIALOG,
    )))
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([4, 8])
    .align_x(alignment::Horizontal::Right)
    .align_y(alignment::Vertical::Top)
    .into()
}

fn detail_line<'a>(label: &'static str, value: String, alarm: bool) -> Element<'a, Message> {
    row![
        text(label)
            .font(BARLOW)
            .size(TEXT_11)
            .color(tokens::muted())
            .width(Length::Fill),
        text(value).font(BSC_MEDIUM).size(TEXT_12).color(if alarm {
            tokens::flag()
        } else {
            tokens::text()
        }),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Types view
// ---------------------------------------------------------------------------

fn types_view<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let search = kit::field(
        kit::FieldProps {
            label: "",
            value: &app.ui.type_filter,
            placeholder: "Search area types",
            on_input: Some(Box::new(|s| Message::Ui(UiMsg::TypeFilterChanged(s)))),
            on_submit: None,
            focused: false,
        },
        d,
    );

    let needle = app.ui.type_filter.trim().to_lowercase();
    let model = dock::paint_palette(&app.area_codes);
    let mut list = column![].spacing(4).width(Length::Fill);

    for grp in &model.groups {
        let matching: Vec<_> = grp
            .tiles
            .iter()
            .filter(|(name, code, _)| {
                needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || code.to_lowercase().contains(&needle)
                    || grp.label.to_lowercase().contains(&needle)
            })
            .collect();
        if matching.is_empty() {
            continue;
        }
        let collapsed = needle.is_empty() && app.workflow.collapsed.contains(&grp.group);
        list = list.push(group_toggle_row(
            &grp.label,
            &format!("{}", matching.len()),
            collapsed,
            false,
            Message::AreaWorkflow(WorkflowMsg::Group(grp.group.clone())),
            d,
        ));
        if collapsed {
            continue;
        }
        for (name, code, color_u32) in matching {
            list = list.push(type_row(app, name, code, *color_u32, false, d));
        }
    }

    if let Some((clear_name, clear_code, clear_color)) = &model.clear {
        if needle.is_empty()
            || clear_name.to_lowercase().contains(&needle)
            || clear_code.to_lowercase().contains(&needle)
            || "undefined".contains(&needle)
        {
            list = list.push(kit::group_header("Undefined", None));
            list = list.push(type_row(app, clear_name, clear_code, *clear_color, true, d));
        }
    }

    let scroll = make_scrollable(list)
        .direction(Direction::Vertical(
            Scrollbar::new().width(5).scroller_width(5),
        ))
        .width(Length::Fill)
        .height(Length::Fill);

    let footer = row![
        kit::text_button("Edit types", Some(Message::OpenCodesEditor), d),
        kit::text_button(
            "New type",
            Some(Message::AreaWorkflow(WorkflowMsg::NewType)),
            d,
        ),
        kit::text_button(
            "Find undefined",
            Some(Message::AreaWorkflow(WorkflowMsg::FindUndefined)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .wrap();

    let mut col = column![search]
        .spacing(SPACE_2)
        .width(Length::Fill)
        .height(Length::Fill);
    if !app.workflow.status.is_empty() {
        col = col.push(
            container(
                text(app.workflow.status.clone())
                    .font(BARLOW)
                    .size(TEXT_11)
                    .color(tokens::accent()),
            )
            .width(Length::Fill)
            .padding([4, 8])
            .style(|_| container::Style {
                background: Some(Background::Color(tokens::accent_soft())),
                border: Border {
                    radius: RADIUS_CONTROL.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        );
    }
    col = col.push(scroll).push(footer);
    col.into()
}

fn type_row<'a>(
    app: &'a App,
    name: &str,
    code: &str,
    color_u32: u32,
    is_clear: bool,
    d: Density,
) -> Element<'a, Message> {
    let used = app.workflow.usage.get(code).copied().unwrap_or(0);
    let armed = app.workflow.armed.as_deref() == Some(code);
    let swatch = hex_u32(color_u32);
    let drag_item = DragItem::Area {
        code: code.to_owned(),
        name: (!is_clear).then(|| name.to_owned()),
    };

    let row_h = d.row_h();
    let mut main_content = row![
        container(Space::new())
            .width(Length::Fixed(10.0))
            .height(Length::Fixed(10.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(swatch)),
                border: Border {
                    color: tokens::rule(),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            }),
        text(name.to_owned())
            .font(if armed {
                BARLOW_SEMIBOLD
            } else {
                BARLOW_MEDIUM
            })
            .size(TEXT_13)
            .color(if is_clear {
                tokens::flag()
            } else {
                tokens::text()
            })
            .width(Length::Fill),
    ]
    .spacing(SPACE_2)
    .align_y(Alignment::Center);

    if used > 0 && !is_clear {
        main_content = main_content.push(
            container(
                text(used.to_string())
                    .font(BSC_MEDIUM)
                    .size(TEXT_11)
                    .color(tokens::accent()),
            )
            .padding([1, 6])
            .style(|_| container::Style {
                background: Some(Background::Color(tokens::accent_soft())),
                border: Border {
                    radius: 999.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        );
    }

    main_content = main_content.push(
        text(code.to_owned())
            .font(BSC_MEDIUM)
            .size(TEXT_11)
            .color(tokens::muted()),
    );

    let type_menu_msg = Message::AreaWorkflow(WorkflowMsg::TypeMenu(code.to_owned()));
    let interactive = kit::long_press(
        mouse_area(
            container(main_content)
                .height(Length::Fixed(row_h))
                .padding([0, 8])
                .align_y(Alignment::Center)
                .width(Length::Fill),
        )
        .on_press(Message::DockDragPress(drag_item.clone()))
        .on_release(Message::DockItemRelease(drag_item))
        .on_right_press(type_menu_msg.clone()),
        type_menu_msg,
    );

    let bg = if armed {
        tokens::accent_soft()
    } else {
        Color::TRANSPARENT
    };
    let tile_btn = button(interactive)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Noop)
        .style(move |_, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered => tokens::hover(),
                button::Status::Pressed => tokens::pressed(),
                _ => bg,
            })),
            border: Border {
                color: if armed {
                    tokens::accent()
                } else {
                    Color::TRANSPARENT
                },
                width: if armed { 1.0 } else { 0.0 },
                radius: RADIUS_CONTROL.into(),
            },
            ..button::Style::default()
        });

    let menu_btn = kit::icon_button(
        "dots-three",
        app.workflow.menu.as_deref() == Some(code),
        Some(Message::AreaWorkflow(WorkflowMsg::TypeMenu(
            code.to_owned(),
        ))),
        d,
    );

    row![tile_btn, menu_btn]
        .spacing(2)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Advanced / Label defaults subview (`workflow.advanced`)
// ---------------------------------------------------------------------------

fn advanced_subview<'a>(app: &'a App, d: Density, overlay: bool) -> Element<'a, Message> {
    let mut header = row![
        kit::icon_button(
            "arrow-left",
            false,
            Some(Message::AreaWorkflow(WorkflowMsg::Advanced(false))),
            d,
        ),
        text("Label defaults & area tools")
            .font(BARLOW_SEMIBOLD)
            .size(TEXT_14)
            .color(tokens::text())
            .width(Length::Fill),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if overlay {
        header = header.push(kit::text_button(
            "Close",
            Some(Message::Ui(UiMsg::CloseOverlays)),
            d,
        ));
    }

    let mut post = app.drawing.auto_post.clone();
    post.dims = app.view.dimensions;
    let placement_in = app.view.dimension_placement == DimensionPlacement::In;

    let body = column![
        kit::group_header("AREA OPERATIONS", None),
        row![
            kit::text_button(
                "Redefine",
                Some(Message::AreaWorkflow(WorkflowMsg::Redefine)),
                d
            ),
            kit::text_button("Clone", Some(Message::AreaWorkflow(WorkflowMsg::Clone)), d),
            kit::text_button(
                "Reopen",
                Some(Message::AreaWorkflow(WorkflowMsg::Reopen)),
                d
            ),
            kit::text_button(
                "Detect",
                Some(Message::AreaWorkflow(WorkflowMsg::Detect)),
                d
            ),
        ]
        .spacing(SPACE_1),
        kit::divider(),
        kit::group_header("LABEL DEFAULTS", None),
        area_workflow::post_controls(&post, true),
        kit::segmented(
            vec![
                kit::Segment {
                    label: "Dims Out",
                    selected: !placement_in,
                    focused: false,
                    on_press: Some(Message::AreaWorkflow(WorkflowMsg::DefaultPlacement(
                        "Out".to_owned(),
                    ))),
                },
                kit::Segment {
                    label: "Dims In",
                    selected: placement_in,
                    focused: false,
                    on_press: Some(Message::AreaWorkflow(WorkflowMsg::DefaultPlacement(
                        "In".to_owned(),
                    ))),
                },
            ],
            d,
        ),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill);

    column![
        header,
        make_scrollable(body)
            .direction(Direction::Vertical(
                Scrollbar::new().width(5).scroller_width(5)
            ))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Totals foot
// ---------------------------------------------------------------------------

fn totals_foot<'a>(
    app: &'a App,
    calcs_model: &calcs::Calcs,
    class: SizeClass,
    d: Density,
) -> Element<'a, Message> {
    let totals = app.calcs_totals.unwrap_or_default();
    let non_living = app
        .last_recalc
        .as_ref()
        .map(|r| (r.totals.total_all_areas - r.totals.total_gla).max(0.0))
        .unwrap_or(0.0);

    if matches!(class, SizeClass::Compact) {
        return column![
            kit::divider(),
            row![
                column![
                    text("Gross living area")
                        .font(BARLOW_MEDIUM)
                        .size(TEXT_11)
                        .color(tokens::muted()),
                    text(format!("{:.1} sf", totals.total_gla))
                        .font(BSC_SEMIBOLD)
                        .size(NUM_24)
                        .color(tokens::text()),
                ]
                .width(Length::Fill),
                kit::text_button("Show the math", Some(Message::OpenBreakdown), d),
            ]
            .spacing(SPACE_2)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill)
        .into();
    }

    let mut stats = column![
        detail_line("Building area", format!("{:.1} sf", totals.gba), false),
        detail_line("Not counted", format!("{:.1} sf", non_living), false),
    ]
    .spacing(2)
    .width(Length::Fill);
    if calcs_model.und_count > 0 {
        stats = stats.push(detail_line(
            "Needs a type",
            format!("{}", calcs_model.und_count),
            true,
        ));
    }

    column![
        kit::divider(),
        text("Gross living area")
            .font(BARLOW_MEDIUM)
            .size(TEXT_11)
            .color(tokens::muted()),
        text(format!("{:.1} sf", totals.total_gla))
            .font(BSC_SEMIBOLD)
            .size(NUM_32)
            .color(tokens::text()),
        stats,
        button(
            container(
                text("Show the math")
                    .font(BARLOW_SEMIBOLD)
                    .size(TEXT_12)
                    .color(tokens::link()),
            )
            .padding([4, 0]),
        )
        .padding(0)
        .on_press(Message::OpenBreakdown)
        .style(|_, _| button::Style::default()),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

fn hex_u32(rgb: u32) -> Color {
    Color::from_rgb8(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    )
}

fn code_color_rgb(entry: &AreaCode) -> Color {
    let parse = |s: &str| u32::from_str_radix(s.trim().trim_start_matches('#'), 16).ok();
    hex_u32(
        parse(&entry.color)
            .or_else(|| parse(&entry.fill_color))
            .unwrap_or(0x85_85_85),
    )
}

fn area_color(codes: &[AreaCode], code: &str) -> Color {
    codes
        .iter()
        .find(|c| c.code == code)
        .map(code_color_rgb)
        .unwrap_or_else(|| {
            if code == "UND" {
                tokens::flag()
            } else {
                tokens::accent()
            }
        })
}
