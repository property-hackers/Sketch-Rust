//! The area-definition editor (screen 7, S7-007): `showAreaDefinitionEditor`.
//!
//! Measured from the live original at 1400x860: a 500x358 card at (450,251)
//! with `--bg-panel`, radius 5 and 12px padding. Five rows with the label left
//! and a 336-wide control at x 601: Area name 297, Line color 336, Fill color
//! 376, Fill style 416, Fill opacity 457 (a 286px range plus an `<output>` at
//! x 895 reading "10%"). The preview is 474x62 at (463,497) and Cancel (59.5) /
//! Save Permanently (126.9) sit at y 569.
//!
//! Saving does more than rewrite the table: the original also renames every
//! polygon of that code and repaints the lines of its edges, then reports
//! `X settings saved permanently.`

use iced::widget::{button, container, pick_list, text, text_input, Space};
use iced::{Background, Border, Color, Element, Length, Padding};

use crate::new_code_dialog::{style_label, style_value, FILL_STYLES};
use crate::snap::nudge;
use crate::theme::{bg_app, bg_hover, bg_panel, border, muted};
use crate::theme::{hex, text_main, ARIAL};
use crate::Message;

const CARD: (f32, f32, f32, f32) = (450.0, 251.0, 500.0, 358.0);
/// See `area_dialog::PIN_Y`.
const PIN_Y: f32 = 2.0;

/// The dialog's state.
#[derive(Debug, Clone)]
pub struct DefEditor {
    pub open: bool,
    /// The code being edited. Empty means "no definition" (the original's
    /// "Select an area definition to edit." notice).
    pub code: String,
    pub name: String,
    pub color: String,
    pub fill_color: String,
    pub fill_style: String,
    pub fill_opacity: f32,
}

impl Default for DefEditor {
    fn default() -> Self {
        DefEditor {
            open: false,
            code: String::new(),
            name: String::new(),
            color: "#718096".into(),
            fill_color: "#718096".into(),
            fill_style: "solid".into(),
            fill_opacity: 10.0,
        }
    }
}

/// A control in the dialog.
#[derive(Debug, Clone)]
pub enum DefMsg {
    Name(String),
    FillStyle(String),
    FillOpacity(f32),
    Save,
    Cancel,
}

impl DefEditor {
    /// Open on one catalogue entry.
    pub fn open_on(entry: &sketch_core::geometry::AreaCode) -> DefEditor {
        DefEditor {
            open: true,
            code: entry.code.clone(),
            name: entry.name.clone(),
            color: if entry.color.is_empty() {
                "#718096".to_string()
            } else {
                entry.color.clone()
            },
            fill_color: if entry.fill_color.is_empty() {
                entry.color.clone()
            } else {
                entry.fill_color.clone()
            },
            fill_style: if entry.fill_style.is_empty() {
                "solid".to_string()
            } else {
                entry.fill_style.clone()
            },
            fill_opacity: entry.fill_opacity as f32,
        }
    }

    pub fn update(&mut self, msg: DefMsg) {
        match msg {
            DefMsg::Name(v) => self.name = v,
            DefMsg::FillStyle(v) => self.fill_style = v,
            DefMsg::FillOpacity(v) => self.fill_opacity = v,
            DefMsg::Save | DefMsg::Cancel => {}
        }
    }

    /// Apply the form to the catalogue (`setAreaCodes`).
    pub fn apply(&self, codes: &mut [sketch_core::geometry::AreaCode]) {
        let name = self.name.trim().to_string();
        for entry in codes.iter_mut().filter(|c| c.code == self.code) {
            if !name.is_empty() {
                entry.name = name.clone();
            }
            entry.color = self.color.clone();
            entry.fill_color = self.fill_color.clone();
            entry.fill_style = self.fill_style.clone();
            entry.fill_opacity = f64::from(self.fill_opacity);
        }
    }

