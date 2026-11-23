//! Responsive "What is this area?" Area Chooser (`01-design-spec.md` §5.8, P9).
//!
//! Replaces classic `area_workflow::chooser_layer` in `Shell::Next` while keeping
//! all `area_workflow::Msg` messages unchanged. Lists all area types when search
//! is empty (`group == "ALL"`), offers group filter chips, name/label switches,
//! "Use for the next areas too", and lets Enter apply the first match.

use std::collections::HashSet;

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
    checkbox, column, container, mouse_area, opaque, row, scrollable, text, text_input, Column,
    Row, Space,
};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use crate::area_workflow::{Msg as WorkflowMsg, Post};
use crate::dock;
use crate::render::parse_hex;
use crate::ui::fonts::{BARLOW, BARLOW_SEMIBOLD, BSC_SEMIBOLD};
use crate::ui::kit;
use crate::ui::layout::SizeClass;
use crate::ui::tokens::{
    self, Density, FOCUS_RING_WIDTH, RADIUS_CONTROL, RADIUS_SHEET, SPACE_1, SPACE_2, SPACE_3,
    TEXT_11, TEXT_12, TEXT_13, TEXT_16,
};
use crate::{App, Message};

fn send(m: WorkflowMsg) -> Message {
    Message::AreaWorkflow(m)
}

/// Overlay layer for the Area Chooser in `Shell::Next`.
pub fn layer<'a>(app: &'a App) -> Option<Element<'a, Message>> {
    let c = app.workflow.chooser.as_ref()?;
    let sc = SizeClass::of(app.window_size.width);
    let d = app.ui.density;
    let all = c.codes(&app.area_codes);

    let title_str = if c.existing {
        "Edit area"
    } else {
        "What is this area?"
    };

    let header = row![
        text(title_str)
            .font(BARLOW_SEMIBOLD)
            .size(TEXT_16)
            .color(tokens::text()),
        Space::new().width(SPACE_2),
        container(
            text(format!("{:.1} sf", c.area))
                .font(BSC_SEMIBOLD)
                .size(TEXT_12)
                .color(tokens::accent()),
        )
        .padding([2, 8])
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::accent_soft())),
            border: Border {
                radius: RADIUS_CONTROL.into(),
                ..Border::default()
            },
            ..container::Style::default()
        }),
        Space::new().width(Length::Fill),
        kit::icon_button("x", false, Some(send(WorkflowMsg::Cancel)), d),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let search = text_input("Search area types or codes…", &c.search)
        .id("area-definition-search")
        .on_input(|s| send(WorkflowMsg::Search(s)))
        .on_submit(send(WorkflowMsg::Submit))
        .size(TEXT_13)
        .font(BARLOW)
        .padding([7, 10])
        .width(Length::Fill)
        .style(input_style);

    let card_w = match sc {
        SizeClass::Compact => (app.window_size.width - SPACE_3 * 2.0).max(260.0),
        SizeClass::Medium | SizeClass::Wide => 420.0,
    };
    let inner_w = card_w - SPACE_3 * 2.0;

    let group_chips = build_group_chips(&all, c, inner_w, d);

    let matching: Vec<_> = all.iter().filter(|a| c.matches(a)).collect();
    let effective_code = c
        .code
        .clone()
        .filter(|code| matching.iter().any(|a| &a.code == code))
        .or_else(|| matching.first().map(|a| a.code.clone()));

    let mut type_col = Column::new().spacing(SPACE_1).width(Length::Fill);
    if matching.is_empty() {
        type_col = type_col.push(
            container(
                text("No matching area types.")
                    .font(BARLOW)
                    .size(TEXT_12)
                    .color(tokens::muted()),
            )
            .padding([SPACE_2, 0.0]),
        );
    } else {
        for area_code in &matching {
            let selected = effective_code.as_deref() == Some(area_code.code.as_str());
            let swatch_color = parse_hex(&area_code.color)
                .or_else(|| parse_hex(&area_code.fill))
                .unwrap_or_else(tokens::accent);
            type_col = type_col.push(kit::list_row_owned(
                kit::OwnedListRowProps {
                    swatch: Some(swatch_color),
                    title: area_code.name.clone(),
                    value: Some(area_code.code.clone()),
                    selected,
                    flagged: false,
                    focused: false,
                    on_press: Some(send(WorkflowMsg::Code(area_code.code.clone()))),
                },
                d,
            ));
        }
    }

    let list_scroll = scrollable(
        container(type_col)
            .padding(Padding {
                right: 12.0,
                ..Padding::ZERO
            })
            .width(Length::Fill),
    )
    // The "-list" suffix marks a list of data that may scroll
    // (`e2e_next::dialogs_fit_1920x1080_without_scrolling`).
    .id("area-chooser-scope-list")
    .direction(Direction::Vertical(
        Scrollbar::new().width(5).scroller_width(5),
    ))
    .width(Length::Fill)
    .height(Length::Fill);

    let name_input = text_input("Optional area name…", &c.name)
        .id("area-chooser-name")
        .on_input(|s| send(WorkflowMsg::Name(s)))
        .on_submit(send(WorkflowMsg::Submit))
        .size(TEXT_12)
        .font(BARLOW)
        .padding([6, 10])
        .width(Length::Fill)
        .style(input_style);

    let labels_btn = kit::text_button_with(
        if c.labels {
            "Label switches ▾"
        } else {
            "Label switches ▸"
        },
        Some(send(WorkflowMsg::Labels)),
        d,
        c.labels,
        false,
    );

    let repeat_cb = checkbox(c.repeat)
        .label("Use for the next areas too")
        .on_toggle(|v| send(WorkflowMsg::Repeat(v)))
        .size(16)
        .text_size(TEXT_12);

    let apply_label = if c.existing { "Save" } else { "Apply" };
    let footer = row![
        repeat_cb,
        Space::new().width(Length::Fill),
        kit::with_id(
            "area-chooser-cancel",
            kit::text_button("Cancel", Some(send(WorkflowMsg::Cancel)), d),
        ),
        kit::with_id(
            "area-chooser-apply",
            kit::primary_button(
                apply_label,
                effective_code
                    .is_some()
                    .then_some(send(WorkflowMsg::Submit)),
                d,
            ),
        ),
    ]
    .spacing(SPACE_2)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let mut content = column![
        header,
        search,
        group_chips,
        kit::divider(),
        list_scroll,
        kit::divider(),
        name_input,
        labels_btn,
    ]
    .spacing(SPACE_2)
    .width(Length::Fill)
    .height(Length::Fill);

    if c.labels {
        content = content.push(label_switches(c, d));
    }
    content = content.push(footer);

    let sheet: Element<'a, Message> = match sc {
        SizeClass::Compact => opaque(
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(SPACE_3)
                .style(|_| container::Style {
                    background: Some(Background::Color(tokens::panel())),
                    ..container::Style::default()
                }),
        ),
        SizeClass::Medium | SizeClass::Wide => {
            let card_h = (app.window_size.height - 64.0).clamp(360.0, 580.0);
            let card = opaque(
                container(content)
                    .width(Length::Fixed(card_w))
                    .height(Length::Fixed(card_h))
                    .padding(SPACE_3)
                    .style(|_| container::Style {
                        background: Some(Background::Color(tokens::panel())),
                        border: Border {
                            color: tokens::rule(),
                            width: 1.0,
                            radius: RADIUS_SHEET.into(),
                        },
                        shadow: tokens::floating_shadow(),
                        ..container::Style::default()
                    }),
            );
            iced::widget::stack![
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(Background::Color(tokens::scrim())),
                            ..container::Style::default()
                        }),
                )
                .on_press(send(WorkflowMsg::Cancel)),
                container(card)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    };

    Some(sheet)
}

