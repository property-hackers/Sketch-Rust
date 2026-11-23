//! Start screen (`Shell::Next`, spec §5.10).
//!
//! Shown at launch when no file was passed on the command line and nothing is
//! drawn yet (unless "Show this screen when Sketch Rust starts" is unticked),
//! or when reopened via Menu/Commands. Left column: "Start a sketch"
//! actions + Appraise-It Pro status; right column: recent sketch cards with
//! cached outline thumbnails from their first page (or a clean placeholder).
//! Zero `theme::at`, `theme::ink`, or `INK_PASSES`.

use std::collections::HashMap;

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{button, column, container, opaque, row, scrollable, text, Canvas, Space};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme,
};

use crate::recent::{display_time, RecentFile};
use crate::ui::fonts::{BARLOW, BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_MEDIUM};
use crate::ui::kit;
use crate::ui::layout::SizeClass;
use crate::ui::tokens::{
    self, Density, RADIUS_CONTROL, RADIUS_TILE, SPACE_1, SPACE_2, SPACE_3, SPACE_4, TEXT_11,
    TEXT_12, TEXT_13, TEXT_14, TEXT_16,
};
use crate::{App, Message};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ReportStatus {
    #[default]
    Idle,
    Checking,
    Open(String),
    NoReport,
    Unavailable(String),
}

#[derive(Debug, Clone, Default)]
pub struct RecentThumb {
    pub lines: Vec<((f32, f32), (f32, f32))>,
    pub polys: Vec<Vec<(f32, f32)>>,
    pub living_sf: Option<f64>,
    pub has_geometry: bool,
}

#[derive(Debug, Clone, Default)]
pub struct StartState {
    pub open: bool,
    /// "Show this screen when Sketch Rust starts" unticked (shown by default).
    pub hide_at_launch: bool,
    pub report: ReportStatus,
    thumbs: HashMap<String, RecentThumb>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Open,
    Close,
    NewSketch,
    OpenFile,
    ImportCad,
    FromPhoto,
    OpenRecent(usize),
    ClearRecent,
    CheckReport,
    ReportChecked(ReportStatus),
    DrawForReport,
    ShowAtLaunch(bool),
}

/// `%APPDATA%\SketchRust\start-screen.json`.
const PREFS_FILE: &str = "start-screen.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct StartPrefs {
    #[serde(rename = "showAtLaunch")]
    show_at_launch: bool,
}

/// The stored choice; a missing or unreadable file means "show it".
pub fn load_hide_at_launch() -> bool {
    crate::persist::read_json::<StartPrefs>(PREFS_FILE).is_some_and(|p| !p.show_at_launch)
}

fn m(msg: Msg) -> Message {
    Message::Ui(crate::ui::Msg::Start(msg))
}

impl StartState {
    pub fn sync_thumbs(&mut self, recent: &[RecentFile]) {
        let mut keep = HashMap::new();
        for rec in recent {
            let key = cache_key(rec);
            let thumb = self
                .thumbs
                .remove(&key)
                .unwrap_or_else(|| parse_thumb(&rec.data));
            keep.insert(key, thumb);
        }
        self.thumbs = keep;
    }

    pub fn thumb_for(&self, rec: &RecentFile) -> RecentThumb {
        self.thumbs
            .get(&cache_key(rec))
            .cloned()
            .unwrap_or_default()
    }
}

fn cache_key(rec: &RecentFile) -> String {
    format!("{}\0{}\0{}", rec.name, rec.last_opened, rec.data.len())
}