    /// The original also renames the polygons of that code and repaints the
    /// lines of their edges. Returns how many polygons were renamed.
    pub fn rename_polygons(
        &self,
        polygons: &mut [sketch_core::model::Polygon],
        lines: &mut [sketch_core::model::SketchLine],
    ) -> usize {
        let name = self.name.trim().to_string();
        let mut touched = 0usize;
        for poly in polygons.iter_mut().filter(|p| p.kind == self.code) {
            if !name.is_empty() {
                poly.name = name.clone();
            }
            for edge in poly.edges.iter() {
                if let Some(line) = lines.iter_mut().find(|l| Some(l.id) == edge.line_id) {
                    line.color = Some(self.color.clone());
                }
            }
            touched += 1;
        }
        touched
    }
}

fn place<'a>(x: f32, y: f32, el: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    crate::theme::at(x - CARD.0, y - CARD.1 + PIN_Y, el)
}

/// The dialog's layer, or None when closed.
pub fn layer<'a>(d: &'a DefEditor) -> Option<Element<'a, Message>> {
    if !d.open {
        return None;
    }
    let send = |m: DefMsg| Message::DefEditor(m);
    let mut body = iced::widget::stack![]
        .width(Length::Fill)
        .height(Length::Fill);

    body = body.push(place(
        463.0,
        263.0,
        nudge(
            0.5,
            0.0,
            text(format!("Edit {}", d.code))
                .font(ARIAL)
                .size(16.0)
                .color(text_main()),
        ),
    ));
    body = body.push(place(
        928.0,
        261.0,
        button(text("\u{d7}").font(ARIAL).size(16.0).color(muted()))
            .on_press(send(DefMsg::Cancel))
            .padding(0)
            .width(12.0)
            .height(23.0)
            .style(|_, _| button::Style {
                background: None,
                border: Border::default(),
                text_color: muted(),
                ..button::Style::default()
            }),
    ));
    // Area name.
    body = body.push(place(
        463.0,
        306.0,
        nudge(
            0.5,
            0.0,
            text("Area name").font(ARIAL).size(11.0).color(text_main()),
        ),
    ));
    body = body.push(place(
        601.0,
        297.0,
        text_input("", &d.name)
            .on_input(|v| Message::DefEditor(DefMsg::Name(v)))
            .font(ARIAL)
            .size(13.33)
            .padding(Padding {
                top: 4.0,
                bottom: 4.0,
                left: 8.0,
                right: 8.0,
            })
            .width(336.0)
            .style(|_, _| iced::widget::text_input::Style {
                background: Background::Color(bg_app()),
                border: Border {
                    color: border(),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                icon: muted(),
                placeholder: muted(),
                value: text_main(),
                selection: crate::dialog::SELECTION,
            }),
    ));
    // The two colour swatches, 5px above their labels (see the 7g note).
    for (label, y, color) in [
        ("Line color", 336.0, &d.color),
        ("Fill color", 376.0, &d.fill_color),
    ] {
        body = body.push(place(
            463.0,
            y + 6.0,
            nudge(
                0.5,
                0.0,
                text(label).font(ARIAL).size(11.0).color(text_main()),
            ),
        ));
        body = body.push(place(
            601.0,
            y - 1.0,
            container(Space::new())
                .width(32.0)
                .height(24.0)
                .style(move |_| container::Style {
                    background: Some(Background::Color(
                        crate::render::parse_hex(color).unwrap_or(hex(0x718096)),
                    )),
                    border: Border {
                        color: border(),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..container::Style::default()
                }),
        ));
    }
    // Fill style.
    body = body.push(place(
        463.0,
        425.0,
        nudge(
            0.5,
            0.0,
            text("Fill style").font(ARIAL).size(11.0).color(text_main()),
        ),
    ));
    body = body.push(place(
        601.0,
        416.0,
        pick_list(
            FILL_STYLES
                .iter()
                .map(|s| style_label(s).to_string())
                .collect::<Vec<_>>(),
            Some(style_label(&d.fill_style).to_string()),
            |v: String| Message::DefEditor(DefMsg::FillStyle(style_value(&v).to_string())),
        )
        .width(336.0)
        .text_size(13.0)
        .font(ARIAL)
        .style(|_, _| pick_list::Style {
            text_color: text_main(),
            placeholder_color: muted(),
            handle_color: muted(),
            background: Background::Color(bg_app()),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 2.0.into(),
            },
        }),
    ));
    // Fill opacity plus its `<output>`.
    body = body.push(place(
        463.0,
        466.0,
        nudge(
            0.5,
            0.0,
            text("Fill opacity")
                .font(ARIAL)
                .size(11.0)
                .color(text_main()),
        ),
    ));
    body = body.push(place(
        601.0,
        456.0,
        iced::widget::slider(0.0..=100.0, d.fill_opacity, |v| {
            Message::DefEditor(DefMsg::FillOpacity(v))
        })
        .width(286.0)
        .step(1.0_f32),
    ));
    body = body.push(place(
        895.0,
        462.0,
        nudge(
            0.5,
            0.0,
            text(format!("{}%", d.fill_opacity.round() as i32))
                .font(ARIAL)
                .size(11.0)
                .color(text_main()),
        ),
    ));
    // The style preview.
    body = body.push(place(
        463.0,
        497.0,
        container(
            container(nudge(
                0.5,
                0.0,
                text("Area style preview")
                    .font(ARIAL)
                    .size(11.0)
                    .color(muted()),
            ))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        )
        .width(474.0)
        .height(62.0)
        .style(move |_| container::Style {
            background: Some(Background::Color(
                crate::render::parse_hex(&d.fill_color)
                    .map(|c| Color {
                        a: d.fill_opacity / 100.0,
                        ..c
                    })
                    .unwrap_or(hex(0x718096)),
            )),
            border: Border {
                color: crate::render::parse_hex(&d.color).unwrap_or(border()),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        }),
    ));
    body = body.push(place(
        742.6,
        569.0,
        dialog_button("Cancel", 59.5, DefMsg::Cancel),
    ));
    body = body.push(place(
        810.1,
        569.0,
        dialog_button("Save Permanently", 126.9, DefMsg::Save),
    ));

    let card = container(body)
        .width(CARD.2)
        .height(CARD.3)
        .style(|_| container::Style {
            background: Some(Background::Color(bg_panel())),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 5.0.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                offset: iced::Vector::new(0.0, 16.0),
                blur_radius: 23.0,
            },
            ..container::Style::default()
        });
    let backdrop = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.46))),
            ..container::Style::default()
        });
    Some(
        iced::widget::stack![
            iced::widget::opaque(iced::widget::mouse_area(backdrop).on_press(send(DefMsg::Cancel))),
            crate::theme::at(CARD.0, CARD.1, card)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
    )
}

