//! Right dock (`#sidebar-dock`): 1px `--border` left edge, the tab strip
//! (Areas / Text / Symbols / Calcs / Layers + pin + collapse chevron), the
//! active panel, the collapsed 30px rail, and the left-edge resize handle.
//!
//! Sources: refs/ui-manager.js (showPanel, setSidebarOpen, populate*
//! builders, bindEvents), the SIDEBAR REDESIGN LAYER (refs/sidebar-redesign.txt:
//! collapse/expand, tab-strip pin) and SIDEBAR RESIZE LAYER
//! (refs/sidebar-resize.txt: 320-400px drag, canvas >= 220px, dblclick reset).
//! Measurements: refs/ui-spec/chrome-spec-states.json `dock:*` and
//! chrome-spec-screen3.json, at the default 312px width; everything that
//! stretches is laid out from the live width `w`.

use iced::widget::scrollable::{self, Direction, Scrollbar};
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{
    button, canvas, column, container, mouse_area, pick_list, row, stack, text, text_input, Column,
    Space,
};
use iced::{alignment, mouse, Background, Border, Color, Element, Length, Padding, Pixels, Point};

use crate::icons::{icon, ph};
use crate::symlib::{Preview, SymIndex};
use crate::theme::*;
use crate::Message;

/// `.sidebar-dock` default `--sidebar-w`.
pub const DEFAULT_W: f32 = 312.0;
/// SIDEBAR RESIZE LAYER: `MIN = 320, MAX = 400, CANVAS_MIN = 220`.
pub const MIN_W: f32 = 320.0;
pub const MAX_W: f32 = 400.0;
pub const CANVAS_MIN: f32 = 220.0;
/// `.sidebar-dock:not(.open)`.
pub const COLLAPSED_W: f32 = 30.0;

/// ui-manager.js `defaultTextLibraryLabels`.
pub const DEFAULT_TEXT_LABELS: &[&str] = &[
    "½ Bath",
    "Atrium",
    "Bath",
    "Bed",
    "Breakfast",
    "Cabin",
    "Carport",
    "Den",
    "Dining",
    "Dining Room",
    "Entrance",
    "Family",
    "Foyer",
    "Garage",
    "Kitchen",
    "Laundry",
    "Living Room",
    "Master",
    "Office",
    "Patio",
    "Play Cabin",
    "Porch",
    "Suite",
    "Utilities",
    "Wood Deck",
];

/// `.alpha-filter-btn` `(label, data-range)`.
const ALPHA: &[(&str, &str)] = &[
    ("All", "all"),
    ("ABC", "abc"),
    ("DEF", "def"),
    ("GHI", "ghi"),
    ("JKL", "jkl"),
    ("MNO", "mno"),
    ("PQR", "pqr"),
    ("STU", "stu"),
    ("VWX", "vwx"),
    ("YZ", "yz"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Areas,
    Text,
    Symbols,
    Calcs,
    Layers,
    /// Version history; shown only for a `.sketchy` store (not in `ALL`).
    Versions,
}

impl DockTab {
    pub const ALL: [DockTab; 5] = [
        DockTab::Areas,
        DockTab::Text,
        DockTab::Symbols,
        DockTab::Calcs,
        DockTab::Layers,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DockTab::Areas => "Areas",
            DockTab::Text => "Text",
            DockTab::Symbols => "Symbols",
            DockTab::Calcs => "Calcs",
            DockTab::Layers => "Layers",
            DockTab::Versions => "Versions",
        }
    }

    /// `state.sidebar.active` key (the Areas panel keeps its legacy key).
    pub fn key(self) -> &'static str {
        match self {
            DockTab::Areas => "define",
            DockTab::Text => "text",
            DockTab::Symbols => "symbols",
            DockTab::Calcs => "calcs",
            DockTab::Layers => "layers",
            DockTab::Versions => "versions",
        }
    }

    /// The inverse of `key` — restoring `state.sidebar.active`. The Versions
    /// tab is never restored: it exists only while a store is open.
    pub fn from_key(text: &str) -> Option<Self> {
        match text {
            "define" => Some(DockTab::Areas),
            "text" => Some(DockTab::Text),
            "symbols" => Some(DockTab::Symbols),
            "calcs" => Some(DockTab::Calcs),
            "layers" => Some(DockTab::Layers),
            _ => None,
        }
    }
}