fn parse_thumb(raw: &str) -> RecentThumb {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return RecentThumb::default();
    };
    let page = val
        .get("pages")
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .unwrap_or(&val);

    let mut raw_lines: Vec<((f64, f64), (f64, f64))> = Vec::new();
    if let Some(lines) = page.get("lines").and_then(|v| v.as_array()) {
        for l in lines {
            let x1 = l.get("x1").and_then(|v| v.as_f64());
            let y1 = l.get("y1").and_then(|v| v.as_f64());
            let x2 = l.get("x2").and_then(|v| v.as_f64());
            let y2 = l.get("y2").and_then(|v| v.as_f64());
            if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (x1, y1, x2, y2) {
                if x1.is_finite() && y1.is_finite() && x2.is_finite() && y2.is_finite() {
                    raw_lines.push(((x1, y1), (x2, y2)));
                }
            }
        }
    }

    let mut raw_polys: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut living_sf = 0.0_f64;
    let mut has_gla_data = false;
    let codes = sketch_core::geometry::area_codes::default_area_codes();
    if let Some(polys) = page.get("polygons").and_then(|v| v.as_array()) {
        for p in polys {
            let mut ring = Vec::new();
            if let Some(pts) = p.get("points").and_then(|v| v.as_array()) {
                for pt in pts {
                    if let (Some(x), Some(y)) = (
                        pt.get("x").and_then(|v| v.as_f64()),
                        pt.get("y").and_then(|v| v.as_f64()),
                    ) {
                        if x.is_finite() && y.is_finite() {
                            ring.push((x, y));
                        }
                    }
                }
            }
            if ring.len() >= 3 {
                let kind = p
                    .get("type")
                    .or_else(|| p.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let counts_gla = codes
                    .iter()
                    .find(|c| c.code == kind)
                    .is_some_and(|c| c.include_in_gla);
                let area = p
                    .get("area")
                    .or_else(|| p.get("netArea"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| shoelace(&ring));
                if counts_gla && area > 0.0 {
                    living_sf += area;
                    has_gla_data = true;
                }
                raw_polys.push(ring);
            }
        }
    }

    if raw_lines.is_empty() && raw_polys.is_empty() {
        return RecentThumb {
            living_sf: has_gla_data.then_some(living_sf),
            ..RecentThumb::default()
        };
    }

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &((x1, y1), (x2, y2)) in &raw_lines {
        min_x = min_x.min(x1).min(x2);
        max_x = max_x.max(x1).max(x2);
        min_y = min_y.min(y1).min(y2);
        max_y = max_y.max(y1).max(y2);
    }
    for ring in &raw_polys {
        for &(x, y) in ring {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }

    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    let span = span_x.max(span_y);
    let ox = (span - span_x) * 0.5;
    let oy = (span - span_y) * 0.5;
    let norm = |(x, y): (f64, f64)| -> (f32, f32) {
        (
            ((x - min_x + ox) / span) as f32,
            ((y - min_y + oy) / span) as f32,
        )
    };

    RecentThumb {
        lines: raw_lines
            .into_iter()
            .map(|(a, b)| (norm(a), norm(b)))
            .collect(),
        polys: raw_polys
            .into_iter()
            .map(|r| r.into_iter().map(norm).collect())
            .collect(),
        living_sf: has_gla_data.then_some(living_sf),
        has_geometry: true,
    }
}

fn shoelace(pts: &[(f64, f64)]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        sum += x1 * y2 - x2 * y1;
    }
    (sum * 0.5).abs()
}

pub fn update(app: &mut App, msg: Msg) -> iced::Task<Message> {
    match msg {
        Msg::Open => {
            app.ui.close_all();
            app.ui.start.sync_thumbs(&app.recent);
            app.ui.start.open = true;
            #[cfg(windows)]
            if std::env::var_os("SKETCH_PERSIST").is_some() {
                app.ui.start.report = ReportStatus::Checking;
                return check_report_task();
            }
            iced::Task::none()
        }
        Msg::Close | Msg::NewSketch => {
            app.ui.start.open = false;
            iced::Task::none()
        }
        Msg::OpenFile => {
            app.ui.start.open = false;
            app.update(Message::OpenFile)
        }
        Msg::ImportCad => {
            app.ui.start.open = false;
            app.update(Message::ImportCad)
        }
        Msg::FromPhoto => {
            app.ui.start.open = false;
            app.update(Message::PhotoLoad)
        }
        Msg::OpenRecent(idx) => {
            app.ui.start.open = false;
            app.update(Message::RecentOpen(idx))
        }
        Msg::ClearRecent => {
            let t = app.update(Message::RecentClear);
            app.ui.start.sync_thumbs(&app.recent);
            t
        }
        Msg::CheckReport => {
            app.ui.start.report = ReportStatus::Checking;
            #[cfg(windows)]
            {
                check_report_task()
            }
            #[cfg(not(windows))]
            {
                app.ui.start.report = ReportStatus::Unavailable(
                    "Appraise-It Pro report integration runs on Windows.".into(),
                );
                iced::Task::none()
            }
        }
        Msg::ReportChecked(status) => {
            app.ui.start.report = status;
            iced::Task::none()
        }
        Msg::ShowAtLaunch(show) => {
            app.ui.start.hide_at_launch = !show;
            if crate::persist::write_json(
                PREFS_FILE,
                &StartPrefs {
                    show_at_launch: show,
                },
            ) == Some(false)
            {
                return app
                    .toasts
                    .show("Could not save the start screen setting.".to_string());
            }
            iced::Task::none()
        }
        Msg::DrawForReport => {
            app.ui.start.open = false;
            #[cfg(windows)]
            {
                app.update(Message::SubjectInfo(
                    crate::subject_info::SubjectMsg::AutoFill,
                ))
            }
            #[cfg(not(windows))]
            iced::Task::none()
        }
    }
}

#[cfg(windows)]
fn check_report_task() -> iced::Task<Message> {
    iced::Task::perform(
        async {
            match crate::appraise_it::check_report().await {
                Ok(crate::appraise_it::ReportStatus::Open(report, _)) => {
                    ReportStatus::Open(report.summary())
                }
                Ok(crate::appraise_it::ReportStatus::NoReport) => ReportStatus::NoReport,
                Ok(crate::appraise_it::ReportStatus::Cancelled) => ReportStatus::Idle,
                Err(e) => ReportStatus::Unavailable(e),
            }
        },
        |status| m(Msg::ReportChecked(status)),
    )
}

struct ThumbCanvas {
    thumb: RecentThumb,
    ink: Color,
    accent: Color,
    fill: Color,
    muted: Color,
}

impl canvas::Program<Message> for ThumbCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let pad = 10.0;
        let w = (bounds.width - pad * 2.0).max(12.0);
        let h = (bounds.height - pad * 2.0).max(12.0);
        let size = w.min(h);
        let ox = (bounds.width - size) * 0.5;
        let oy = (bounds.height - size) * 0.5;

        if !self.thumb.has_geometry {
            // Clean architectural outline placeholder when no pages/geometry exist.
            let x = ox + size * 0.14;
            let y = oy + size * 0.2;
            let pw = size * 0.72;
            let ph = size * 0.6;
            let rect = Path::rectangle(Point::new(x, y), Size::new(pw, ph));
            frame.stroke(
                &rect,
                Stroke::default().with_width(1.2).with_color(Color {
                    a: 0.45,
                    ..self.muted
                }),
            );
            let notch = Path::line(
                Point::new(x + pw * 0.55, y),
                Point::new(x + pw * 0.55, y + ph * 0.55),
            );
            frame.stroke(
                &notch,
                Stroke::default().with_width(1.0).with_color(Color {
                    a: 0.35,
                    ..self.muted
                }),
            );
            return vec![frame.into_geometry()];
        }

        let map = |(nx, ny): (f32, f32)| Point::new(ox + nx * size, oy + ny * size);
        for poly in &self.thumb.polys {
            if poly.len() < 3 {
                continue;
            }
            let path = Path::new(|b| {
                b.move_to(map(poly[0]));
                for &p in &poly[1..] {
                    b.line_to(map(p));
                }
                b.close();
            });
            frame.fill(&path, self.fill);
            frame.stroke(
                &path,
                Stroke::default().with_width(1.2).with_color(self.accent),
            );
        }
        for &(a, b) in &self.thumb.lines {
            frame.stroke(
                &Path::line(map(a), map(b)),
                Stroke::default().with_width(1.5).with_color(self.ink),
            );
        }
        vec![frame.into_geometry()]
    }
}

