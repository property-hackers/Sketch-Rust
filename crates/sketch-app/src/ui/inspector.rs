//! Right-docked / slide-over / Compact-sheet Inspector (`ui/inspector.rs`):
//! Contextual editor for walls, areas, text labels, symbols, chain dimensions,
//! trace images in edit mode, and multi-kind selections
//! (`docs/ui-redesign/01-design-spec.md` §5.4 & `03-phases.md` P7).

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
    button, checkbox, column, container, keyed_column, pick_list, row,
    scrollable as make_scrollable, slider, text, Column, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};
use sketch_core::geometry::distance;

use crate::area_dialog::{AreaDialogMsg, PostSwitch};
use crate::area_props::AreaCycle;
use crate::area_workflow::Msg as WorkflowMsg;
use crate::context_menu::Action;
use crate::dock::palette_group_label;
use crate::icons;
use crate::input::KeyCmd;
use crate::ui::fonts::{BARLOW, BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_MEDIUM};
use crate::ui::kit::{self, FieldProps, Segment};
use crate::ui::tokens::{
    self, Density, RADIUS_CONTROL, RADIUS_DIALOG, SPACE_1, SPACE_2, SPACE_3, TEXT_11, TEXT_12,
    TEXT_14,
};
use crate::ui::Msg as UiMsg;
use crate::view_state::{CurveMode, Mode};
use crate::{App, Message};

/// Contextual selection data shown by the Inspector.
pub struct EditToolsData {
    pub endpoint_label: &'static str,
    pub color: Color,
    pub text: Option<TextTools>,
    pub area: Option<AreaTools>,
}

/// The editable text fields inside the Inspector text section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    /// Text font size input (6–72).
    Size,
    /// Label text value input.
    Value,
}

/// Selected label / area text typography values.
#[derive(Debug, Clone)]
pub struct TextTools {
    pub font: String,
    pub size: String,
    pub color: Color,
    pub value: String,
}

/// Available label font family options.
pub const TEXT_FONTS: [&str; 6] = [
    "Arial",
    "Calibri",
    "Georgia",
    "Segoe UI",
    "Times New Roman",
    "Verdana",
];

/// Selected area's fill and outline values for the Inspector.
#[derive(Debug, Clone, Copy)]
pub struct AreaTools {
    pub pattern: &'static str,
    pub fill: Color,
    pub line_style: &'static str,
    pub thickness: &'static str,
    pub line_color: Color,
}

const MEASURE_OPTIONS: [&str; 3] = ["Centerline", "Interior Face", "Exterior Face"];
const SUBTRACT_OPTIONS: [&str; 2] = ["Automatic", "None"];

/// True when the Inspector has an active subject (selection or Trace image edit/adjust).
pub fn is_available(app: &App) -> bool {
    !app.selection.is_empty() || trace_image_active(app)
}

fn trace_image_active(app: &App) -> bool {
    app.bg_image.edit_mode || (app.bg_image.loaded() && app.ui.inspector_image_adjust)
}

/// Complete Inspector panel: Wide right dock, Medium slide-over, or Compact bottom sheet.
pub fn panel<'a>(app: &'a App, d: Density, overlay: bool) -> Element<'a, Message> {
    let has_wall = !app.selection.lines.is_empty();
    let has_area = !app.selection.polys.is_empty();
    let has_text = !app.selection.texts.is_empty()
        || !app.selection.area_labels.is_empty()
        || !app.selection.area_sizes.is_empty();
    let has_symbol = !app.selection.symbols.is_empty();
    let has_chain = !app.selection.chains.is_empty();
    let trace_active = trace_image_active(app);

    let kind_count = [has_wall, has_area, has_text, has_symbol, has_chain]
        .into_iter()
        .filter(|&b| b)
        .count();

    let title = inspector_title(
        app,
        kind_count,
        has_wall,
        has_area,
        has_text,
        has_symbol,
        has_chain,
        trace_active,
    );

    let mut header = row![text(title)
        .font(BARLOW_SEMIBOLD)
        .size(TEXT_14)
        .color(tokens::text())
        .width(Length::Fill),]
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

    let subject = if kind_count > 1 {
        multi_kind_section(app, d)
    } else if has_wall {
        wall_section(app, d)
    } else if has_area {
        area_section(app, d)
    } else if has_text {
        text_section(app, d)
    } else if has_symbol {
        symbol_section(app, d)
    } else if has_chain {
        chain_section(app, d)
    } else if trace_active {
        trace_image_section(app, d)
    } else if !app.selection.is_empty() {
        multi_kind_section(app, d)
    } else {
        text("Select a wall, area, label, symbol, or chain dimension.")
            .font(BARLOW)
            .size(TEXT_12)
            .color(tokens::muted())
            .into()
    };
    let mut sections = column![subject].spacing(SPACE_2);
    if kind_count == 1 && (has_wall || has_text || has_symbol) {
        sections = sections
            .push(kit::divider())
            .push(common_actions_section(d));
    }
    if kind_count != 0 && trace_active {
        sections = column![trace_image_section(app, d), kit::divider(), sections].spacing(SPACE_2);
    }

    let scroll = make_scrollable(
        sections
            .padding(Padding {
                top: 0.0,
                right: SPACE_2 + 2.0,
                bottom: 0.0,
                left: 0.0,
            })
            .width(Length::Fill),
    )
    .direction(Direction::Vertical(
        Scrollbar::new().width(5).scroller_width(5),
    ))
    .width(Length::Fill)
    .height(Length::Fill);

    // Start at the top when switching to a different object.
    let selection = &app.selection;
    let subject_ids = [
        &selection.lines,
        &selection.polys,
        &selection.texts,
        &selection.area_labels,
        &selection.area_sizes,
        &selection.symbols,
        &selection.chains,
        &selection.dimensions,
    ]
    .map(|ids| ids.iter().min().copied());
    let inspector_content = keyed_column([((subject_ids, trace_active), scroll.into())])
        .height(Length::Fill)
        .width(Length::Fill);
    let body = column![header, kit::divider(), inspector_content]
        .spacing(SPACE_2)
        .width(Length::Fill)
        .height(Length::Fill);

    let mut box_el = container(body)
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

    if overlay {
        box_el = box_el.width(Length::Fill);
    } else {
        box_el = box_el.width(Length::Fixed(d.inspector_w()));
    }
    box_el.into()
}