/// What a dock item carries when it is dragged onto the canvas
/// (`application/x-sketch-item`: `kind` 'symbol' / 'text').
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragItem {
    Symbol(String),
    Text(String),
    /// An Areas-palette tile (`kind: 'area'`): the code, and its name when
    /// the tile shows one (the "Undefined · clear" tile does not).
    Area {
        code: String,
        name: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum DockMsg {
    Tab(DockTab),
    /// `showPanel(id)` from outside the tab strip (e.g. the totals chip):
    /// activates the panel and always opens the dock.
    Show(DockTab),
    Collapse,
    Expand,
    TogglePin,
    ResizeStart,
    ResizeReset,
    HandleHover(bool),
    TextSearch(String),
    TextSubmit,
    TextRange(&'static str),
    TextPick(String),
    SymbolCategory(CategoryOption),
    SymbolSearch(String),
    SymbolPick(String),
    /// The two library managers' openers (the Edit / Organize... buttons).
    EditTextLibrary,
    OrganizeSymbols,
    /// Buttons whose behavior belongs to a later screen (dialogs, Calcs).
    Noop,
}

/// A `#symbol-category-select` option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryOption {
    pub key: String,
    pub name: String,
}

impl std::fmt::Display for CategoryOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// `state.sidebar` plus the dock's UI-manager state.
#[derive(Debug, Clone)]
pub struct DockState {
    pub active: DockTab,
    pub open: bool,
    pub pinned: bool,
    /// `state.sidebar.width`; None = the CSS default (312px).
    pub width: Option<f32>,
    pub resizing: bool,
    pub handle_hover: bool,
    pub text_labels: Vec<String>,
    pub text_range: &'static str,
    pub text_search: String,
    /// `app.areaCodes` — the live table the Areas tree renders from.
    pub area_codes: Vec<sketch_core::geometry::AreaCode>,
    /// The `.lib-item.active` label (cleared whenever the list re-renders).
    pub text_active: Option<String>,
    pub symbol_category: String,
    pub symbol_search: String,
    /// The symbol shown in `#symbol-preview`.
    pub preview: Option<String>,
    /// Calcs groups the user collapsed (empty = all expanded, as the original
    /// opens them).
    pub calcs_collapsed: std::collections::HashSet<String>,
    /// Calcs areas whose details are open (the original starts them closed).
    pub calcs_expanded: std::collections::HashSet<i64>,
    // --- The Layers tree (S10-025..034 + S10-031) ---
    /// The row armed by a press (the drag source); `updateLayersPanel`'s
    /// rows are HTML5 `draggable` — the port arms on press and commits on
    /// release (drop-before/after/inside markers follow).
    pub layer_drag: Option<String>,
    /// The hovered drop target + ratio zone while a drag is armed.
    pub layer_over: Option<(String, crate::layers::DragPos)>,
    /// `.layer-row:hover` — the cursor's row.
    pub layer_hover: Option<String>,
    /// The inline rename input: (row id, live buffer).
    pub layer_rename: Option<(String, String)>,
    /// The right-click popup's row (`showLayerContextMenu`).
    pub layer_menu: Option<String>,
    /// The version rows (newest first) while a `.sketchy` store is open; None
    /// hides the Versions tab.
    pub versions: Option<Vec<crate::versions::VersionRow>>,
}

impl Default for DockState {
    fn default() -> Self {
        DockState {
            active: DockTab::Areas,
            open: true,
            pinned: true,
            width: None,
            resizing: false,
            handle_hover: false,
            text_labels: DEFAULT_TEXT_LABELS.iter().map(|s| s.to_string()).collect(),
            text_range: "all",
            text_search: String::new(),
            area_codes: Vec::new(),
            text_active: None,
            symbol_category: "favorites".into(),
            symbol_search: String::new(),
            preview: None,
            calcs_collapsed: Default::default(),
            calcs_expanded: Default::default(),
            layer_drag: None,
            layer_over: None,
            layer_hover: None,
            layer_rename: None,
            layer_menu: None,
            versions: None,
        }
    }
}

/// What a dock message asks the app to do besides updating dock state.
#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Toast(String),
}

/// SIDEBAR RESIZE LAYER `clamp(w)`.
pub fn clamp_width(w: f32, viewport_w: f32) -> f32 {
    let avail = MIN_W.max(viewport_w - CANVAS_MIN);
    MIN_W.max(MAX_W.min(w.round()).min(avail))
}

/// `String.prototype.localeCompare` order for the label sets involved:
/// case-insensitive, with symbols before digits/number forms (`½`) before
/// letters, as ICU collates them.
fn collate_key(s: &str) -> Vec<(u8, String)> {
    s.chars()
        .map(|c| {
            let class = if c.is_alphabetic() {
                2
            } else if c.is_numeric() {
                1
            } else {
                0
            };
            (class, c.to_lowercase().collect())
        })
        .collect()
}

impl DockState {
    /// The dock's laid-out width.
    pub fn width(&self) -> f32 {
        if self.open {
            self.width.unwrap_or(DEFAULT_W)
        } else {
            COLLAPSED_W
        }
    }

    /// Pointer moved to `x` while dragging the handle (`right - clientX`,
    /// the dock's right edge being the viewport's).
    pub fn drag_to(&mut self, x: f32, viewport_w: f32) {
        if self.resizing {
            self.width = Some(clamp_width(viewport_w - x, viewport_w));
        }
    }

    pub fn end_drag(&mut self) {
        self.resizing = false;
    }

    /// `populateTextLibrary(filter)` — the labels on display, sorted.
    pub fn text_items(&self) -> Vec<&str> {
        let q = self.text_search.trim().to_lowercase();
        let letters = if self.text_range == "all" {
            ""
        } else {
            self.text_range
        };
        let mut items: Vec<&str> = self
            .text_labels
            .iter()
            .map(String::as_str)
            .filter(|l| q.is_empty() || l.to_lowercase().contains(&q))
            .filter(|l| {
                letters.is_empty()
                    || l.trim()
                        .chars()
                        .next()
                        .map(|c| c.to_lowercase().any(|lc| letters.contains(lc)))
                        .unwrap_or(false)
            })
            .collect();
        items.sort_by_key(|l| collate_key(l));
        items
    }

    pub fn update(&mut self, msg: DockMsg) -> Effect {
        match msg {
            // showPanel(): tab clicks only act on inactive tabs and open the dock.
            DockMsg::Tab(tab) => {
                if self.active != tab {
                    self.active = tab;
                    self.open = true;
                }
            }
            DockMsg::Show(tab) => {
                self.active = tab;
                self.open = true;
            }
            DockMsg::Collapse => {
                self.open = false;
                self.handle_hover = false;
            }
            DockMsg::Expand => self.open = true,
            DockMsg::TogglePin => {
                self.pinned = !self.pinned;
                return Effect::Toast(
                    if self.pinned {
                        "Dock stays open after placing."
                    } else {
                        "Dock hides after placing."
                    }
                    .into(),
                );
            }
            DockMsg::ResizeStart => {
                if self.open {
                    self.resizing = true;
                }
            }
            DockMsg::ResizeReset => self.width = None,
            DockMsg::HandleHover(h) => self.handle_hover = h,
            DockMsg::TextSearch(q) => {
                self.text_search = q;
                self.text_active = None;
            }
            DockMsg::TextSubmit => {
                let value = self.text_search.trim().to_string();
                if !value.is_empty() {
                    self.text_active = Some(value.clone());
                    return Effect::Toast(format!("Click the drawing to place {}.", value));
                    // placeholder “{value}”."));
                }
            }
            DockMsg::TextRange(r) => {
                self.text_range = r;
                self.text_active = None;
            }
            // startTextPlacement(label) arrives with screen 9.
            DockMsg::TextPick(label) => self.text_active = Some(label),
            DockMsg::SymbolCategory(opt) => self.symbol_category = opt.key,
            DockMsg::SymbolSearch(q) => self.symbol_search = q,
            // startSymbolPlacement(id) arrives with screen 8; it re-renders the
            // list, so no row keeps `.active` — only the preview stays.
            DockMsg::SymbolPick(id) => self.preview = Some(id),
            // The App opens the two library managers before the dock sees
            // these.
            DockMsg::EditTextLibrary | DockMsg::OrganizeSymbols => {}
            DockMsg::Noop => {}
        }
        Effect::None
    }
}

fn dock(m: DockMsg) -> Message {
    Message::Dock(m)
}

/// A 1px `--border` line spanning the full height.
fn vline<'a>() -> Element<'a, Message> {
    container(Space::new().width(1).height(Length::Fill))
        .style(|_| container::Style {
            background: Some(border().into()),
            ..container::Style::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Tab strip
// ---------------------------------------------------------------------------

/// Tab left edges + the strip's last edge (page x at the default width,
/// tabs flex-fill 251px) per active tab — measured; the bold active label
/// widens its tab by 2-3px. The Areas row carries Chrome's sub-pixel flex
/// edges (they round to the measured pixels at both 312px and 400px).
fn tab_edges(active: DockTab) -> [f32; 6] {
    match active {
        DockTab::Areas => [1089.0, 1139.2, 1178.4, 1240.1, 1287.3, 1340.0],
        DockTab::Text => [1089.0, 1137.0, 1178.0, 1240.0, 1287.0, 1340.0],
        DockTab::Symbols => [1089.0, 1137.0, 1176.0, 1241.0, 1288.0, 1340.0],
        DockTab::Calcs => [1089.0, 1137.0, 1177.0, 1239.0, 1287.0, 1340.0],
        DockTab::Layers | DockTab::Versions => [1089.0, 1137.0, 1176.0, 1238.0, 1285.0, 1340.0],
    }
}

fn tab<'a>(t: DockTab, w: f32, active: bool) -> Element<'a, Message> {
    let (font, fg) = if active {
        (ARIAL_BOLD, WHITE)
    } else {
        (ARIAL, muted())
    };
    let body = stack![
        container(spaced(t.label(), 11.0, font, fg, 12.0, 0.2))
            .width(w - 1.0)
            .align_x(alignment::Horizontal::Center)
            .padding(Padding {
                top: 9.0,
                ..Padding::ZERO
            }),
        at(w - 1.0, 0.0, rect(1.0, 30.0, border())),
    ]
    .width(w)
    .height(30);
    button(body)
        .width(w)
        .height(30)
        .padding(0)
        .on_press(dock(DockMsg::Tab(t)))
        .style(move |_, status| button::Style {
            background: if active {
                Some(accent().into())
            } else {
                match status {
                    button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
                    _ => None,
                }
            },
            ..button::Style::default()
        })
        .into()
}

/// `.sidebar-collapse` — 26x26; `left_border` for the pin/collapse pair.
fn chrome_btn<'a>(
    glyph: Element<'a, Message>,
    left_border: bool,
    msg: DockMsg,
    title: &'static str,
) -> Element<'a, Message> {
    let mut body = stack![].width(26).height(26);
    if left_border {
        body = body.push(at(0.0, 0.0, rect(1.0, 26.0, border())));
    }
    body = body.push(at(6.0, 6.0, glyph));
    crate::tooltips::tip_below(
        button(body)
            .width(26)
            .height(26)
            .padding(0)
            .on_press(dock(msg))
            .style(|_, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
                    _ => None,
                },
                ..button::Style::default()
            }),
        title,
    )
}

fn tab_strip<'a>(st: &DockState, w: f32) -> Element<'a, Message> {
    let left = 1400.0 - DEFAULT_W; // page x of the dock at the default width
    let grow = (w - DEFAULT_W) / 5.0; // flex: 1 1 auto — extra space shared
    let edges = tab_edges(st.active);
    let mut s = stack![at(0.0, 30.0, rect(w - 1.0, 1.0, border()))]
        .width(w - 1.0)
        .height(31);
    if st.versions.is_some() {
        // Six tabs: the measured five-tab edges no longer apply, so the same
        // span is shared by label length.
        let start = edges[0] - left - 1.0;
        let span = edges[5] - edges[0] + grow * 5.0;
        let tabs: Vec<DockTab> = DockTab::ALL.into_iter().chain([DockTab::Versions]).collect();
        let weight = |t: &DockTab| t.label().chars().count() as f32 + 3.0;
        let total: f32 = tabs.iter().map(weight).sum();
        let mut used = 0.0;
        for t in &tabs {
            let x0 = (start + span * used / total).round();
            used += weight(t);
            let x1 = (start + span * used / total).round();
            s = s.push(at(x0, 0.0, tab(*t, x1 - x0, st.active == *t)));
        }
    } else {
        for (i, t) in DockTab::ALL.into_iter().enumerate() {
            let x0 = (edges[i] - left - 1.0 + grow * i as f32).round();
            let x1 = (edges[i + 1] - left - 1.0 + grow * (i + 1) as f32).round();
            s = s.push(at(x0, 0.0, tab(t, x1 - x0, st.active == t)));
        }
    }
    let pin_color = if st.pinned { ICON_BLUE } else { muted() };
    // `.sidebar-collapse` pair: left borders at page x 1342 / 1372 (default).
    s = s
        .push(at(
            w - 1.0 - 58.0,
            2.0,
            chrome_btn(
                icon("push-pin", true, 14.0, pin_color),
                true,
                DockMsg::TogglePin,
                "Keep sidebar open",
            ),
        ))
        .push(at(
            w - 1.0 - 28.0,
            2.0,
            chrome_btn(
                ph("caret-right", 14.0, muted()),
                true,
                DockMsg::Collapse,
                "Collapse sidebar",
            ),
        ));
    fill_bg(s, bg_ribbon()).width(w - 1.0).height(31).into()
}

// ---------------------------------------------------------------------------
// Shared controls
// ---------------------------------------------------------------------------

/// `::-webkit-scrollbar` (10px, 6px thumb inset 2px) on a list.
pub(crate) fn scroll<'a>(content: impl Into<Element<'a, Message>>, bg: Color) -> Element<'a, Message> {
    scrollable::Scrollable::new(content)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(10)
                .scroller_width(10)
                .margin(0)
                .spacing(0),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_, _| {
            let rail = scrollable::Rail {
                background: None,
                border: Border::default(),
                scroller: scrollable::Scroller {
                    background: Background::Color(bg_hover()),
                    border: Border {
                        color: bg,
                        width: 2.0,
                        radius: 5.0.into(),
                    },
                },
            };
            scrollable::Style {
                container: container::Style::default(),
                vertical_rail: rail,
                horizontal_rail: rail,
                gap: None,
                auto_scroll: scrollable::AutoScroll {
                    background: Background::Color(bg_panel()),
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                    icon: text_main(),
                },
            }
        })
        .into()
}

/// `.action-btn` — 25px, `--bg-hover` fill, 1px border, 13.33px control font.
/// The crate-message variant (the Calcs panel's Expand/Collapse All).
pub(crate) fn msg_action_btn<'a>(label_s: &str, w: impl Into<Length>, msg: Message) -> Element<'a, Message> {
    button(
        container(label(label_s, 13.333, ARIAL, text_main(), 15.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
    )
    .width(w)
    .height(25)
    .padding(0)
    .on_press(msg)
    .style(|_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => hex(0x4e4e52),
            _ => bg_hover(),
        })),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..button::Style::default()
    })
    .into()
}