pub fn layer<'a>(app: &'a App, size_class: SizeClass, d: Density) -> Option<Element<'a, Message>> {
    if !app.ui.start.open {
        return None;
    }
    let compact = matches!(size_class, SizeClass::Compact);

    let start_actions = column![
        text("Start a sketch")
            .font(BARLOW_SEMIBOLD)
            .size(18.0)
            .color(tokens::text()),
        kit::with_id(
            "new-sketch",
            kit::primary_button("New sketch", Some(m(Msg::NewSketch)), d),
        ),
        start_action_row("folder-simple", "Open a file", m(Msg::OpenFile), d),
        start_action_row("layout", "Import a CAD drawing", m(Msg::ImportCad), d),
        start_action_row(
            "image",
            "Continue from a photo or plan",
            m(Msg::FromPhoto),
            d,
        ),
        kit::divider(),
        appraise_it_card(app, d),
    ]
    .spacing(SPACE_2)
    .width(Length::Fill);

    let left_col: Element<'a, Message> = container(start_actions)
        .padding(SPACE_3)
        .width(if compact {
            Length::Fill
        } else {
            Length::Fixed(310.0)
        })
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::panel())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_TILE.into(),
            },
            ..container::Style::default()
        })
        .into();

    let recent_header = row![
        text("Recent sketches")
            .font(BARLOW_SEMIBOLD)
            .size(18.0)
            .color(tokens::text())
            .width(Length::Fill),
        kit::text_button(
            "Clear recent",
            (!app.recent.is_empty()).then_some(m(Msg::ClearRecent)),
            d,
        ),
    ]
    .spacing(SPACE_2)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let cards: Element<'a, Message> = if app.recent.is_empty() {
        container(
            column![
                text("No recent sketches yet")
                    .font(BARLOW_MEDIUM)
                    .size(TEXT_14)
                    .color(tokens::text()),
                text("Create a new sketch, open a file, or drop a .skx or .sketchie file anywhere on this window.")
                    .font(BARLOW)
                    .size(TEXT_12)
                    .color(tokens::muted()),
            ]
            .spacing(SPACE_1),
        )
        .padding(SPACE_4)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::panel())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_TILE.into(),
            },
            ..container::Style::default()
        })
        .into()
    } else {
        let cols = if compact {
            1
        } else if matches!(size_class, SizeClass::Medium) {
            2
        } else {
            3
        };
        let mut grid_rows = column![].spacing(SPACE_2).width(Length::Fill);
        let mut idx = 0;
        while idx < app.recent.len() {
            let mut r = row![].spacing(SPACE_2).width(Length::Fill);
            for col_i in 0..cols {
                if let Some(rec) = app.recent.get(idx + col_i) {
                    r = r.push(recent_card(app, idx + col_i, rec));
                } else {
                    r = r.push(Space::new().width(Length::Fill));
                }
            }
            grid_rows = grid_rows.push(r);
            idx += cols;
        }
        grid_rows.into()
    };

    let right_col = column![recent_header, cards]
        .spacing(SPACE_3)
        .width(Length::Fill);

    let header = row![
        crate::icons::mono("compass", 18.0, tokens::accent()),
        text("Sketch Rust")
            .font(BARLOW_SEMIBOLD)
            .size(TEXT_16)
            .color(tokens::text()),
        Space::new().width(Length::Fill),
        kit::with_id(
            "blank-canvas",
            kit::text_button("Blank canvas", Some(m(Msg::Close)), d),
        ),
        kit::icon_button("x", false, Some(m(Msg::Close)), d),
    ]
    .spacing(SPACE_2)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let show_at_launch = crate::ui::dialogs::check_row(
        "Show this screen when Sketch Rust starts",
        !app.ui.start.hide_at_launch,
        |show| m(Msg::ShowAtLaunch(show)),
        d,
    );

    let content: Element<'a, Message> = if compact {
        scrollable(
            column![header, kit::divider(), left_col, right_col, show_at_launch]
                .spacing(SPACE_3)
                .padding(SPACE_3)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let max_w = (app.window_size.width - 48.0).clamp(620.0, 1020.0);
        let max_h = (app.window_size.height - 48.0).clamp(420.0, 720.0);
        container(
            column![
                header,
                kit::divider(),
                scrollable(
                    row![left_col, right_col]
                        .spacing(SPACE_4)
                        .align_y(Alignment::Start)
                        .width(Length::Fill),
                )
                .height(Length::Fill),
                kit::divider(),
                show_at_launch,
            ]
            .spacing(SPACE_3)
            .padding(SPACE_4)
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fixed(max_w))
        .height(Length::Fixed(max_h))
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::field())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: tokens::RADIUS_DIALOG.into(),
            },
            ..container::Style::default()
        })
        .into()
    };

    let screen = container(opaque(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::field())),
            ..container::Style::default()
        });

    Some(screen.into())
}