fn dialog_button<'a>(label: &'a str, width: f32, msg: DefMsg) -> Element<'a, Message> {
    button(text(label).font(ARIAL).size(13.33).color(text_main()))
        .on_press(Message::DefEditor(msg))
        .padding(Padding {
            top: 5.0,
            bottom: 5.0,
            left: 10.0,
            right: 10.0,
        })
        .width(width)
        .height(27.0)
        .style(|_, _| button::Style {
            background: Some(Background::Color(bg_hover())),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 2.0.into(),
            },
            text_color: text_main(),
            ..button::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::geometry::area_codes::default_area_codes;
    use sketch_core::geometry::AreaCode;
    use sketch_core::geometry::{Edge, Pt};
    use sketch_core::model::{Polygon, SketchLine, WallType};

    #[test]
    fn it_opens_on_the_entrys_own_values() {
        let codes = default_area_codes();
        let gla = codes.iter().find(|c| c.code == "GLA1").expect("GLA1");
        let d = DefEditor::open_on(gla);
        assert!(d.open);
        assert_eq!(d.code, "GLA1");
        assert_eq!(d.name, gla.name);
        assert_eq!(d.fill_opacity, gla.fill_opacity as f32);
    }

    #[test]
    fn saving_rewrites_only_that_code() {
        let mut codes = default_area_codes();
        let mut d = DefEditor::open_on(codes.iter().find(|c| c.code == "GLA1").expect("GLA1"));
        d.update(DefMsg::Name(" Main Level ".into()));
        d.update(DefMsg::FillStyle("crosshatch".into()));
        d.update(DefMsg::FillOpacity(40.0));
        d.apply(&mut codes);
        let gla = codes.iter().find(|c| c.code == "GLA1").expect("GLA1");
        assert_eq!(gla.name, "Main Level", "trimmed");
        assert_eq!(gla.fill_style, "crosshatch");
        assert_eq!(gla.fill_opacity, 40.0);
        // The other codes keep their names.
        assert_eq!(
            codes.iter().find(|c| c.code == "GLA2").expect("GLA2").name,
            default_area_codes()
                .iter()
                .find(|c| c.code == "GLA2")
                .expect("GLA2")
                .name
        );
    }

    #[test]
    fn saving_renames_the_polygons_and_repaints_their_edges() {
        let codes = default_area_codes();
        let mut d = DefEditor::open_on(codes.iter().find(|c| c.code == "GLA1").expect("GLA1"));
        d.update(DefMsg::Name("Main Level".into()));
        d.color = "#FF0000".to_string();
        let mut polygons = vec![
            Polygon {
                id: 1,
                points: vec![],
                edges: vec![Edge {
                    line_id: Some(10),
                    bow_angle: 0.0,
                }],
                kind: "GLA1".into(),
                name: "Old".into(),
                ..sample_polygon()
            },
            Polygon {
                id: 2,
                points: vec![],
                edges: vec![Edge {
                    line_id: Some(11),
                    bow_angle: 0.0,
                }],
                kind: "GAR".into(),
                name: "Garage".into(),
                ..sample_polygon()
            },
        ];
        let mut lines = vec![
            SketchLine {
                id: 10,
                color: Some("#111111".into()),
                ..sample_line()
            },
            SketchLine {
                id: 11,
                color: Some("#222222".into()),
                ..sample_line()
            },
        ];
        let touched = d.rename_polygons(&mut polygons, &mut lines);
        assert_eq!(touched, 1, "only the GLA1 polygon");
        assert_eq!(polygons[0].name, "Main Level");
        assert_eq!(
            lines[0].color.as_deref(),
            Some("#FF0000"),
            "its edge line was repainted"
        );
        assert_eq!(polygons[1].name, "Garage", "the GAR polygon is untouched");
        assert_eq!(
            lines[1].color.as_deref(),
            Some("#222222"),
            "and so is its line"
        );
    }

    fn sample_polygon() -> Polygon {
        Polygon {
            id: 0,
            points: vec![],
            edges: vec![],
            kind: "UND".into(),
            name: String::new(),
            gross: 0.0,
            area: 0.0,
            factor_override: None,
            factor: 1.0,
            measure_to: "center".into(),
            subtract_mode: "auto".into(),
            subtract_target_id: None,
            auto_post: Default::default(),
            attributes: Default::default(),
            layer_id: None,
            fill_color: None,
            fill_pattern: None,
            fill_opacity: None,
            line_color: None,
            line_style: None,
            line_thickness: None,
            label_rotation: None,
            area_size_rotation: None,
            label_position: None,
            area_size_position: None,
            label_scale: None,
            area_size_scale: None,
            label_font_size: None,
            label_font_family: None,
            label_bold: None,
            label_italic: None,
            label_underline: None,
            label_color: None,
            extra: Default::default(),
        }
    }
    fn sample_line() -> SketchLine {
        SketchLine {
            id: 0,
            start: Pt::new(0.0, 0.0),
            end: Pt::new(1.0, 1.0),
            kind: WallType::Exterior,
            color: None,
            line_thickness: None,
            thickness: None,
            dimension_hidden: false,
            layer_id: None,
            bow_angle: 0.0,
            arc_height: 0.0,
            line_style: None,
            dimension_position: None,
            dimension_anchor: None,
            extra: Default::default(),
        }
    }
    #[test]
    fn the_codes_default_fill_style_is_solid_when_empty() {
        let mut entry: AreaCode = sketch_core::geometry::AreaCode {
            code: "X".into(),
            name: "Y".into(),
            ..default_area_codes()
                .into_iter()
                .find(|c| c.code == "OTH")
                .expect("OTH")
        };
        entry.code = "X".into();
        entry.color = String::new();
        entry.fill_color = String::new();
        entry.fill_style = String::new();
        let d = DefEditor::open_on(&entry);
        assert_eq!(d.fill_style, "solid");
        assert_eq!(
            d.fill_color, "",
            "an empty fill falls back to the line colour"
        );
        assert_eq!(
            d.color, "#718096",
            "and an empty line colour to the default"
        );
    }
}