/// `.action-btn` — 25px, `--bg-hover` fill, 1px border, 13.33px control font.
fn action_btn<'a>(label_s: &str, w: impl Into<Length>, msg: DockMsg) -> Element<'a, Message> {
    button(
        container(label(label_s, 13.333, ARIAL, text_main(), 15.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
    )
    .width(w)
    .height(25)
    .padding(0)
    .on_press(dock(msg))
    .style(|_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => hex(0x4e4e52),
            _ => bg_hover(),
        })),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..button::Style::default()
    })
    .into()
}

/// `.lib-search-input` — 24px, radius 3, 11px control font.
fn search<'a>(
    placeholder: &str,
    value: &str,
    on_input: fn(String) -> DockMsg,
    on_submit: Option<DockMsg>,
) -> Element<'a, Message> {
    let mut input = text_input(placeholder, value)
        .on_input(move |s| dock(on_input(s)))
        .font(ARIAL)
        .size(11)
        .line_height(LineHeight::Absolute(Pixels(12.0)))
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 9.0,
            right: 9.0,
        })
        .width(Length::Fill)
        .style(|_, status| text_input::Style {
            background: Background::Color(bg_app()),
            border: Border {
                color: if matches!(status, text_input::Status::Focused { .. }) {
                    ICON_BLUE
                } else {
                    border()
                },
                width: 1.0,
                radius: 3.0.into(),
            },
            icon: muted(),
            placeholder: hex(0x757575),
            value: text_main(),
            selection: Color { a: 0.4, ..accent() },
        });
    if let Some(m) = on_submit {
        input = input.on_submit(dock(m));
    }
    container(input).height(24).into()
}

/// The lists' frame: 1px border, `--bg-app`, 3px padding.
fn list_frame<'a>(content: impl Into<Element<'a, Message>>, radius: f32) -> Element<'a, Message> {
    boxed(
        container(content).padding(3),
        Length::Fill,
        Length::Fill,
        Some(bg_app()),
        border(),
        radius,
    )
    .padding(1)
    .into()
}

/// Muted message in an empty list or tree (`.library-empty` and friends).
fn empty_note<'a>(s: &str, pad_y: f32, pad_x: f32, centered: bool) -> Element<'a, Message> {
    container(label(s, 11.0, SEGOE, muted(), 15.0))
        .width(Length::Fill)
        .align_x(if centered {
            alignment::Horizontal::Center
        } else {
            alignment::Horizontal::Left
        })
        .padding(Padding {
            top: pad_y,
            bottom: pad_y,
            left: pad_x,
            right: pad_x,
        })
        .into()
}

// ---------------------------------------------------------------------------
// Areas panel (the palette; screen 1)
// ---------------------------------------------------------------------------

/// `.area-tile-group` — 24px header + 1px bottom border.
fn group_header<'a>(
    name: &str,
    count: usize,
    lw: f32,
    group: &str,
    collapsed: bool,
) -> Element<'a, Message> {
    let count_c = over(muted(), 0.75, bg_ribbon());
    let body = stack![
        at(0.0, 24.0, rect(lw, 1.0, border())),
        at(
            8.0,
            7.0,
            ph(
                if collapsed {
                    "caret-right"
                } else {
                    "caret-down"
                },
                10.0,
                muted()
            )
        ),
        at(
            24.0,
            6.0,
            spaced(&name.to_uppercase(), 9.0, SEGOE, muted(), 12.0, 1.08)
        ),
        container(label(&count.to_string(), 10.0, SEGOE, count_c, 14.0))
            .width(lw)
            .align_x(alignment::Horizontal::Right)
            .padding(Padding {
                top: 5.0,
                right: 8.0,
                ..Padding::ZERO
            }),
    ]
    .width(lw)
    .height(25);
    button(body)
        .width(lw)
        .height(25)
        .padding(0)
        .on_press(Message::AreaWorkflow(crate::area_workflow::Msg::Group(
            group.to_string(),
        )))
        .style(|_, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered | button::Status::Pressed => bg_hover(),
                _ => bg_ribbon(),
            })),
            ..button::Style::default()
        })
        .into()
}

/// `.area-tile` — 30px row (radius 5, 1px bottom border), 5px swatch that
/// starts 2px down and overhangs into the next row by 2px.
fn tile<'a>(
    name: &str,
    code: &str,
    swatch: u32,
    prev: Option<u32>,
    lw: f32,
    item: DragItem,
    workflow: &crate::area_workflow::State,
) -> Element<'a, Message> {
    let used = workflow.usage.get(code).copied().unwrap_or(0);
    let armed = workflow.armed.as_deref() == Some(code);
    // Bottom border of a radius-5 box with only border-bottom: a rounded
    // rect of border color, covered by the row's own rounded fill 1px up.
    let underline =
        container(
            container(Space::new().width(lw).height(29.0)).style(|_| container::Style {
                background: Some(bg_app().into()),
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        )
        .width(lw)
        .height(30)
        .style(|_| container::Style {
            background: Some(border().into()),
            border: Border {
                radius: 5.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let mut body = stack![underline].width(lw).height(30);
    // The inner area takes the press (a click or a drag to the canvas) and
    // the release; the button keeps the hover look.
    let hit = mouse_area(Space::new().width(lw).height(29.0))
        .on_press(Message::DockDragPress(item.clone()))
        .on_release(Message::DockItemRelease(item))
        .on_right_press(Message::AreaWorkflow(crate::area_workflow::Msg::TypeMenu(
            code.to_string(),
        )));
    body = body.push(
        button(hit)
            .padding(0)
            .on_press(Message::Noop)
            .style(|_, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
                    _ => None,
                },
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    );
    if let Some(p) = prev {
        body = body.push(at(0.0, 0.0, rect(5.0, 2.0, hex(p))));
    }
    body = body
        .push(at(0.0, 2.0, rect(5.0, 28.0, hex(swatch))))
        .push(at(13.0, 7.0, label(name, 12.0, SEGOE, text_main(), 16.0)))
        .push(
            container(spaced(code, 9.5, SEGOE, muted(), 12.0, 0.38))
                .width(lw)
                .align_x(alignment::Horizontal::Right)
                .padding(Padding {
                    top: 9.0,
                    right: 6.0,
                    ..Padding::ZERO
                }),
        );
    if armed {
        body = body.push(at(0.0, 0.0, rect(lw, 2.0, accent())));
    }
    if used > 0 && code != "UND" {
        body = body.push(at(
            lw - 77.0,
            7.0,
            label(&used.to_string(), 11.0, SEGOE, accent(), 16.0),
        ));
    }
    crate::tooltips::tip(
        crate::event_tap::EventTap::new(body).keyboard_action(
            format!("area-tile-{code}"),
            Message::AreaWorkflow(crate::area_workflow::Msg::Arm(code.to_string())),
        ),
        if code == "UND" {
            "Drag onto an area to clear its classification. Right-click for options."
        } else {
            "Drag onto an outline, or click to arm. Right-click for options."
        },
    )
}

/// `paintPalette`'s `labels` — the short group display names the live palette
/// uses (not the long `groupNames` map from `populateAreaCodes`, which targets
/// a `.area-tree` DOM that does not exist in the original).
pub fn palette_group_label(group: &str) -> &'static str {
    match group {
        "GLA" => "Living space",
        "GBA" => "Building area",
        "GAR" => "Garage / carport",
        "BSMT" => "Basement",
        "P/P" => "Outdoor",
        "OTH" => "Storage",
        "LAND" => "Land",
        "SITE" => "Site",
        "NCA" => "Non-calculated",
        "UND" => "Undefined",
        _ => "",
    }
}

/// One `.area-tile-group` of the live palette.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaGroup {
    /// The raw group code (GLA, GAR, OTH…).
    pub group: String,
    /// Display label — `groupLabel(group)`, falling back to the group code.
    pub label: String,
    /// The codes in it, in the table's order — `(name, code, colour)`.
    pub tiles: Vec<(String, String, u32)>,
}

/// The palette model: the group rows plus the trailing "Undefined · clear"
/// tile. Empty groups are skipped, as `paintPalette` does.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub groups: Vec<AreaGroup>,
    /// The trailing clear tile — `(name, code, colour)`. `None` when the UND
    /// entry is missing from the table and the fallback provides it.
    pub clear: Option<(String, String, u32)>,
}

/// `codeColor`: `entry.color || entry.fillColor || '#858585'`.
pub(crate) fn code_color(entry: &sketch_core::geometry::AreaCode) -> u32 {
    let hex_or = |s: &str| u32::from_str_radix(s.trim_start_matches('#'), 16).ok();
    hex_or(&entry.color)
        .or_else(|| hex_or(&entry.fill_color))
        .unwrap_or(0x85_85_85)
}

/// `paintPalette` — the palette's model: groups (UND excluded) plus the
/// trailing "Undefined · clear" tile.
pub fn paint_palette(codes: &[sketch_core::geometry::AreaCode]) -> Palette {
    let mut model = Palette {
        groups: Vec::new(),
        clear: None,
    };
    // areaCodes is not group-contiguous (OTH sits either side of LAND/SITE),
    // so collect each group rather than heading on adjacency.
    let mut seen: Vec<&str> = Vec::new();
    for entry in codes.iter().filter(|e| e.group != "UND") {
        if seen.iter().any(|s| *s == entry.group) {
            continue;
        }
        seen.push(&entry.group);
    }
    for group in seen {
        let tiles: Vec<(String, String, u32)> = codes
            .iter()
            .filter(|e| e.code != "UND" && e.group == group)
            .map(|e| {
                (
                    if e.name.is_empty() {
                        e.code.clone()
                    } else {
                        e.name.clone()
                    },
                    e.code.clone(),
                    code_color(e),
                )
            })
            .collect();
        if tiles.is_empty() {
            continue;
        }
        // `groupLabel(group) { return labels[group] || group; }`
        let label = match palette_group_label(group) {
            "" => group.to_string(),
            known => known.to_string(),
        };
        model.groups.push(AreaGroup {
            group: group.to_string(),
            label,
            tiles,
        });
    }
    // The trailing clear tile: the UND entry or the fallback.
    let undefined_entry = codes.iter().find(|c| c.code == "UND").cloned().unwrap_or(
        sketch_core::geometry::AreaCode {
            code: "UND".into(),
            name: "Undefined Area".into(),
            color: "#E53E3E".into(),
            ..Default::default()
        },
    );
    let clear_color = code_color(&undefined_entry);
    model.clear = Some((
        "Undefined · clear".to_string(),
        "UND".to_string(),
        clear_color,
    ));
    model
}

fn palette<'a>(
    codes: &[sketch_core::geometry::AreaCode],
    lw: f32,
    workflow: &crate::area_workflow::State,
) -> Element<'a, Message> {
    let model = paint_palette(codes);
    let mut list = column![].width(lw);
    let mut prev: Option<u32> = None;
    for g in &model.groups {
        let collapsed = workflow.collapsed.contains(&g.group);
        list = list.push(group_header(
            &g.label,
            g.tiles.len(),
            lw,
            &g.group,
            collapsed,
        ));
        if collapsed {
            continue;
        }
        for (name, code, color) in &g.tiles {
            list = list.push(tile(
                name,
                code,
                *color,
                prev,
                lw,
                DragItem::Area {
                    code: code.clone(),
                    name: Some(name.clone()),
                },
                workflow,
            ));
            prev = Some(*color);
        }
    }
    if let Some((name, code, color)) = &model.clear {
        list = list.push(tile(
            name,
            code,
            *color,
            None,
            lw,
            DragItem::Area {
                code: code.clone(),
                name: None,
            },
            workflow,
        ));
    }

    boxed(
        scroll(list, bg_app()),
        Length::Fill,
        Length::Fill,
        Some(bg_app()),
        border(),
        0.0,
    )
    .padding(1)
    .into()
}