fn start_action_row<'a>(
    icon: &'static str,
    label: &'static str,
    msg: Message,
    d: Density,
) -> Element<'a, Message> {
    let body = row![
        crate::icons::mono(icon, 15.0, tokens::text()),
        text(label)
            .font(BARLOW_MEDIUM)
            .size(TEXT_13)
            .color(tokens::text()),
    ]
    .spacing(SPACE_2)
    .align_y(Alignment::Center);

    button(
        container(body)
            .height(Length::Fixed(d.min_target()))
            .padding([0, 12])
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(0)
    .width(Length::Fill)
    .on_press(msg)
    .style(|_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => tokens::hover(),
            button::Status::Pressed => tokens::pressed(),
            _ => tokens::panel(),
        })),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        text_color: tokens::text(),
        ..button::Style::default()
    })
    .into()
}

fn appraise_it_card<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let status_text = match &app.ui.start.report {
        ReportStatus::Idle => "Check if an Appraise-It Pro report is open.".to_owned(),
        ReportStatus::Checking => "Checking Appraise-It Pro session…".to_owned(),
        ReportStatus::Open(summary) => format!("Report open: {summary}"),
        ReportStatus::NoReport => "No open Appraise-It Pro report detected.".to_owned(),
        ReportStatus::Unavailable(reason) => reason.clone(),
    };
    let mut col = column![
        row![
            text("Appraise-It Pro")
                .font(BARLOW_SEMIBOLD)
                .size(TEXT_13)
                .color(tokens::text())
                .width(Length::Fill),
            kit::text_button("Check", Some(m(Msg::CheckReport)), d),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill),
        text(status_text)
            .font(BARLOW)
            .size(TEXT_12)
            .color(tokens::muted()),
    ]
    .spacing(SPACE_1)
    .width(Length::Fill);

    if matches!(app.ui.start.report, ReportStatus::Open(_)) {
        col = col.push(kit::primary_button(
            "Draw the sketch for this report",
            Some(m(Msg::DrawForReport)),
            d,
        ));
    }
    col.into()
}