#[allow(clippy::too_many_arguments)]
fn inspector_title(
    app: &App,
    kind_count: usize,
    has_wall: bool,
    has_area: bool,
    has_text: bool,
    has_symbol: bool,
    has_chain: bool,
    trace_active: bool,
) -> String {
    if kind_count > 1 {
        return "Several items".to_owned();
    }
    if has_wall {
        let n = app.selection.lines.len();
        return if n == 1 {
            "Wall".to_owned()
        } else {
            format!("Walls ({n})")
        };
    }
    if has_area {
        let n = app.selection.polys.len();
        if n == 1 {
            if let Some(p) = app
                .drawing
                .polygons
                .iter()
                .find(|p| app.selection.polys.contains(&p.id))
            {
                if !p.name.is_empty() {
                    return p.name.clone();
                }
                if !p.kind.is_empty() && p.kind != "UND" {
                    return p.kind.clone();
                }
            }
            return "Area".to_owned();
        }
        return format!("Areas ({n})");
    }
    if has_text {
        return "Text label".to_owned();
    }
    if has_symbol {
        if app.selection.symbols.len() == 1 {
            if let Some(sym) = app
                .drawing
                .symbols
                .iter()
                .find(|s| app.selection.symbols.contains(&s.id))
            {
                if let Some(def) = app.symbols_lib().symbol(&sym.symbol_type) {
                    if !def.label.is_empty() {
                        return def.label.clone();
                    }
                }
                if !sym.symbol_type.is_empty() {
                    return sym.symbol_type.clone();
                }
            }
        }
        return "Symbol".to_owned();
    }
    if has_chain {
        return "Chain dimension".to_owned();
    }
    if trace_active {
        return "Trace image".to_owned();
    }
    "Inspector".to_owned()
}

// ---------------------------------------------------------------------------
// Wall section
// ---------------------------------------------------------------------------