/// `.area-workflow-foot` — 1px top border, links 8px below it.
fn footer<'a>(cw: f32) -> Element<'a, Message> {
    let link = |s: &'static str, w: f32, msg: Message| -> Element<'a, Message> {
        button(stack![
            label(s, 11.0, SEGOE, muted(), 15.0),
            at(0.0, 14.0, rect(w, 1.0, muted())),
        ])
        .padding(0)
        .on_press(msg)
        .style(|_, _| button::Style::default())
        .into()
    };
    let dot = || label("·", 11.0, SEGOE, muted(), 15.0);
    stack![
        at(0.0, 0.0, rect(cw, 1.0, border())),
        at(
            0.0,
            9.0,
            link(
                "New area",
                46.0,
                Message::AreaWorkflow(crate::area_workflow::Msg::NewArea)
            )
        ),
        at(54.0, 9.0, dot()),
        at(
            64.0,
            9.0,
            link(
                "Find undefined",
                73.0,
                Message::AreaWorkflow(crate::area_workflow::Msg::FindUndefined)
            )
        ),
        at(145.0, 9.0, dot()),
        at(
            156.0,
            9.0,
            link(
                "Edit areas…",
                55.0,
                Message::AreaWorkflow(crate::area_workflow::Msg::Advanced(true))
            )
        ),
    ]
    .width(cw)
    .height(24)
    .into()
}

fn areas_panel<'a>(
    codes: &'a [sketch_core::geometry::AreaCode],
    w: f32,
    workflow: &crate::area_workflow::State,
    post: &sketch_core::model::AutoPost,
    dimensions: bool,
    placement: crate::view_state::DimensionPlacement,
) -> Element<'a, Message> {
    if workflow.advanced {
        return crate::area_workflow::advanced(post, dimensions, placement);
    }
    // Keep the palette at a stable child index when guidance appears, so an
    // armed tile retains its keyboard focus and a drag keeps its widget state.
    let status = container(text(workflow.status.clone()).font(SEGOE).size(11)).height(
        if workflow.status.is_empty() {
            Length::Fixed(0.0)
        } else {
            Length::Shrink
        },
    );
    let content = column![status];
    content
        .push(column![
            palette(codes, w - 29.0, workflow),
            Space::new().height(12),
            footer(w - 17.0)
        ])
        .height(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Text panel (`#panel-text-library`)
// ---------------------------------------------------------------------------

/// `.lib-item` — 24px, radius 2, 1px border, 11px control font at 5px 8px
/// (inside the 1px border: text box at 6, 9).
fn lib_item<'a>(s: &str, active: bool) -> Element<'a, Message> {
    // The press also arms a drag (`draggable="true"`); the button keeps the
    // click, which a drag suppresses.
    button(
        mouse_area(
            container(label(s, 11.0, ARIAL, text_main(), 12.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding {
                    top: 6.0,
                    left: 9.0,
                    ..Padding::ZERO
                }),
        )
        .on_press(Message::DockDragPress(DragItem::Text(s.to_string())))
        .on_release(Message::DockItemRelease(DragItem::Text(s.to_string()))),
    )
    .width(Length::Fill)
    .height(24)
    .padding(0)
    // The inner area takes the press and release (a click is a release that
    // no drag took); this only keeps the button's hover look.
    .on_press(dock(DockMsg::Noop))
    .style(move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, border) = if active {
            (over(accent(), 0.2, bg_app()), ICON_BLUE)
        } else if hovered {
            (bg_hover(), border())
        } else {
            (bg_panel(), Color::TRANSPARENT)
        };
        button::Style {
            background: Some(bg.into()),
            border: Border {
                color: border,
                width: 1.0,
                radius: 2.0.into(),
            },
            ..button::Style::default()
        }
    })
    .into()
}

/// `.alpha-filter-btn` — 40x21, radius 2, 10px control font.
fn alpha_btn<'a>(s: &'static str, range: &'static str, active: bool) -> Element<'a, Message> {
    let fg = if active { hex(0x171717) } else { text_main() };
    button(
        container(label(s, 10.0, ARIAL, fg, 11.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
    )
    .width(40)
    .height(21)
    .padding(0)
    .on_press(dock(DockMsg::TextRange(range)))
    .style(move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, border) = if active {
            (hex(0xd69e2e), hex(0xf6ad55))
        } else if hovered {
            (bg_hover(), border())
        } else {
            (bg_panel(), border())
        };
        button::Style {
            background: Some(bg.into()),
            border: Border {
                color: border,
                width: 1.0,
                radius: 2.0.into(),
            },
            ..button::Style::default()
        }
    })
    .into()
}

fn text_panel<'a>(st: &'a DockState) -> Element<'a, Message> {
    let items = st.text_items();
    let list: Element<'a, Message> = if items.is_empty() {
        empty_note("No matching labels.", 10.0, 10.0, true)
    } else {
        scroll(
            column(
                items
                    .into_iter()
                    .map(|l| lib_item(l, st.text_active.as_deref() == Some(l))),
            )
            .spacing(2),
            bg_app(),
        )
    };
    let alpha = column(
        ALPHA
            .iter()
            .map(|&(l, r)| alpha_btn(l, r, st.text_range == r)),
    )
    .spacing(3)
    .width(40);
    column![
        search(
            "Find or type a label...",
            &st.text_search,
            DockMsg::TextSearch,
            Some(DockMsg::TextSubmit)
        ),
        Space::new().height(12),
        row![list_frame(list, 0.0), Space::new().width(6), alpha].height(Length::Fill),
        Space::new().height(6),
        action_btn("Edit", Length::Fill, DockMsg::EditTextLibrary),
    ]
    .into()
}

// ---------------------------------------------------------------------------
// Symbols panel (`#panel-symbol-library`)
// ---------------------------------------------------------------------------

/// `.lib-symbol-item` — 26px, radius 3, icon 16px + 7px gap + label.
fn symbol_item<'a>(id: &str, label_s: &str, icon_name: &str) -> Element<'a, Message> {
    let body = stack![
        at(7.0, 5.0, ph(icon_name, 16.0, ICON_BLUE)),
        at(32.0, 7.0, label(label_s, 11.0, ARIAL, text_main(), 12.0)),
    ]
    .width(Length::Fill)
    .height(26);
    let body = mouse_area(body)
        .on_press(Message::DockDragPress(DragItem::Symbol(id.to_string())))
        .on_release(Message::DockItemRelease(DragItem::Symbol(id.to_string())))
        .on_right_press(Message::SymbolManager(crate::symbol_manager::Msg::Context(
            id.to_string(),
        )));
    button(body)
        .width(Length::Fill)
        .height(26)
        .padding(0)
        .on_press(dock(DockMsg::Noop))
        .style(|_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hovered { bg_hover() } else { bg_panel() }.into()),
                border: Border {
                    color: if hovered {
                        border()
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The native `<select>` arrow: a 2px chevron, `--text-main`.
struct SelectArrow;

impl<M> canvas::Program<M> for SelectArrow {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: iced::Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut f = canvas::Frame::new(renderer, bounds.size());
        let path = canvas::Path::new(|p| {
            p.move_to(Point::new(1.0, 1.0));
            p.line_to(Point::new(5.0, 5.6));
            p.line_to(Point::new(9.0, 1.0));
        });
        f.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(text_main())
                .with_width(1.6),
        );
        vec![f.into_geometry()]
    }
}

/// `#symbol-category-select` — 26px, radius 3, 11px control font.
fn category_select<'a>(st: &DockState, ix: &SymIndex) -> Element<'a, Message> {
    let options: Vec<CategoryOption> = ix
        .options()
        .into_iter()
        .map(|(key, name)| CategoryOption { key, name })
        .collect();
    let selected = options
        .iter()
        .find(|o| o.key == st.symbol_category)
        .cloned();
    let pick = pick_list(options, selected, |o| dock(DockMsg::SymbolCategory(o)))
        .font(ARIAL)
        .text_size(11)
        .text_line_height(LineHeight::Absolute(Pixels(12.0)))
        // Measured: the native select draws its text at (+13, +8) of the box.
        .padding(Padding {
            top: 8.0,
            bottom: 4.0,
            left: 12.0,
            right: 8.0,
        })
        .width(Length::Fill)
        .handle(pick_list::Handle::None)
        .style(|_, _| pick_list::Style {
            text_color: text_main(),
            placeholder_color: muted(),
            handle_color: text_main(),
            background: Background::Color(bg_app()),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 3.0.into(),
            },
        });
    stack![
        container(pick).height(26),
        container(canvas(SelectArrow).width(10).height(7))
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .padding(Padding {
                top: 10.0,
                right: 4.0,
                ..Padding::ZERO
            }),
    ]
    .height(26)
    .into()
}