fn recent_card<'a>(app: &'a App, index: usize, rec: &RecentFile) -> Element<'a, Message> {
    let thumb = app.ui.start.thumb_for(rec);
    let area_label = match thumb.living_sf {
        Some(sf) if sf > 0.0 => format!("{:.0} sf living", sf),
        _ => "Sketch".to_owned(),
    };
    let time_label = display_time(&rec.last_opened);

    let preview = container(
        Canvas::new(ThumbCanvas {
            thumb,
            ink: tokens::text(),
            accent: tokens::accent(),
            fill: tokens::accent_soft(),
            muted: tokens::muted(),
        })
        .width(Length::Fill)
        .height(Length::Fixed(104.0)),
    )
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::field())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    });

    let card_body = column![
        preview,
        text(rec.name.clone())
            .font(BARLOW_SEMIBOLD)
            .size(TEXT_13)
            .color(tokens::text()),
        row![
            text(area_label)
                .font(BSC_MEDIUM)
                .size(TEXT_11)
                .color(tokens::accent())
                .width(Length::Fill),
            text(time_label)
                .font(BARLOW)
                .size(TEXT_11)
                .color(tokens::muted()),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill),
    ]
    .spacing(SPACE_1)
    .padding(SPACE_2)
    .width(Length::Fill);

    button(card_body)
        .width(Length::Fill)
        .padding(0)
        .on_press(m(Msg::OpenRecent(index)))
        .style(|_, status| {
            let hov = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if hov {
                    tokens::hover()
                } else {
                    tokens::panel()
                })),
                border: Border {
                    color: if hov {
                        tokens::accent()
                    } else {
                        tokens::rule()
                    },
                    width: 1.0,
                    radius: RADIUS_TILE.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_at_launch_defaults_on_and_the_untick_is_remembered() {
        crate::persist::with_temp_store(|| {
            assert!(!load_hide_at_launch(), "shown by default");
            let (mut app, _) = crate::App::boot(1.0);
            assert!(!app.ui.start.hide_at_launch);
            let _ = update(&mut app, Msg::ShowAtLaunch(false));
            assert!(app.ui.start.hide_at_launch);
            assert!(load_hide_at_launch(), "the untick persists");
            let (reopened, _) = crate::App::boot(1.0);
            assert!(reopened.ui.start.hide_at_launch);
            assert!(!reopened.ui.start.open, "hidden at the next launch");
            let _ = update(&mut app, Msg::ShowAtLaunch(true));
            assert!(!load_hide_at_launch());
        });
    }
}