fn build_group_chips<'a>(
    all: &[sketch_core::geometry::AreaCode],
    c: &crate::area_workflow::Chooser,
    inner_w: f32,
    d: Density,
) -> Element<'a, Message> {
    let mut entries: Vec<(String, String)> = vec![("ALL".into(), "All".into())];
    let mut seen = HashSet::new();
    for code in all {
        if seen.insert(code.group.clone()) {
            let label = match dock::palette_group_label(&code.group) {
                "" => code.group.clone(),
                l => l.to_string(),
            };
            entries.push((code.group.clone(), label));
        }
    }

    let mut rows_col = Column::new().spacing(SPACE_1).width(Length::Fill);
    let mut current_row = Row::new().spacing(SPACE_1).align_y(Alignment::Center);
    let mut used = 0.0_f32;
    let mut has_item = false;

    for (key, label) in entries {
        let est_w = label.chars().count() as f32 * 7.0 + 24.0;
        if has_item && used + est_w > inner_w {
            rows_col = rows_col.push(current_row);
            current_row = Row::new().spacing(SPACE_1).align_y(Alignment::Center);
            used = 0.0;
        }
        let selected = c.search.trim().is_empty() && c.group == key;
        current_row = current_row.push(kit::chip(
            label,
            selected,
            Some(send(WorkflowMsg::ChooseGroup(key))),
            d,
        ));
        used += est_w + SPACE_1;
        has_item = true;
    }
    if has_item {
        rows_col = rows_col.push(current_row);
    }

    rows_col.into()
}

fn label_switches<'a>(c: &crate::area_workflow::Chooser, d: Density) -> Element<'a, Message> {
    let switches = row![
        kit::chip(
            "Code",
            c.post.code,
            Some(send(WorkflowMsg::Post(Post::Code, !c.post.code))),
            d
        ),
        kit::chip(
            "Name",
            c.post.name,
            Some(send(WorkflowMsg::Post(Post::Name, !c.post.name))),
            d
        ),
        kit::chip(
            "Dimensions",
            c.post.dims,
            Some(send(WorkflowMsg::Post(Post::Dims, !c.post.dims))),
            d
        ),
    ]
    .spacing(SPACE_1);

    let switches2 = row![
        kit::chip(
            "Math",
            c.post.calcs,
            Some(send(WorkflowMsg::Post(Post::Calcs, !c.post.calcs))),
            d
        ),
        kit::chip(
            "Base",
            c.post.base,
            Some(send(WorkflowMsg::Post(Post::Base, !c.post.base))),
            d
        ),
        kit::chip(
            "Factor",
            c.post.factor,
            Some(send(WorkflowMsg::Post(Post::Factor, !c.post.factor))),
            d
        ),
    ]
    .spacing(SPACE_1);

    let suffix = text_input("Suffix (e.g. sf)…", &c.post.suffix)
        .on_input(|s| send(WorkflowMsg::Suffix(s)))
        .size(TEXT_11)
        .font(BARLOW)
        .padding([4, 8])
        .width(Length::Fill)
        .style(input_style);

    column![switches, switches2, suffix]
        .spacing(SPACE_1)
        .width(Length::Fill)
        .into()
}

fn input_style(_: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: Background::Color(tokens::input_bg()),
        border: Border {
            color: if focused {
                tokens::accent()
            } else {
                tokens::rule()
            },
            width: if focused { FOCUS_RING_WIDTH } else { 1.0 },
            radius: RADIUS_CONTROL.into(),
        },
        icon: tokens::muted(),
        placeholder: tokens::muted(),
        value: tokens::text(),
        selection: tokens::accent_soft(),
    }
}