/// `#symbol-preview` — 78px frame; "Select a symbol" or canvas + caption.
fn symbol_preview<'a>(st: &'a DockState, ix: &'a SymIndex) -> Element<'a, Message> {
    let inner: Element<'a, Message> = match st.preview.as_deref().and_then(|id| ix.symbol(id)) {
        None => container(label("Select a symbol", 16.0, SEGOE, muted(), 21.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .padding(Padding {
                top: 1.0,
                ..Padding::ZERO
            })
            .into(),
        Some(def) => {
            let note = match def.wall_snap {
                sketch_core::symbols::WallSnap::None => "Click the drawing to place".to_string(),
                snap => format!("Wall snap: {}", format!("{snap:?}").to_lowercase()),
            };
            let caption = column![
                label(&def.label, 11.0, SEGOE_BOLD, text_main(), 15.0),
                Space::new().height(3),
                label(&note, 13.333, SEGOE, muted(), 17.0),
            ]
            .align_x(alignment::Horizontal::Center);
            container(
                row![
                    canvas(Preview { symbol: def }).width(92).height(68),
                    rect(1.0, 68.0, border()),
                    Space::new().width(8),
                    container(caption).padding(Padding {
                        top: 2.0,
                        right: 8.0,
                        ..Padding::ZERO
                    }),
                ]
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .into()
        }
    };
    boxed(inner, Length::Fill, 78, Some(bg_app()), border(), 0.0)
        .padding(1)
        .into()
}

/// The Symbols panel; the App's saved favorites (`state.symbolFavorites`)
/// override the library defaults in the Favorites category.
fn symbols_panel_with_favorites<'a>(
    st: &'a DockState,
    lib: &'a Result<SymIndex, String>,
    favorites: Option<&'a [String]>,
) -> Element<'a, Message> {
    let ix = match lib {
        Ok(ix) => ix,
        Err(e) => {
            return column![
                list_frame(empty_note(e, 10.0, 10.0, true), 0.0),
                Space::new().height(6),
                action_btn("Manage symbols...", Length::Fill, DockMsg::OrganizeSymbols),
            ]
            .into()
        }
    };
    let q = st.symbol_search.trim().to_lowercase();
    let items: Vec<_> = match (st.symbol_category.as_str(), favorites) {
        ("favorites", Some(list)) => list
            .iter()
            .filter_map(|t| ix.by_id(t))
            .filter(|i| q.is_empty() || i.label.to_lowercase().contains(&q))
            .collect(),
        _ => ix
            .items(&st.symbol_category)
            .iter()
            .filter(|i| q.is_empty() || i.label.to_lowercase().contains(&q))
            .cloned()
            .collect(),
    };
    let list: Element<'a, Message> = if items.is_empty() {
        empty_note("No matching symbols.", 10.0, 10.0, true)
    } else {
        scroll(
            column(
                items
                    .into_iter()
                    .map(|i| symbol_item(&i.id, &i.label, i.icon)),
            )
            .spacing(2),
            bg_app(),
        )
    };
    let hidden_actions: Element<'a, Message> = if ix.hidden.is_empty() {
        Space::new().into()
    } else {
        row![
            button(text("Restore hidden…").size(11)).on_press(Message::SymbolManager(
                crate::symbol_manager::Msg::RestoreAll
            ))
        ]
        .spacing(6)
        .into()
    };
    column![
        category_select(st, ix),
        Space::new().height(6),
        search(
            "Find a symbol...",
            &st.symbol_search,
            DockMsg::SymbolSearch,
            None
        ),
        Space::new().height(12),
        list_frame(list, 0.0),
        Space::new().height(6),
        symbol_preview(st, ix),
        Space::new().height(6),
        action_btn("Manage symbols...", Length::Fill, DockMsg::OrganizeSymbols),
        hidden_actions,
    ]
    .into()
}

// ---------------------------------------------------------------------------
// Calcs panel (`#panel-calcs`, empty sketch; live numbers arrive in screen 7)
// ---------------------------------------------------------------------------

fn summary_row<'a>(name: &str, value: &str) -> Element<'a, Message> {
    stack![
        at(
            0.0,
            3.0,
            label(name, 11.0, SEGOE_SEMIBOLD, text_main(), 15.0)
        ),
        container(label(value, 12.0, CONSOLAS_BOLD, ICON_BLUE, 15.0))
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .padding(Padding {
                top: 4.0,
                ..Padding::ZERO
            }),
    ]
    .width(Length::Fill)
    .height(21)
    .into()
}

/// The calcs tree: group headers (expanded unless collapsed) with their area
/// rows, and each area's details when expanded. Fonts, paddings and colours
/// come from `theme.css` `.calcs-*` (11px 600 group header with a monospace
/// `#63B3ED` total; 10px area header with a 10px caret; 10px detail rows with a
/// monospace value).
fn calcs_tree<'a>(
    calcs: &crate::calcs::Calcs,
    collapsed: &std::collections::HashSet<String>,
    expanded_areas: &std::collections::HashSet<i64>,
) -> Element<'a, Message> {
    let mut groups: Vec<Element<'a, Message>> = Vec::new();
    for group in &calcs.groups {
        let open = !collapsed.contains(&group.code);
        let caret = if open { "caret-down" } else { "caret-right" };
        let header = button(
            row![
                row![
                    ph(caret, 10.0, text_main()),
                    text(group.code.clone())
                        .size(11.0)
                        .font(SEGOE_SEMIBOLD)
                        .color(text_main()),
                ]
                .spacing(4.0)
                .align_y(alignment::Vertical::Center),
                container(
                    text(format!("{:.2}", group.total))
                        .size(11.0)
                        .font(CONSOLAS)
                        .color(hex(0x63B3ED))
                )
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right),
            ]
            .align_y(alignment::Vertical::Center),
        )
        .on_press(Message::CalcsGroup(group.code.clone()))
        .width(Length::Fill)
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 8.0,
            right: 8.0,
        })
        .style(|_, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered | button::Status::Pressed => hex(0x2A2A2B),
                _ => bg_panel(),
            })),
            border: Border::default(),
            text_color: text_main(),
            ..button::Style::default()
        });
        groups.push(header.into());
        if !open {
            continue;
        }
        let mut areas: Vec<Element<'a, Message>> = Vec::new();
        for area in &group.areas {
            let expanded = expanded_areas.contains(&area.id);
            let caret = if expanded {
                "caret-down"
            } else {
                "caret-right"
            };
            let header = button(
                row![
                    ph(caret, 10.0, text_main()),
                    text(format!("{} : {:.2}", area.name, area.net_area))
                        .size(10.0)
                        .font(SEGOE)
                        .color(text_main()),
                ]
                .spacing(4.0)
                .align_y(alignment::Vertical::Center),
            )
            .on_press(Message::CalcsArea(area.id))
            .width(Length::Fill)
            .padding(Padding {
                top: 3.0,
                bottom: 3.0,
                left: 2.0,
                right: 2.0,
            })
            .style(|_, status| button::Style {
                background: Some(Background::Color(match status {
                    button::Status::Hovered | button::Status::Pressed => hex(0x2A2A2B),
                    _ => Color::TRANSPARENT,
                })),
                border: Border::default(),
                text_color: text_main(),
                ..button::Style::default()
            });
            let header = mouse_area(header).on_right_press(Message::AreaWorkflow(
                crate::area_workflow::Msg::Open(area.id),
            ));
            areas.push(
                container(header)
                    .padding(Padding {
                        top: 2.0,
                        bottom: 3.0,
                        left: 8.0,
                        right: 0.0,
                    })
                    .into(),
            );
            if !expanded {
                continue;
            }
            let mut details: Vec<Element<'a, Message>> = Vec::new();
            let mut push_row = |label: &'static str, value: String, alarm: bool| {
                details.push(
                    container(
                        row![
                            text(label).size(10.0).font(SEGOE).color(muted()),
                            container(text(value).size(10.0).font(CONSOLAS).color(if alarm {
                                hex(0xFC8181)
                            } else {
                                text_main()
                            }))
                            .width(Length::Fill)
                            .align_x(alignment::Horizontal::Right),
                        ]
                        .align_y(alignment::Vertical::Center),
                    )
                    .padding(Padding {
                        top: 2.0,
                        bottom: 2.0,
                        ..Padding::ZERO
                    })
                    .into(),
                );
            };
            push_row("Base Area", format!("{:.2}", area.base_area), false);
            push_row("Factor", format!("{:.2}", area.factor), false);
            push_row("Total", format!("{:.2}", area.total), false);
            if let Some(subtracted) = area.subtracted {
                push_row("Subtracted", format!("-{subtracted:.2}"), true);
            }
            push_row("Net Area", format!("{:.2}", area.net_area), false);
            push_row("Perimeter", format!("{:.2}", area.perimeter), false);
            areas.push(
                container(Column::with_children(details))
                    .padding(Padding {
                        top: 2.0,
                        bottom: 5.0,
                        left: 14.0,
                        right: 0.0,
                    })
                    .into(),
            );
        }
        groups.push(
            container(Column::with_children(areas))
                .padding(Padding {
                    top: 4.0,
                    bottom: 4.0,
                    left: 8.0,
                    right: 8.0,
                })
                .into(),
        );
    }
    Column::with_children(groups).width(Length::Fill).into()
}