fn wall_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let line = app.drawing.first_selected_line(&app.selection);
    let edit = app.edit_tools_data();
    let handle_note = edit.as_ref().map(|e| e.endpoint_label).unwrap_or("Length");
    let units = &app.drawing.options.units;
    let length_val = app.line_length_edit.clone().unwrap_or_else(|| {
        line.map(|line| {
            let length = distance(line.start, line.end);
            if units.uses_feet_and_inches() {
                units.length(length)
            } else {
                format!("{length:.2}")
            }
        })
        .unwrap_or_default()
    });
    let current_style = line
        .and_then(|l| l.line_style.as_deref())
        .unwrap_or("solid");
    let wall_color = edit
        .as_ref()
        .map_or_else(tokens::accent, |values| values.color);

    let curve_height_mode = app.view.curve_mode == CurveMode::Height;
    let bow_readout = line
        .map(|l| {
            if curve_height_mode {
                units.length(l.arc_height)
            } else {
                format!("{:.0}°", l.bow_angle)
            }
        })
        .unwrap_or_else(|| "0".to_owned());
    let dim_hidden = line.is_some_and(|l| l.dimension_hidden);

    let length_row = row![
        container(kit::field(
            FieldProps {
                label: "",
                value: &length_val,
                placeholder: "10.00",
                on_input: Some(Box::new(Message::LineLengthInput)),
                on_submit: Some(Message::ApplyLineLength),
                focused: false,
            },
            d,
        ))
        .width(Length::Fill),
        kit::text_button("Set length", Some(Message::ApplyLineLength), d),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let style_seg = kit::segmented(
        vec![
            Segment {
                label: "Solid",
                selected: current_style == "solid",
                focused: false,
                on_press: Some(Message::Ui(UiMsg::SetWallLineStyle("solid"))),
            },
            Segment {
                label: "Dashed",
                selected: current_style == "dashed",
                focused: false,
                on_press: Some(Message::Ui(UiMsg::SetWallLineStyle("dashed"))),
            },
            Segment {
                label: "Dotted",
                selected: current_style == "dotted",
                focused: false,
                on_press: Some(Message::Ui(UiMsg::SetWallLineStyle("dotted"))),
            },
        ],
        d,
    );

    let color_row = row![
        text("Line colour")
            .font(BARLOW_MEDIUM)
            .size(TEXT_12)
            .color(tokens::muted())
            .width(Length::Fill),
        kit::swatch(wall_color, false, false, Some(Message::LineColorClicked), d),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let curve_mode_seg = kit::segmented(
        vec![
            Segment {
                label: "Degree",
                selected: !curve_height_mode,
                focused: false,
                on_press: Some(Message::Ui(UiMsg::SetWallCurveMode(CurveMode::Degree))),
            },
            Segment {
                label: "Height",
                selected: curve_height_mode,
                focused: false,
                on_press: Some(Message::Ui(UiMsg::SetWallCurveMode(CurveMode::Height))),
            },
        ],
        d,
    );

    let bow_step = kit::stepper(
        "Bow",
        bow_readout,
        Some(Message::Ui(UiMsg::WallBowStep(-1.0))),
        Some(Message::Ui(UiMsg::WallBowStep(1.0))),
        d,
    );

    let tools_row = row![
        action_btn(
            "bezier-curve",
            "Curve tool",
            app.view.mode == Mode::Curve,
            Some(Message::CurveSelected),
            d,
        ),
        action_btn(
            "plus-circle",
            "Insert vertex",
            app.view.mode == Mode::InsertVertex,
            Some(Message::Key(KeyCmd::InsertVertex)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill);

    let dim_row = row![
        text(if dim_hidden {
            "Dimension: Hidden"
        } else {
            "Dimension: Shown"
        })
        .font(BARLOW_MEDIUM)
        .size(TEXT_12)
        .color(tokens::muted())
        .width(Length::Fill),
        kit::text_button(
            "Toggle dimension",
            Some(Message::Ui(UiMsg::ToggleSelectedDimensions)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    column![
        kit::group_header("DIMENSIONS", None),
        label_caption(handle_note),
        length_row,
        label_caption(if units.uses_feet_and_inches() {
            r#"Feet and inches (10' 6")"#
        } else {
            "Decimal feet (10.50)"
        }),
        dim_row,
        kit::divider(),
        kit::group_header("APPEARANCE", None),
        label_caption("Line style"),
        style_seg,
        color_row,
        kit::divider(),
        kit::group_header("CURVE", None),
        curve_mode_seg,
        bow_step,
        tools_row,
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Area section
// ---------------------------------------------------------------------------

fn area_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let Some(poly) = app
        .drawing
        .polygons
        .iter()
        .find(|p| app.selection.polys.contains(&p.id))
    else {
        return text("Select an area.")
            .font(BARLOW)
            .size(TEXT_12)
            .color(tokens::muted())
            .into();
    };

    let area_tools = app.edit_tools_data().and_then(|e| e.area);

    let dialog_synced = !app.area_dialog.open && app.ui.inspector_area_id == Some(poly.id);
    let name_val = if dialog_synced {
        app.area_dialog.props.name.clone()
    } else {
        poly.name.clone()
    };
    let factor_val = if dialog_synced {
        app.area_dialog.factor_text.clone()
    } else {
        poly.factor_override
            .map(|f| format!("{f}"))
            .unwrap_or_else(|| format!("{}", poly.factor))
    };
    let code_val = if dialog_synced {
        app.area_dialog.props.code.clone()
    } else {
        poly.kind.clone()
    };
    let suffix_val = if dialog_synced {
        app.area_dialog.suffix_text.clone()
    } else {
        poly.auto_post.suffix.clone()
    };
    let post = if dialog_synced {
        app.area_dialog
            .props
            .auto_post
            .clone()
            .unwrap_or_else(|| poly.auto_post.clone())
    } else {
        poly.auto_post.clone()
    };
    let measure_val = if dialog_synced {
        app.area_dialog
            .props
            .measure_to
            .as_deref()
            .unwrap_or(&poly.measure_to)
    } else {
        poly.measure_to.as_str()
    };
    let auto_sub_on = if dialog_synced {
        app.area_dialog.props.auto_subtract
    } else {
        poly.subtract_mode != "never"
    };

    let code_options: Vec<String> = app
        .area_codes
        .iter()
        .map(|c| format!("{} - {}", c.code, c.name))
        .collect();
    let active_code_opt = code_options
        .iter()
        .find(|s| s.starts_with(&format!("{code_val} - ")))
        .cloned();

    let name_row = row![
        kit::with_id(
            "area-name",
            container(kit::field(
                FieldProps {
                    label: "",
                    value: &name_val,
                    placeholder: "Area name",
                    on_input: Some(Box::new(|s| Message::AreaDialog(AreaDialogMsg::Name(s)))),
                    on_submit: Some(Message::AreaDialog(AreaDialogMsg::Apply)),
                    focused: false,
                },
                d,
            ))
            .width(Length::Fill),
        ),
        kit::with_id(
            "area-apply",
            kit::text_button("Apply", Some(Message::AreaDialog(AreaDialogMsg::Apply)), d),
        ),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let type_row = row![
        container(styled_pick_list(
            code_options,
            active_code_opt,
            |v: String| {
                Message::AreaDialog(AreaDialogMsg::Code(
                    v.split(" - ").next().unwrap_or("").to_owned(),
                ))
            },
            d,
        ))
        .width(Length::Fill),
        kit::with_id(
            "area-type-chooser",
            kit::text_button(
                "Chooser",
                Some(Message::AreaWorkflow(WorkflowMsg::Open(poly.id))),
                d,
            ),
        ),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let counts_toward = counts_toward_label(app, &poly.kind);
    let comp = app.computed.get(&poly.id);
    let base_area = poly.gross;
    let total_area = poly.gross * poly.factor;
    let subtracted = comp.map_or(0.0, |c| c.subtracted);
    let net_area = poly.area;
    let perimeter = comp.map_or_else(|| polygon_perimeter(poly), |c| c.perimeter);

    let mut stats = column![
        metric_line("Base area", format!("{base_area:.1} sf")),
        metric_line("Total", format!("{total_area:.1} sf")),
    ]
    .spacing(2);
    if subtracted.abs() > 0.01 {
        stats = stats.push(metric_line("Subtracted", format!("-{subtracted:.1} sf")));
    }
    stats = stats
        .push(metric_line("Net area", format!("{net_area:.1} sf")))
        .push(metric_line(
            "Perimeter",
            app.drawing.options.units.length(perimeter),
        ));

    let factor_row: Element<'a, Message> = kit::field(
        FieldProps {
            label: "",
            value: &factor_val,
            placeholder: "1.0",
            on_input: Some(Box::new(|s| Message::AreaDialog(AreaDialogMsg::Factor(s)))),
            on_submit: Some(Message::AreaDialog(AreaDialogMsg::Apply)),
            focused: false,
        },
        d,
    );

    let positive_sign = poly.factor_override.unwrap_or(poly.factor) >= 0.0;
    let sign_seg = kit::segmented(
        vec![
            Segment {
                label: "Adds (+)",
                selected: positive_sign,
                focused: false,
                on_press: (!positive_sign).then_some(Message::ContextAction(Action::MakeNegative)),
            },
            Segment {
                label: "Subtracts (-)",
                selected: !positive_sign,
                focused: false,
                on_press: positive_sign.then_some(Message::ContextAction(Action::MakeNegative)),
            },
        ],
        d,
    );

    let auto_sub_pick = styled_pick_list(
        SUBTRACT_OPTIONS.iter().map(|s| s.to_string()).collect(),
        Some(if auto_sub_on { "Automatic" } else { "None" }.to_owned()),
        |v| Message::AreaDialog(AreaDialogMsg::AutoSubtract(v)),
        d,
    );

    let mut target_options = vec!["Auto (containing area)".to_owned()];
    for other in &app.drawing.polygons {
        if other.id != poly.id {
            let label = if other.name.is_empty() {
                other.kind.clone()
            } else {
                other.name.clone()
            };
            target_options.push(format!("#{} · {label}", other.id));
        }
    }
    let current_target = poly
        .subtract_target_id
        .and_then(|id| {
            target_options
                .iter()
                .find(|s| s.starts_with(&format!("#{id} ")))
                .cloned()
        })
        .unwrap_or_else(|| "Auto (containing area)".to_owned());
    let deduct_pick = styled_pick_list(
        target_options,
        Some(current_target),
        |v| {
            let parsed = v
                .strip_prefix('#')
                .and_then(|rest| rest.split(' ').next())
                .and_then(|id_str| id_str.parse::<i64>().ok());
            Message::Ui(UiMsg::SetAreaSubtractTarget(parsed))
        },
        d,
    );

    let measure_pick = styled_pick_list(
        MEASURE_OPTIONS.iter().map(|s| s.to_string()).collect(),
        Some(
            match measure_val {
                "interior" => "Interior Face",
                "exterior" => "Exterior Face",
                _ => "Centerline",
            }
            .to_owned(),
        ),
        |v| Message::AreaDialog(AreaDialogMsg::Measure(v)),
        d,
    );

    let appearance = if let Some(tools) = area_tools {
        column![
            row![
                label_caption("Fill"),
                Space::new().width(Length::Fill),
                kit::text_button(
                    tools.pattern,
                    Some(Message::AreaCycle(AreaCycle::FillPattern)),
                    d,
                ),
                kit::swatch(
                    tools.fill,
                    false,
                    false,
                    Some(Message::AreaColorClicked(true)),
                    d
                ),
            ]
            .spacing(SPACE_1)
            .align_y(Alignment::Center)
            .width(Length::Fill),
            row![
                label_caption("Line"),
                Space::new().width(Length::Fill),
                kit::text_button(
                    tools.line_style,
                    Some(Message::AreaCycle(AreaCycle::LineStyle)),
                    d,
                ),
                kit::text_button(
                    tools.thickness,
                    Some(Message::AreaCycle(AreaCycle::LineThickness)),
                    d,
                ),
                kit::swatch(
                    tools.line_color,
                    false,
                    false,
                    Some(Message::AreaColorClicked(false)),
                    d,
                ),
            ]
            .spacing(SPACE_1)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        ]
        .spacing(SPACE_1)
    } else {
        column![]
    };

    let post_grid = column![
        row![
            styled_checkbox("Code", post.code, |v| {
                Message::AreaDialog(AreaDialogMsg::Post(PostSwitch::Code, v))
            }),
            styled_checkbox("Name", post.name, |v| {
                Message::AreaDialog(AreaDialogMsg::Post(PostSwitch::Name, v))
            }),
            styled_checkbox("Dims", post.dims, |v| {
                Message::AreaDialog(AreaDialogMsg::Post(PostSwitch::Dims, v))
            }),
        ]
        .spacing(SPACE_2),
        row![
            styled_checkbox("Math", post.calcs, |v| {
                Message::AreaDialog(AreaDialogMsg::Post(PostSwitch::Calcs, v))
            }),
            styled_checkbox("Base", post.base, |v| {
                Message::AreaDialog(AreaDialogMsg::Post(PostSwitch::Base, v))
            }),
            styled_checkbox("Factor", post.factor, |v| {
                Message::AreaDialog(AreaDialogMsg::Post(PostSwitch::Factor, v))
            }),
        ]
        .spacing(SPACE_2),
    ]
    .spacing(SPACE_1);

    let suffix_row = row![
        label_caption("Suffix"),
        container(kit::field(
            FieldProps {
                label: "",
                value: &suffix_val,
                placeholder: "sf",
                on_input: Some(Box::new(|s| Message::AreaDialog(AreaDialogMsg::Suffix(s)))),
                on_submit: Some(Message::AreaDialog(AreaDialogMsg::Apply)),
                focused: false,
            },
            d,
        ))
        .width(Length::Fill),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let actions = column![
        row![
            action_btn(
                "list-dashes",
                "Attributes",
                false,
                Some(Message::OpenAreaAttributes(true)),
                d,
            ),
            action_btn(
                "selection-background",
                "Auto-Subtract",
                false,
                Some(Message::ContextAction(Action::AutoSubtract)),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "arrows-left-right",
                "Flip H",
                false,
                Some(Message::ContextAction(Action::FlipHorz)),
                d,
            ),
            action_btn(
                "arrows-down-up",
                "Flip V",
                false,
                Some(Message::ContextAction(Action::FlipVert)),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "arrow-clockwise",
                "Rotate 90°",
                false,
                Some(Message::ContextAction(Action::Rotate90)),
                d,
            ),
            action_btn(
                "copy",
                "Clone",
                false,
                Some(Message::ContextAction(Action::CloneArea)),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "arrow-line-up",
                "To front",
                false,
                Some(Message::LayerBringToFront),
                d,
            ),
            action_btn(
                "arrow-line-down",
                "To back",
                false,
                Some(Message::LayerSendToBack),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "pencil-simple",
                "Reopen as walls",
                false,
                Some(Message::AreaWorkflow(WorkflowMsg::Reopen)),
                d,
            ),
            danger_btn("Delete", Message::ContextAction(Action::Delete), d),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill);

    // Every wall of the area, two to a row, so a long outline stays short.
    let mut walls_block = Column::new().spacing(SPACE_1).width(Length::Fill);
    let wall_buttons: Vec<Element<'a, Message>> = poly
        .edges
        .iter()
        .enumerate()
        .map(|(idx, edge)| {
            let next = poly.points.get((idx + 1) % poly.points.len());
            let len = match (poly.points.get(idx), next) {
                (Some(a), Some(b)) => distance(*a, *b),
                _ => 0.0,
            };
            let label = format!(
                "Wall {} · {}",
                idx + 1,
                app.drawing.options.units.length(len)
            );
            let msg = edge.line_id.map(|id| Message::Ui(UiMsg::SelectWall(id)));
            container(kit::text_button(label, msg, d))
                .width(Length::FillPortion(1))
                .into()
        })
        .collect();
    let mut pending: Option<Element<'a, Message>> = None;
    for btn in wall_buttons {
        match pending.take() {
            None => pending = Some(btn),
            Some(first) => {
                walls_block =
                    walls_block.push(row![first, btn].spacing(SPACE_1).width(Length::Fill));
            }
        }
    }
    if let Some(last) = pending {
        walls_block = walls_block.push(
            row![last, Space::new().width(Length::FillPortion(1))]
                .spacing(SPACE_1)
                .width(Length::Fill),
        );
    }

    column![
        label_caption("Name"),
        name_row,
        label_caption("Type"),
        type_row,
        column![
            label_caption("Counts toward"),
            text(counts_toward)
                .font(BARLOW_MEDIUM)
                .size(TEXT_12)
                .color(tokens::text()),
        ]
        .spacing(2),
        stats,
        kit::divider(),
        kit::group_header("EDIT", None),
        actions,
        kit::divider(),
        kit::group_header("CALCULATION", None),
        label_caption("Factor"),
        factor_row,
        sign_seg,
        label_caption("Auto-subtract"),
        auto_sub_pick,
        label_caption("Deduct from"),
        deduct_pick,
        label_caption("Measure to"),
        measure_pick,
        kit::divider(),
        kit::group_header("APPEARANCE", None),
        appearance,
        label_caption("Show on the label"),
        post_grid,
        suffix_row,
        kit::divider(),
        kit::group_header("WALLS", None),
        walls_block,
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Text label section
// ---------------------------------------------------------------------------

fn text_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let tools = app.edit_tools_data().and_then(|e| e.text);
    let value = tools.as_ref().map(|t| t.value.as_str()).unwrap_or("");
    let font = tools.as_ref().map(|t| t.font.as_str()).unwrap_or("Arial");
    let size_text = tools.as_ref().map(|t| t.size.as_str()).unwrap_or("12");
    let color = tools.as_ref().map(|t| t.color).unwrap_or_else(tokens::text);

    let (bold_on, italic_on, under_on) = selected_text_flags(app);

    let value_row = row![
        container(kit::field(
            FieldProps {
                label: "",
                value,
                placeholder: "Label text",
                on_input: Some(Box::new(|s| Message::TextFieldInput(TextField::Value, s))),
                on_submit: Some(Message::TextFieldApply(TextField::Value)),
                focused: false,
            },
            d,
        ))
        .width(Length::Fill),
        kit::text_button("Apply", Some(Message::TextFieldApply(TextField::Value)), d),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let font_options: Vec<String> = TEXT_FONTS.iter().map(|f| f.to_string()).collect();
    let font_pick = styled_pick_list(
        font_options.clone(),
        font_options.into_iter().find(|f| f == font),
        Message::TextFontPicked,
        d,
    );

    let size_row = row![
        container(kit::field(
            FieldProps {
                label: "",
                value: size_text,
                placeholder: "12",
                on_input: Some(Box::new(|s| Message::TextFieldInput(TextField::Size, s))),
                on_submit: Some(Message::TextFieldApply(TextField::Size)),
                focused: false,
            },
            d,
        ))
        .width(Length::Fixed(64.0)),
        kit::stepper(
            "",
            format!("{size_text}pt"),
            Some(Message::AdjustSelectedTextSize(-1)),
            Some(Message::AdjustSelectedTextSize(1)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let style_row = row![
        action_btn(
            "text-b",
            "Bold",
            bold_on,
            Some(Message::ToggleSelectedTextStyle("bold".to_owned())),
            d,
        ),
        action_btn(
            "text-italic",
            "Italic",
            italic_on,
            Some(Message::ToggleSelectedTextStyle("italic".to_owned())),
            d,
        ),
        action_btn(
            "text-underline",
            "Underline",
            under_on,
            Some(Message::ToggleSelectedTextStyle("underline".to_owned())),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill);

    let apply_all_row = row![action_btn(
        "text-aa",
        "Apply to all labels",
        false,
        Some(Message::ApplyTextStyleToAll),
        d,
    )]
    .width(Length::Fill);

    let color_row = row![
        text("Colour")
            .font(BARLOW_MEDIUM)
            .size(TEXT_12)
            .color(tokens::muted())
            .width(Length::Fill),
        kit::swatch(color, false, false, Some(Message::TextColorClicked), d),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let rotate_row = row![
        action_btn(
            "arrow-counter-clockwise",
            "-90°",
            false,
            Some(Message::RotateSelected(-90.0)),
            d,
        ),
        action_btn(
            "arrow-clockwise",
            "+90°",
            false,
            Some(Message::RotateSelected(90.0)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill);

    column![
        kit::group_header("TEXT LABEL", None),
        label_caption("Text"),
        value_row,
        label_caption("Font"),
        font_pick,
        label_caption("Size"),
        size_row,
        style_row,
        color_row,
        apply_all_row,
        label_caption("Rotate"),
        rotate_row,
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Symbol section
// ---------------------------------------------------------------------------

fn symbol_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let sym = app
        .drawing
        .symbols
        .iter()
        .find(|s| app.selection.symbols.contains(&s.id));
    let sym_name = sym
        .map(|s| {
            app.symbols_lib()
                .symbol(&s.symbol_type)
                .map(|def| def.label.as_str())
                .filter(|n| !n.is_empty())
                .unwrap_or(s.symbol_type.as_str())
        })
        .unwrap_or("Symbol");
    let scale_pct = sym.map_or(100.0, |s| s.scale * 100.0);
    let rot_deg = sym.map_or(0.0, |s| s.rotation);
    let hosted = app
        .drawing
        .symbols
        .iter()
        .any(|s| app.selection.symbols.contains(&s.id) && s.wall_id.is_some());
    let gap_text = sym
        .filter(|s| s.wall_id.is_some())
        .map(|s| app.drawing.options.units.length(s.wall_gap_width))
        .unwrap_or_else(|| "-".to_owned());

    let mut col = column![
        kit::group_header("SYMBOL", None),
        metric_line("Name", sym_name.to_owned()),
        metric_line("Rotation", format!("{rot_deg:.0}°")),
        row![
            action_btn(
                "arrow-counter-clockwise",
                "Rotate -90°",
                false,
                Some(Message::RotateSelected(-90.0)),
                d,
            ),
            action_btn(
                "arrow-clockwise",
                "Rotate +90°",
                false,
                Some(Message::RotateSelected(90.0)),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        kit::stepper(
            "Size",
            format!("{scale_pct:.0}%"),
            Some(Message::ScaleSelected(1.0 / 1.08)),
            Some(Message::ScaleSelected(1.08)),
            d,
        ),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill);

    if hosted {
        col = col
            .push(kit::divider())
            .push(kit::group_header("WALL OPENING", None))
            .push(kit::stepper(
                "Opening width",
                gap_text,
                Some(Message::OpeningNarrower),
                Some(Message::OpeningWider),
                d,
            ))
            .push(action_btn(
                "arrows-left-right",
                "Flip swing",
                false,
                Some(Message::FlipOpening),
                d,
            ));
    }

    col.into()
}

// ---------------------------------------------------------------------------
// Chain dimension section
// ---------------------------------------------------------------------------

fn chain_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let chain = app
        .drawing
        .dimension_chains
        .iter()
        .find(|c| app.selection.chains.contains(&c.id));
    let side_label = match chain.map(|c| c.side) {
        Some(s) if s < 0 => "Side B",
        _ => "Side A",
    };
    let offset_text = chain.map_or_else(
        || app.drawing.options.units.length(0.0),
        |c| app.drawing.options.units.length(c.offset),
    );
    let overall_on = chain.is_some_and(|c| c.show_overall);

    column![
        kit::group_header("CHAIN DIMENSION", None),
        row![
            text(format!("Side: {side_label}"))
                .font(BARLOW_MEDIUM)
                .size(TEXT_12)
                .color(tokens::muted())
                .width(Length::Fill),
            kit::text_button("Flip side", Some(Message::Ui(UiMsg::ChainSideFlip)), d),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill),
        kit::stepper(
            "Offset",
            offset_text,
            Some(Message::Ui(UiMsg::ChainOffsetStep(-0.5))),
            Some(Message::Ui(UiMsg::ChainOffsetStep(0.5))),
            d,
        ),
        styled_checkbox("Show overall tier", overall_on, |_| {
            Message::Ui(UiMsg::ChainToggleOverall)
        }),
        danger_btn("Delete chain", Message::Delete, d),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Trace image section
// ---------------------------------------------------------------------------

fn trace_image_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let bg = &app.bg_image;
    let opacity_pct = (bg.opacity * 100.0).round().clamp(0.0, 100.0) as f32;
    let show_msg = (!bg.visible).then_some(Message::PhotoShow);
    let hide_msg = bg.visible.then_some(Message::PhotoHide);

    column![
        kit::group_header("TRACE IMAGE", None),
        row![
            kit::text_button_with("Show", show_msg, d, bg.visible, false),
            kit::text_button_with("Hide", hide_msg, d, !bg.visible, false),
            danger_btn("Remove", Message::PhotoRemove, d),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        action_btn(
            "arrows-out-cardinal",
            if bg.edit_mode {
                "Done moving"
            } else {
                "Move on canvas"
            },
            bg.edit_mode,
            Some(Message::PhotoEditModeToggle),
            d,
        ),
        row![
            action_btn(
                "ruler",
                "Set scale",
                bg.scale_mode,
                Some(Message::PhotoScaleStart),
                d,
            ),
            kit::text_button("Smaller", Some(Message::PhotoSmaller), d),
            kit::text_button("Larger", Some(Message::PhotoLarger), d),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "arrow-counter-clockwise",
                "Rotate L",
                false,
                Some(Message::PhotoRotateLeft),
                d,
            ),
            action_btn(
                "arrow-clockwise",
                "Rotate R",
                false,
                Some(Message::PhotoRotateRight),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "arrows-left-right",
                "Flip H",
                bg.flip_h,
                Some(Message::PhotoFlipH),
                d,
            ),
            action_btn(
                "arrows-down-up",
                "Flip V",
                bg.flip_v,
                Some(Message::PhotoFlipV),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        action_btn(
            "compass",
            "Align guide",
            bg.orient_mode,
            Some(Message::PhotoOrientStart),
            d,
        ),
        row![
            label_caption("Intensity"),
            Space::new().width(Length::Fill),
            text(format!("{:.0}%", opacity_pct))
                .font(BSC_MEDIUM)
                .size(TEXT_12)
                .color(tokens::text()),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill),
        slider(0.0..=100.0, opacity_pct, Message::PhotoIntensity)
            .style(kit::slider_style)
            .width(Length::Fill)
            .step(5.0_f32),
        kit::stepper(
            "Opacity step",
            format!("{:.0}%", opacity_pct),
            Some(Message::PhotoIntensity(
                (opacity_pct - 10.0).clamp(0.0, 100.0)
            )),
            Some(Message::PhotoIntensity(
                (opacity_pct + 10.0).clamp(0.0, 100.0)
            )),
            d,
        ),
        row![
            action_btn(
                "map-trifold",
                "Map options",
                false,
                Some(Message::GeoOpen),
                d
            ),
            kit::text_button("Close", Some(Message::Ui(UiMsg::CloseOverlays)), d),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Multiple kinds / Common actions
// ---------------------------------------------------------------------------

fn multi_kind_section<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let can_scale = !app.selection.symbols.is_empty()
        || !app.selection.texts.is_empty()
        || !app.selection.area_labels.is_empty()
        || !app.selection.area_sizes.is_empty();

    let scale_row = row![
        action_btn(
            "minus",
            "Smaller",
            false,
            can_scale.then_some(Message::ScaleSelected(0.92)),
            d,
        ),
        action_btn(
            "plus",
            "Larger",
            false,
            can_scale.then_some(Message::ScaleSelected(1.08)),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill);

    let nudge_row = row![
        kit::icon_button(
            "arrow-left",
            false,
            Some(Message::Key(KeyCmd::Nudge(-1.0, 0.0))),
            d,
        ),
        kit::icon_button(
            "arrow-up",
            false,
            Some(Message::Key(KeyCmd::Nudge(0.0, 1.0))),
            d,
        ),
        kit::icon_button(
            "arrow-down",
            false,
            Some(Message::Key(KeyCmd::Nudge(0.0, -1.0))),
            d,
        ),
        kit::icon_button(
            "arrow-right",
            false,
            Some(Message::Key(KeyCmd::Nudge(1.0, 0.0))),
            d,
        ),
    ]
    .spacing(SPACE_1)
    .align_y(Alignment::Center);

    column![
        kit::group_header("SELECTION ACTIONS", None),
        row![
            action_btn(
                "arrow-counter-clockwise",
                "Rotate -90°",
                false,
                Some(Message::RotateSelected(-90.0)),
                d,
            ),
            action_btn(
                "arrow-clockwise",
                "Rotate +90°",
                false,
                Some(Message::RotateSelected(90.0)),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        scale_row,
        label_caption("Nudge"),
        nudge_row,
        kit::divider(),
        common_actions_section(d),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .into()
}

fn common_actions_section<'a>(d: Density) -> Element<'a, Message> {
    column![
        kit::group_header("EDIT", None),
        row![
            action_btn("scissors", "Cut", false, Some(Message::Cut), d),
            action_btn("copy", "Copy", false, Some(Message::Copy), d),
            danger_btn("Delete", Message::Delete, d),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
        row![
            action_btn(
                "arrow-line-up",
                "Bring to front",
                false,
                Some(Message::LayerBringToFront),
                d,
            ),
            action_btn(
                "arrow-line-down",
                "Send to back",
                false,
                Some(Message::LayerSendToBack),
                d,
            ),
        ]
        .spacing(SPACE_1)
        .width(Length::Fill),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Helpers & shared controls
// ---------------------------------------------------------------------------

fn selected_text_flags(app: &App) -> (bool, bool, bool) {
    if let Some(t) = app
        .drawing
        .texts
        .iter()
        .find(|t| app.selection.texts.contains(&t.id))
    {
        return (t.bold, t.italic, t.underline);
    }
    if let Some(p) = app.drawing.polygons.iter().find(|p| {
        app.selection.area_labels.contains(&p.id) || app.selection.area_sizes.contains(&p.id)
    }) {
        let opts = &app.drawing.options;
        // An area's label draws its first line bold until Bold is set either
        // way, so that is the state the button shows.
        let default_bold = opts.auto_post_bold || app.selection.area_labels.contains(&p.id);
        return (
            p.label_bold.unwrap_or(default_bold),
            p.label_italic.unwrap_or(false),
            p.label_underline.unwrap_or(false),
        );
    }
    (false, false, false)
}

fn counts_toward_label(app: &App, code: &str) -> String {
    if code.is_empty() || code == "UND" {
        return "Not classified (Needs a type)".to_owned();
    }
    if let Some(meta) = app.area_codes.iter().find(|c| c.code == code) {
        if meta.include_in_gla {
            return "Gross living area (GLA)".to_owned();
        }
        let group = palette_group_label(&meta.group);
        if !group.is_empty() {
            return group.to_owned();
        }
        if !meta.group.is_empty() {
            return meta.group.clone();
        }
    }
    "Area calculation".to_owned()
}

fn polygon_perimeter(poly: &sketch_core::model::Polygon) -> f64 {
    let n = poly.points.len();
    if n < 2 {
        return 0.0;
    }
    (0..n)
        .map(|i| distance(poly.points[i], poly.points[(i + 1) % n]))
        .sum()
}

fn label_caption<'a>(label: &'a str) -> Element<'a, Message> {
    text(label)
        .font(BARLOW_MEDIUM)
        .size(TEXT_11)
        .color(tokens::muted())
        .into()
}

fn metric_line<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label)
            .font(BARLOW)
            .size(TEXT_12)
            .color(tokens::muted())
            .width(Length::Fill),
        text(value)
            .font(BSC_MEDIUM)
            .size(TEXT_12)
            .color(tokens::text()),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn action_btn<'a>(
    icon_name: &'static str,
    label: &'a str,
    active: bool,
    on_press: Option<Message>,
    d: Density,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let fg = if disabled {
        tokens::disabled()
    } else if active {
        tokens::on_accent()
    } else {
        tokens::text()
    };
    let bg = if active && !disabled {
        tokens::accent()
    } else {
        tokens::field()
    };
    let mut btn = button(
        container(
            row![
                if active || disabled {
                    icons::mono(icon_name, 13.0, fg)
                } else {
                    icons::tinted(icon_name, false, 13.0, fg)
                },
                text(label).font(BARLOW_MEDIUM).size(TEXT_12).color(fg),
            ]
            .spacing(SPACE_1)
            .align_y(Alignment::Center),
        )
        .height(Length::Fixed(d.field_h()))
        .padding([0, 8])
        .align_y(Alignment::Center),
    )
    .padding(0)
    .style(move |_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Disabled => tokens::panel(),
            button::Status::Hovered if !active => tokens::hover(),
            _ => bg,
        })),
        border: Border {
            color: if active && !disabled {
                tokens::accent()
            } else {
                tokens::rule()
            },
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        text_color: fg,
        ..button::Style::default()
    });
    if let Some(m) = on_press {
        btn = btn.on_press(m);
    }
    btn.into()
}

fn danger_btn<'a>(label: &'a str, on_press: Message, d: Density) -> Element<'a, Message> {
    let fg = tokens::flag();
    button(
        container(
            row![
                icons::mono("trash", 13.0, fg),
                text(label).font(BARLOW_MEDIUM).size(TEXT_12).color(fg),
            ]
            .spacing(SPACE_1)
            .align_y(Alignment::Center),
        )
        .height(Length::Fixed(d.field_h()))
        .padding([0, 8])
        .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press(on_press)
    .style(|_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => tokens::hover(),
            _ => tokens::field(),
        })),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        text_color: tokens::flag(),
        ..button::Style::default()
    })
    .into()
}

fn styled_pick_list<'a>(
    options: Vec<String>,
    selected: Option<String>,
    on_select: impl Fn(String) -> Message + 'a,
    d: Density,
) -> Element<'a, Message> {
    pick_list(options, selected, on_select)
        .width(Length::Fill)
        .text_size(TEXT_12)
        .font(BARLOW)
        .padding(Padding {
            top: ((d.field_h() - 16.0).max(4.0)) * 0.5,
            bottom: ((d.field_h() - 16.0).max(4.0)) * 0.5,
            left: 8.0,
            right: 8.0,
        })
        .menu_style(|_| iced::overlay::menu::Style {
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
        })
        .style(|_, _| pick_list::Style {
            text_color: tokens::text(),
            placeholder_color: tokens::muted(),
            handle_color: tokens::muted(),
            background: Background::Color(tokens::input_bg()),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_CONTROL.into(),
            },
        })
        .into()
}

fn styled_checkbox<'a>(
    label: &'a str,
    checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    checkbox(checked)
        .label(label)
        .on_toggle(on_toggle)
        .font(BARLOW)
        .text_size(TEXT_12)
        .size(14.0)
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
                    radius: 3.0.into(),
                },
                text_color: Some(tokens::text()),
            }
        })
        .into()
}