fn calcs_panel<'a>(
    cw: f32,
    calcs: &crate::calcs::Calcs,
    totals: Option<crate::calcs::AreaTotals>,
    collapsed: &std::collections::HashSet<String>,
    expanded_areas: &std::collections::HashSet<i64>,
) -> Element<'a, Message> {
    let half = (cw - 8.0) / 2.0;
    let tree_body: Element<'a, Message> = if calcs.is_empty() {
        empty_note(calcs.empty_note(), 8.0, 8.0, false)
    } else {
        calcs_tree(calcs, collapsed, expanded_areas)
    };
    let tree = boxed(
        container(tree_body).padding(4),
        Length::Fill,
        300,
        Some(bg_app()),
        border(),
        0.0,
    )
    .padding(1);
    let hint = container(
        text("Right-click an area here or on the drawing to edit its definition.")
            .size(11)
            .font(SEGOE)
            .color(muted())
            .line_height(LineHeight::Absolute(Pixels(15.0)))
            .wrapping(Wrapping::Word),
    )
    .width(Length::Fill)
    .height(35)
    .padding(Padding {
        top: 2.9,
        ..Padding::ZERO
    });
    // The summary is the original's own four rows (Undefined only when there
    // are any), driven by `calcs::summary`.
    let mut summary_rows: Vec<Element<'a, Message>> = Vec::new();
    for row in crate::calcs::summary(calcs, totals) {
        summary_rows.push(summary_row(row.label, &row.value));
    }
    let summary = column![
        rect(cw, 1.0, border()),
        container(Column::with_children(summary_rows)).padding(8),
    ];
    column![
        row![
            msg_action_btn("Expand All", half, Message::CalcsExpandAll),
            Space::new().width(8),
            msg_action_btn("Collapse All", half, Message::CalcsCollapseAll),
        ],
        Space::new().height(6),
        tree,
        Space::new().height(6),
        hint,
        Space::new().height(6),
        msg_action_btn("Calculation Details", Length::Fill, Message::OpenBreakdown),
        Space::new().height(14),
        summary,
    ]
    .into()
}

// ---------------------------------------------------------------------------
// Layers panel (`#panel-layers`) — the area-layer tree (S10-025..034)
// ---------------------------------------------------------------------------

/// `.layer-row` height: a 15px line + [5,6] padding (theme.css 1656-1662).
/// The name strip is a column of press/drop zones: a group row splits
/// 25% / 50% / 25% (6/13/6) with the middle zone dropping *inside* the group;
/// a leaf row splits at the original's `ratio < 0.5` with a widened middle so
/// the name keeps its centered 5px offset (exact 12.5 would push it down).
/// Leaf rows have no inside drop — the middle zone's marker resolves to
/// `after` on commit, matching `moveLayer`'s guard.
const LAYER_ROW_H: f32 = 25.0;
const LAYER_STRIP_H: f32 = 13.0;
/// `--depth * 14px` indent on `.layer-name` (theme.css 1705-1712).
const DEPTH_STEP: f32 = 14.0;
/// `.layer-row.is-group > i { color: #e8b64c }` — the folder icon.
const GROUP_GOLD: Color = hex(0xe8b64c);

/// The `.layer-rename` field's text_input id (dialog.rs's INPUT_ID pattern).
pub const RENAME_ID: &str = "layer-rename-input";

/// Blends `c` toward the row background for the states the CSS expresses as
/// opacity: hidden names 0.75, locked 0.6, `.layer-row.dragging` 0.45.
fn fade(c: Color, alpha: f32) -> Color {
    let bg = bg_panel();
    Color {
        r: bg.r * (1.0 - alpha) + c.r * alpha,
        g: bg.g * (1.0 - alpha) + c.g * alpha,
        b: bg.b * (1.0 - alpha) + c.b * alpha,
        a: 1.0,
    }
}

/// One name strip with eye/lock tools, 13px icons + 17px hit targets
/// (`.layer-eye/.layer-lock`: 2px padding on the 13px glyph).
fn layer_tool<'a>(
    glyph: Element<'a, Message>,
    msg: Message,
    title: &'static str,
) -> Element<'a, Message> {
    let b: Element<'a, Message> = button(
        container(glyph)
            .width(17)
            .height(17)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
    )
    .width(17)
    .height(17)
    .padding(0)
    .on_press(msg)
    .style(|_, status| button::Style {
        background: match status {
            // rgba(128,128,128,0.25) over the row.
            button::Status::Hovered | button::Status::Pressed => {
                Some(Color { a: 0.25, ..muted() }.into())
            }
            _ => None,
        },
        ..button::Style::default()
    })
    .into();
    crate::tooltips::tip_below(b, title)
}

/// One segment (before / inside / after) of a row's draggable name strip.
/// A press on free strip arms the drag (a nested widget — eye/lock buttons
/// capture their press first — so tool clicks never arm); the release over a
/// zone commits it, and a release with no armed press resolves as a plain
/// row click (set-active).
fn zone<'a>(
    height: f32,
    pos: crate::layers::DragPos,
    id: &str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    mouse_area(container(content).width(Length::Fill).height(height))
        .on_press(Message::LayerPress(id.to_owned()))
        .on_release(Message::LayerDrop(id.to_owned(), pos))
        .on_enter(Message::LayerZoneHover(id.to_owned(), pos))
        .into()
}

/// One `.layer-row`. The row data is per-frame (`visible_rows` output), so
/// every owned string is cloned into the widgets and nothing borrows `r`.
fn layer_row<'a>(st: &'a DockState, r: &crate::layers::Row) -> Element<'a, Message> {
    use crate::layers::DragPos;
    let id = r.id.as_str();
    let dragging = st.layer_drag.as_deref() == Some(id);
    // The drop marker only renders on a NON-drag target row while a press is
    // armed (showLayerContextMenu keeps `.dragging` on the source).
    let drop_mark: Option<DragPos> = if dragging {
        None
    } else {
        st.layer_over
            .as_ref()
            .filter(|(target, _)| target != id && st.layer_drag.is_some())
            .map(|(_, p)| *p)
    };
    // Row coloring: `.layer-row:hover` bg, the `active/inside` outline, and
    // the fades the CSS expresses with opacity.
    let hovered = st.layer_hover.as_deref() == Some(id);
    let bg = if hovered && !dragging {
        bg_hover()
    } else {
        bg_panel()
    };
    let inside = drop_mark == Some(DragPos::Inside);
    let name = if dragging {
        fade(text_main(), 0.45)
    } else if !r.visible {
        // line-through not available in iced 0.14 text (documented gap).
        fade(muted(), 0.75)
    } else if r.locked {
        fade(text_main(), 0.6)
    } else {
        text_main()
    };
    let icon_color = if dragging {
        fade(muted(), 0.45)
    } else {
        muted()
    };
    let tool_color = |own: bool, muted_anchor: Color, plain: Color| -> Color {
        if dragging {
            fade(plain, 0.45)
        } else if !own {
            muted_anchor
        } else {
            plain
        }
    };

    // The name: an inline text_input while renaming (S10-030), else the label
    // with the depth indent, centered on the row like the 12px/15px span.
    let renaming = st
        .layer_rename
        .as_ref()
        .filter(|(target, _)| target == id)
        .map(|(_, buffer)| buffer.clone());
    let mid_content: Element<'a, Message> = if let Some(buffer) = renaming {
        // `.layer-rename`: 12px input on `--bg-app`, `--bg-active` border.
        let id_owned = id.to_owned();
        container(
            text_input("", buffer.as_str())
                .size(12)
                .padding(2)
                .width(Length::Fill)
                .id(RENAME_ID)
                .on_input(move |s| Message::LayerRename(id_owned.clone(), s))
                .on_submit(Message::LayerRenameCommit),
        )
        .width(Length::Fill)
        .into()
    } else {
        crate::tooltips::tip_below(
            container(label(r.name.as_str(), 12.0, SEGOE, name, 15.0))
                .width(Length::Fill)
                .height(LAYER_ROW_H - 6.0)
                .align_y(alignment::Vertical::Center)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: r.depth as f32 * DEPTH_STEP,
                }),
            r.name.clone(),
        )
    };

    // The drop surfaces — group rows [6|13|6] with a real inside drop, leaf
    // rows [6|13|6] where the middle marker resolves to `after`.
    let strip = Column::with_children(vec![
        zone(6.0, DragPos::Before, id, Space::new()),
        zone(LAYER_STRIP_H, DragPos::Inside, id, mid_content),
        zone(6.0, DragPos::After, id, Space::new()),
    ]);

    // Tools: eye and lock per `updateLayersPanel` (`ph-eye`/`ph-eye-slash`,
    // `ph-lock-simple-fill`/`ph-lock-simple-open`.
    let eye = if r.visible { "eye" } else { "eye-slash" };
    let eye_color = tool_color(r.visible, muted(), text_main());
    let lock = if r.locked {
        "lock-simple"
    } else {
        "lock-simple-open"
    };
    let lock_color = tool_color(r.locked, muted(), text_main());
    let tools = row![
        layer_tool(
            ph(eye, 13.0, eye_color),
            Message::LayerToggleVisible(id.to_owned()),
            if r.visible {
                "Hide layer"
            } else {
                "Show layer"
            },
        ),
        Space::new().width(2),
        layer_tool(
            icon(lock, r.locked, 13.0, lock_color),
            Message::LayerToggleLocked(id.to_owned()),
            if r.locked {
                "Unlock layer"
            } else {
                "Lock layer"
            },
        ),
    ];

    // The head: a folding caret on groups (S10-029), a 14px slot on leaf rows.
    let head: Element<'a, Message> = if r.is_group {
        let caret = if r.collapsed {
            "caret-right"
        } else {
            "caret-down"
        };
        crate::tooltips::tip_below(
            button(
                container(ph(caret, 12.0, text_main()))
                    .width(14)
                    .height(17)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center),
            )
            .width(14)
            .height(17)
            .padding(0)
            .on_press(Message::LayerToggleCollapsed(id.to_owned()))
            .style(|_, status| button::Style {
                background: match status {
                    // `.layer-caret:hover` (no explicit rule; the row's hover
                    // bg is a fair approximation).
                    button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
                    _ => None,
                },
                ..button::Style::default()
            }),
            if r.collapsed {
                "Expand group"
            } else {
                "Collapse group"
            },
        )
    } else {
        Space::new().width(14).into()
    };

    let row_element = container(
        row![
            head,
            ph(
                if r.is_group {
                    "folder-simple"
                } else {
                    "shapes"
                },
                14.0,
                if r.is_group { GROUP_GOLD } else { icon_color }
            ),
            strip,
            tools,
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .height(LAYER_ROW_H);

    // Copied out so the style closure owns its data (r is per-frame).
    let active_row = r.active;
    let state_frame: container::Container<'a, Message> = match drop_mark {
        // The CSS box-shadows: a 2px `--bg-active` bar above/below the row.
        Some(DragPos::Before) | Some(DragPos::After) => {
            let y = match drop_mark {
                Some(DragPos::Before) => 0.0,
                _ => LAYER_ROW_H - 2.0,
            };
            container(stack![
                row_element,
                at(
                    0.0,
                    y,
                    container(Space::new())
                        .width(Length::Fill)
                        .height(2.0)
                        .style(|_| container::Style {
                            background: Some(Background::Color(accent())),
                            ..container::Style::default()
                        })
                )
            ])
        }
        _ => container(row_element),
    }
    .width(Length::Fill)
    .height(LAYER_ROW_H)
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: if active_row || inside {
            // `.drop-inside` (dashed in CSS, solid here — a noted gap) and
            // the `.active` outline share the accent 1px frame.
            Border {
                color: accent(),
                width: 1.0,
                radius: 5.0.into(),
            }
        } else {
            Border::default().rounded(5.0)
        },
        ..container::Style::default()
    });
    // The row wrapper: `.layer-row:hover` tracking and the tree's
    // contextmenu (a nested widget — the tools/caret — captures presses
    // first, so tool clicks never open the menu).
    mouse_area(state_frame)
        .on_enter(Message::LayerHoverRow(id.to_owned()))
        .on_exit(Message::LayerHoverRowGone(id.to_owned()))
        .on_right_press(Message::LayerRowMenu(id.to_owned()))
        .into()
}

/// The layer row under a window point, for the right-press menu. Iced's
/// right presses reach mouse_area with no cursor (the EventTap's
/// last-cursor fallback routes here instead), so the row is resolved from
/// the panel's fixed geometry: the dock's top is 129 (28px titlebar, 1px
/// rule, 99px ribbon), then the 31px tab strip, 8px panel padding and the
/// 5px frame (1 padding, 1 border, 3 padding); 25px rows on a 27px pitch.
pub fn layer_row_at(
    st: &DockState,
    layers: &crate::layers::Layers,
    area: &crate::layers::AreaLayerFilter,
    dock_left: f32,
    x: f32,
    y: f32,
) -> Option<String> {
    if !st.open || st.active != DockTab::Layers {
        return None;
    }
    const ROWS_TOP: f32 = 129.0 + 31.0 + 8.0 + 5.0;
    let rel = y - ROWS_TOP;
    if rel < 0.0 {
        return None;
    }
    let pitch = LAYER_ROW_H + 2.0;
    let index = (rel / pitch).floor() as usize;
    if rel - index as f32 * pitch > LAYER_ROW_H {
        return None; // the 2px gap between rows
    }
    // The tree card's x span: 8px panel padding + 5px frame each side.
    let card_x0 = dock_left + 8.0 + 5.0;
    let card_x1 = dock_left + st.width() - 8.0 - 5.0;
    if !(card_x0..=card_x1).contains(&x) {
        return None;
    }
    let rows = layers.visible_rows(|l| area.keeps(l));
    rows.get(index).map(|r| r.id.clone())
}

/// showLayerContextMenu's popup (ui-manager.js 3020): Rename / Hide|Show /
/// Lock|Unlock / — / Export. The original floats it at the right-click
/// point; iced 0.14's fixed-message handlers can't report cursor coordinates
/// in a scrolled tree, so the row's own top-left is the approximation (a
/// noted gap). Everything is owned: the popup outlives the row borrow.
fn layer_menu_popup(index: usize, r: &crate::layers::Row) -> Element<'static, Message> {
    let id = r.id.clone();
    let item = |icon_name: &'static str,
                color: Color,
                text: &'static str,
                msg: Message|
     -> Element<'static, Message> {
        button(
            row![
                container(ph(icon_name, 14.0, color))
                    .width(20)
                    .align_x(alignment::Horizontal::Center),
                label(text, 12.0, SEGOE, text_main(), 15.0),
                Space::new().width(12),
            ]
            .height(24)
            .align_y(alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            left: 8.0,
            ..Padding::ZERO
        })
        .on_press(msg)
        .style(|_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => Some(bg_hover().into()),
                _ => None,
            },
            ..button::Style::default()
        })
        .into()
    };
    let sep = || -> Element<'static, Message> { Space::new().height(6).into() };
    let mut col = Column::new().push(item(
        "text-t",
        text_main(),
        "Rename",
        Message::LayerStartRename(id.clone()),
    ));
    col = col.push(item(
        if r.visible { "eye-slash" } else { "eye" },
        if r.visible { text_main() } else { muted() },
        if r.visible { "Hide" } else { "Show" },
        Message::LayerToggleVisible(id.clone()),
    ));
    col = col.push(item(
        if r.locked {
            "lock-simple-open"
        } else {
            "lock-simple"
        },
        if r.locked { muted() } else { text_main() },
        if r.locked { "Unlock" } else { "Lock" },
        Message::LayerToggleLocked(id.clone()),
    ));
    col = col.push(sep());
    col = col.push(item(
        "export",
        text_main(),
        "Export",
        Message::LayerExport(id.clone()),
    ));
    let card = container(col)
        .width(150)
        .padding(4.0)
        .style(|_| container::Style {
            background: Some(Background::Color(bg_ribbon())),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        });
    // Row tops: 4px of frame padding (1 boxed + 3 inner) + the 27px pitch;
    // the anchor clamps to stay inside the dock at the default heights.
    let top = (4.0 + index as f32 * (LAYER_ROW_H + 2.0)).min(324.0);
    at(14.0, top, card)
}

fn layers_panel<'a>(
    st: &'a DockState,
    layers: &'a crate::layers::Layers,
    area: &crate::layers::AreaLayerFilter,
) -> Element<'a, Message> {
    let rows = layers.visible_rows(|l| area.keeps(l));
    let mut col: iced::widget::Column<'a, Message> = iced::widget::column![].spacing(2);
    // The popup clones its row: per-frame data must not borrow into 'a.
    let mut menu: Option<(usize, crate::layers::Row)> = None;
    for (i, r) in rows.iter().enumerate() {
        if st.layer_menu.as_deref() == Some(r.id.as_str()) {
            menu = Some((i, r.clone()));
        }
        col = col.push(layer_row(st, r));
    }
    let tree: Element<'a, Message> = if rows.is_empty() {
        column![
            Space::new().height(Length::Fill),
            empty_note(
                "Closed areas create layers automatically.",
                14.0,
                10.0,
                true
            ),
            Space::new().height(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    } else {
        col.into()
    };
    let card = list_frame(tree, 6.0);
    match menu {
        Some((i, r)) => stack![card, layer_menu_popup(i, &r)].into(),
        None => card,
    }
}

// ---------------------------------------------------------------------------
// Dock
// ---------------------------------------------------------------------------

/// `#sidebar-collapsed-rail` — the 30px strip with the expand chevron.
fn collapsed<'a>() -> Element<'a, Message> {
    let expand = chrome_btn(
        ph("caret-left", 14.0, text_main()),
        false,
        DockMsg::Expand,
        "Expand sidebar",
    );
    row![
        vline(),
        fill_bg(
            stack![at(2.0, 8.0, expand)].width(29).height(Length::Fill),
            bg_panel()
        )
        .width(29)
        .height(Length::Fill),
    ]
    .width(COLLAPSED_W)
    .height(Length::Fill)
    .into()
}

pub struct PanelContext<'a> {
    pub favorites: Option<&'a [String]>,
    pub workflow: &'a crate::area_workflow::State,
    pub auto_post: &'a sketch_core::model::AutoPost,
    pub dimensions: bool,
    pub placement: crate::view_state::DimensionPlacement,
    /// The revision id on screen while viewing an old version.
    pub viewing: Option<&'a str>,
}

pub fn view<'a>(
    st: &'a DockState,
    lib: &'a Result<SymIndex, String>,
    calcs: &crate::calcs::Calcs,
    totals: Option<crate::calcs::AreaTotals>,
    layers: &'a crate::layers::Layers,
    area: &crate::layers::AreaLayerFilter,
    panels: PanelContext<'a>,
) -> Element<'a, Message> {
    if !st.open {
        return collapsed();
    }
    let w = st.width();
    let cw = w - 17.0;
    let body: Element<'a, Message> = match st.active {
        DockTab::Areas => areas_panel(
            &st.area_codes,
            w,
            panels.workflow,
            panels.auto_post,
            panels.dimensions,
            panels.placement,
        ),
        DockTab::Text => text_panel(st),
        DockTab::Symbols => symbols_panel_with_favorites(st, lib, panels.favorites),
        DockTab::Calcs => calcs_panel(cw, calcs, totals, &st.calcs_collapsed, &st.calcs_expanded),
        DockTab::Layers => layers_panel(st, layers, area),
        DockTab::Versions => match &st.versions {
            Some(rows) => crate::versions::panel(rows, panels.viewing),
            None => Space::new().into(),
        },
    };
    let panel = container(body)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill);
    row![
        vline(),
        fill_bg(column![tab_strip(st, w), panel], bg_panel())
            .width(w - 1.0)
            .height(Length::Fill),
    ]
    .width(w)
    .height(Length::Fill)
    .into()
}

/// `#sidebar-resize-handle` — 7px wide, straddling the dock's left edge
/// (left: -3px), over the canvas and the dock. Returned as an overlay row the
/// workspace stacks on top of itself; empty when the dock is collapsed.
pub fn resize_handle<'a>(st: &DockState) -> Element<'a, Message> {
    if !st.open {
        return Space::new().into();
    }
    let lit = st.handle_hover || st.resizing;
    let strip =
        container(Space::new().width(7).height(Length::Fill)).style(move |_| container::Style {
            background: lit.then_some(Background::Color(Color {
                a: 0.35,
                ..accent()
            })),
            ..container::Style::default()
        });
    let handle = mouse_area(strip)
        .on_press(dock(DockMsg::ResizeStart))
        .on_double_click(dock(DockMsg::ResizeReset))
        .on_enter(dock(DockMsg::HandleHover(true)))
        .on_exit(dock(DockMsg::HandleHover(false)))
        .interaction(mouse::Interaction::ResizingHorizontally);
    row![
        Space::new().width(Length::Fill),
        handle,
        Space::new().width(st.width() - 4.0),
    ]
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_library_sorts_and_filters_like_ui_manager() {
        let mut st = DockState::default();
        let all = st.text_items();
        // The live original's order (localeCompare): ½ Bath first.
        assert_eq!(all.len(), 25);
        assert_eq!(&all[..4], &["½ Bath", "Atrium", "Bath", "Bed"]);
        assert_eq!(all.last(), Some(&"Wood Deck"));
        st.update(DockMsg::TextRange("abc"));
        assert_eq!(
            st.text_items(),
            ["Atrium", "Bath", "Bed", "Breakfast", "Cabin", "Carport"]
        );
        st.update(DockMsg::TextSearch("ba".into()));
        assert_eq!(st.text_items(), ["Bath"]);
        st.update(DockMsg::TextRange("all"));
        assert_eq!(st.text_items(), ["½ Bath", "Bath"]);
        st.update(DockMsg::TextSearch("zzz".into()));
        assert!(st.text_items().is_empty());
    }

    #[test]
    fn resize_clamps_like_the_resize_layer() {
        assert_eq!(clamp_width(390.0, 1400.0), 390.0);
        assert_eq!(clamp_width(410.0, 1400.0), 400.0);
        assert_eq!(clamp_width(100.0, 1400.0), 320.0);
        // Canvas keeps >= 220px: at 560px the dock may not exceed 340.
        assert_eq!(clamp_width(400.0, 560.0), 340.0);
        assert_eq!(clamp_width(400.0, 400.0), 320.0);
        let mut st = DockState::default();
        st.drag_to(990.0, 1400.0);
        assert_eq!(st.width, None, "moves only count after ResizeStart");
        st.update(DockMsg::ResizeStart);
        st.drag_to(990.0, 1400.0);
        assert_eq!(st.width(), 400.0);
        st.drag_to(1050.0, 1400.0);
        assert_eq!(st.width(), 350.0);
        st.end_drag();
        st.drag_to(990.0, 1400.0);
        assert_eq!(st.width(), 350.0);
        st.update(DockMsg::ResizeReset);
        assert_eq!(st.width(), DEFAULT_W);
    }

    #[test]
    fn collapse_pin_and_tabs_follow_show_panel() {
        let mut st = DockState::default();
        st.update(DockMsg::Collapse);
        assert_eq!(st.width(), COLLAPSED_W);
        st.update(DockMsg::ResizeStart);
        assert!(!st.resizing, "no resize while collapsed");
        st.update(DockMsg::Expand);
        assert_eq!(st.width(), DEFAULT_W);
        st.update(DockMsg::Collapse);
        st.update(DockMsg::Tab(DockTab::Calcs));
        assert!(st.open, "a tab click opens the dock");
        assert_eq!(st.active.key(), "calcs");
        assert_eq!(
            st.update(DockMsg::TogglePin),
            Effect::Toast("Dock hides after placing.".into())
        );
        assert!(!st.pinned);
        assert_eq!(
            st.update(DockMsg::TogglePin),
            Effect::Toast("Dock stays open after placing.".into())
        );
    }
}

#[cfg(test)]
mod area_tree_tests {
    use super::*;
    use sketch_core::geometry::area_codes::default_area_codes;

    #[test]
    fn the_palette_uses_the_short_group_labels() {
        assert_eq!(palette_group_label("GLA"), "Living space");
        assert_eq!(palette_group_label("GBA"), "Building area");
        assert_eq!(palette_group_label("GAR"), "Garage / carport");
        assert_eq!(palette_group_label("P/P"), "Outdoor");
        assert_eq!(palette_group_label("NCA"), "Non-calculated");
        // An unknown group falls back to its code, as groupLabel does.
        assert_eq!(palette_group_label("XX"), "");
    }

    #[test]
    fn the_default_table_groups_in_first_seen_order_without_und() {
        let model = paint_palette(&default_area_codes());
        let groups: Vec<&str> = model.groups.iter().map(|g| g.group.as_str()).collect();
        assert_eq!(
            groups,
            vec!["GLA", "GBA", "GAR", "BSMT", "P/P", "OTH", "LAND", "SITE", "NCA"]
        );
        let gla = &model.groups[0];
        assert_eq!(gla.label, "Living space");
        assert_eq!(gla.tiles.len(), 4);
        assert_eq!(gla.tiles[0].0, "First Floor");
        assert_eq!(gla.tiles[0].1, "GLA1");
    }

    #[test]
    fn und_is_the_trailing_clear_tile_not_a_group() {
        let model = paint_palette(&default_area_codes());
        let (name, code, color) = model.clear.expect("clear tile");
        assert_eq!(name, "Undefined · clear");
        assert_eq!(code, "UND");
        assert_eq!(color, 0xe53e3e, "the default table's UND colour");
        assert!(!model.groups.iter().any(|g| g.group == "UND"));
    }

    #[test]
    fn an_und_entry_missing_from_the_table_falls_back() {
        let codes: Vec<_> = default_area_codes()
            .into_iter()
            .filter(|c| c.code != "UND")
            .collect();
        let model = paint_palette(&codes);
        let (name, code, color) = model.clear.expect("clear tile");
        assert_eq!(name, "Undefined · clear");
        assert_eq!(code, "UND");
        assert_eq!(color, 0xe53e3e, "the fallback's colour");
    }

    #[test]
    fn code_color_falls_back_through_the_fill_to_the_default() {
        let mut entry = default_area_codes()
            .into_iter()
            .find(|c| c.code == "OTH")
            .expect("OTH");
        assert_eq!(code_color(&entry), 0xa0aec0);
        entry.color = String::new();
        // Falls back to fillColor when color is empty.
        let via_fill = if entry.fill_color.is_empty() {
            0x85_85_85
        } else {
            u32::from_str_radix(entry.fill_color.trim_start_matches('#'), 16).expect("fill")
        };
        assert_eq!(code_color(&entry), via_fill);
        entry.fill_color = String::new();
        assert_eq!(code_color(&entry), 0x85_85_85, "then to #858585");
        // A full hex string is parsed; the '#' is optional.
        entry.color = "#2B6CB0".into();
        assert_eq!(code_color(&entry), 0x2b6cb0);
    }

    #[test]
    fn empty_groups_are_skipped_and_labels_fall_back_to_the_code() {
        let codes: Vec<_> = default_area_codes()
            .into_iter()
            .filter(|c| c.group != "GAR")
            .collect();
        let model = paint_palette(&codes);
        assert!(!model.groups.iter().any(|g| g.group == "GAR"));
        // A group the label map doesn't know shows just its code.
        let mut entry = default_area_codes()
            .into_iter()
            .find(|c| c.code == "OTH")
            .expect("OTH");
        entry.group = "XX".into();
        entry.code = "XX1".into();
        let model = paint_palette(std::slice::from_ref(&entry));
        assert_eq!(model.groups[0].label, "XX");
        assert_eq!(model.groups[0].tiles[0].0, "Storage");
    }
}
