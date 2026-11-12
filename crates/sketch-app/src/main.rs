//! sketch-app — native rebuild of Appraisal Sketch 1.1 (Iced 0.14).
//!
//! Screens 1-2 of ACCEPTANCE.md. Screen 1: the static chrome of the default state —
//! borderless window with the app-drawn title bar (logo, QAT, save-state
//! pill, title, working minimize / full-screen / close + drag-to-move), the
//! ribbon tab strip with the Home body, the 17-button left rail, the white
//! canvas, the right dock on Areas, and the status bar. Every element is
//! placed from `refs/ui-spec/chrome-spec-home.json` (measured from the
//! running original at 1400x860 CSS px). Only the window controls are live
//! on this screen; every other control is visual (with its CSS hover).
//! Screen 2: the ribbon tabs switch bodies (View / Tools / Reference Image /
//! Help from `refs/ui-spec/chrome-spec-states.json`) and the File tab
//! toggles the backstage (`backstage.rs`), all per `refs/ui-manager.js`.
//!
//! Display scaling: the original is a web page, so one CSS px is one device
//! px times the device pixel ratio. By default the app follows the system
//! DPI exactly like the browser does. `SKETCH_DPR=<ratio>` overrides the
//! ratio — `SKETCH_DPR=1` renders 1 CSS px = 1 physical px, which is what
//! `tools/shot.ps1` + `tools/compare.py` need to compare against the
//! gallery captured at DPR 1.

// Native GUI executables should not allocate a Windows console.
#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

mod apex_compat;
#[cfg(test)]
mod apex_compat_tests;
#[cfg(windows)]
mod apex_registration;
mod apex_startup;
mod area_dialog;
mod area_props;
mod area_repair;
#[cfg(test)]
mod area_repair_tests;
mod area_workflow;
mod attr_dialog;
mod autosave;
mod backstage;
mod breakdown_dialog;
mod calcs;
mod canvas_theme;
mod chains;
mod clipboard;
mod codes_editor;
mod color_dialog;
mod command_palette;
mod context_menu;
mod def_editor;
mod define_area;
#[allow(unused_imports)]
mod dialog;
mod disto;
mod dock;
mod drawing;
#[cfg(test)]
mod e2e;
mod event_tap;
mod exporter;
mod file_dialogs;
mod filter;
mod format;
mod handles;
mod icons;
mod input;
mod integration_setup;
mod layer_io;
mod layers;
#[cfg(test)]
mod management_tests;
mod minimap;
mod new_code_dialog;
mod options;
mod options_dialog;
mod page_layout;
mod pages;
mod pages_nav;
mod persist;
mod placed_symbol;
mod placed_text;
mod platform;
mod png_overlay;
mod popup_menu;
mod print;
mod rail;
mod recent;
mod reference_image;
mod render;
mod report_sketch;
mod ribbon;
mod ribbon_tabs;
mod room_shapes;
mod select_menu;
mod selection;
mod shapes;
mod guide;
mod shortcuts;
mod sketch_canvas;
mod snap;
mod statusbar;
mod subject_info;
mod symbol_manager;
mod symbol_organizer;
mod symlib;
mod text_manager;
mod theme;
mod titlebar;
mod toast;
mod tooltips;
mod touch_panel;
mod transform;
mod undo;
mod undo_history;
mod view_state;
mod window_state;

use std::sync::Arc;

use iced::widget::{column, container, row, stack};
use iced::{event, mouse, window, Element, Event, Length, Point, Size, Subscription, Task};

use ribbon::{Body, TabId};

/// Reference window client size in CSS px.
const WINDOW_W: f32 = 1400.0;
const WINDOW_H: f32 = 860.0;

#[derive(Debug, Clone)]
pub enum Message {
    /// A visual-only control was clicked (no behavior on this screen).
    Noop,
    ApexReturn,
    ApexPoll,
    ApexSnapshotReady(apex_compat::SnapshotReady),
    ApexReturned(Result<(), String>),
    ApexSaved(Result<(), String>),
    IntegrationSetup(integration_setup::Msg),
    SymbolManager(symbol_manager::Msg),
    ImportLayer,
    AutoSaveTick,
    RecoveryOpen,
    RecoverAutosave(bool),
    /// The Page Layout dialog (Screen 11, S11-033..037).
    PageLayout(page_layout::LayoutMsg),
    WindowReady(Option<window::Id>),
    MonitorSize(Option<Size>),
    TitleDrag,
    Minimize,
    ToggleMaximize,
    /// The window's maximized state, re-read after every resize or move.
    Maximized(bool),
    WindowMoved(Point),
    /// A press on one of the window's edge / corner resize zones.
    ResizeDrag(window::Direction),
    /// A file dragged from the desktop and dropped on the window.
    FileDropped(std::path::PathBuf),
    /// Open the sketch named on the command line (after the window is up).
    OpenStartupFile,
    /// Another instance was asked to open this window's file: come forward.
    ActivateWindow,
    /// A two-finger touch on the canvas: zoom by `factor` about (x, y)
    /// (canvas px) and pan by the centroid's travel.
    Pinch {
        factor: f32,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    Close,
    CloseHover(bool),
    /// A ribbon tab (File included) was clicked.
    TabClicked(TabId),
    /// A mouse button went down at this point (backstage open only; reported
    /// by the root `EventTap`, the press still reaches its target).
    PointerDown(Point),
    /// Escape was pressed (backstage open only).
    Escape,
    /// The right dock (screen 3).
    Dock(dock::DockMsg),
    /// Cursor moved while dragging the dock's resize handle: position and
    /// viewport width (root `EventTap`, i.e. pointer capture).
    PointerMoved(Point, f32),
    /// A press on a dock symbol / text item: arms a drag to the canvas.
    DockDragPress(dock::DragItem),
    /// The pointer moved (window CSS px) while a dock item is armed.
    DockDragMove(Point, f32),
    /// The button came up: a drag that ends over the canvas drops there.
    DockDragRelease,
    /// The button came up over a dock item (its click, unless a drag).
    DockItemRelease(dock::DragItem),
    /// A mouse button was released while dragging the resize handle.
    PointerUp,
    /// The toast of this generation timed out.
    ToastExpired(u64),
    /// The window's logical size changed.
    WindowResized(Size),
    /// Status bar (screen 4) and the ribbon commands that share it.
    Status(statusbar::StatusMsg),
    /// A Select-mode press: starts the pointer's marquee when it missed.
    CanvasPress(f32, f32),
    /// A drag in progress (canvas px) and its release.
    /// The pointer moved during a drag (canvas px); the flag is Ctrl (fine
    /// 1 degree rotation steps).
    CanvasDrag(f32, f32, bool),
    CanvasDragEnd,
    /// Zoom fence drag on the canvas (canvas px).
    FenceStart(f32, f32),
    FenceMove(f32, f32),
    FenceEnd,
    /// The modal dialog (alert / confirm / prompt).
    Dialog(dialog::DialogMsg),
    /// File I/O.
    NewFile,
    OpenFile,
    SaveFile,
    SaveFileAs,
    /// Command palette (Ctrl+K).
    CommandPaletteOpen,
    CommandPaletteClose,
    CommandPaletteQuery(String),
    CommandPaletteMove(i32),
    CommandPaletteRun,
    /// A palette row clicked (its position among the matches).
    CommandPaletteRunAt(usize),
    /// `executeCommand('rotate-sketch')` / R: `rotateSketch(90)`.
    RotateSketch,
    /// L / R: `rotateSketch(degrees)`.
    RotateSketchBy(f64),
    /// Shift+R / Ctrl+Shift+R: `rotateSelected(degrees)`.
    RotateSelected(f64),
    /// + / -: `scaleSelected(factor)`.
    ScaleSelected(f64),
    TouchPanelToggle,
    TouchDistance(String),
    TouchAction(String),
    RoomShapeOpen,
    RoomShapeField(String, String),
    RoomShapePlace,
    RoomShapeCancel,
    RoomShapePlaceAt(f32, f32),
    OpenSubjectInfo,
    SubjectInfo(subject_info::SubjectMsg),
    /// The Layers tree (screen 10): S10-025..034 + the S10-031 drag/drop.
    /// A press on a row's name strip arms the drag, hovering another row
    /// shows the drop marker (before / inside / after), the release commits.
    LayerPress(String),
    LayerDrop(String, crate::layers::DragPos),
    LayerZoneHover(String, crate::layers::DragPos),
    /// `.layer-row:hover` tracking (the hover background).
    LayerHoverRow(String),
    LayerHoverRowGone(String),
    LayerToggleVisible(String),
    LayerToggleLocked(String),
    LayerToggleCollapsed(String),
    LayerSetActive(String),
    /// Double-click / menu-Rename: the inline `.layer-rename` input.
    LayerStartRename(String),
    /// The rename input's buffer (id, live value).
    LayerRename(String, String),
    LayerRenameCommit,
    LayerRenameCancel,
    /// The row context menu (`showLayerContextMenu`).
    LayerRowMenu(String),
    LayerMenuClose,
    /// `exportLayer` (S10-033): the layer + its entities as a .json file.
    LayerExport(String),
    /// The context menu's layer ops (S10-037/038): autoAssignLayer with a
    /// named layer pushed to the stack's back/front.
    LayerSendToBack,
    LayerBringToFront,
    /// An orphaned drag release (the pointer left the dock): resolve or
    /// clear the armed press (no-op once a widget release committed).
    LayerReleaseAnywhere,
    /// The two library managers (the dock's Edit / Organize... buttons).
    TextLibraryManager(text_manager::TextManagerMsg),
    SymbolOrganizer(symbol_organizer::SymbolOrganizerMsg),
    /// Recent Files (S11-049..053).
    RecentToggle,
    RecentOpen(usize),
    RecentClear,
    /// Calcs Expand All / Collapse All (S7-016/017).
    CalcsExpandAll,
    CalcsCollapseAll,
    /// `newSketch()` — the confirm + resetToBlank (Ctrl+N / File > New).
    New,
    /// `importFromFile` — merge another sketch's first page (S11-044).
    ImportSketch,
    /// The reference image's ribbon (screen 10, S10-007..020).
    PhotoLoad,
    PhotoShow,
    PhotoHide,
    PhotoRemove,
    PhotoScaleStart,
    PhotoSmaller,
    PhotoLarger,
    PhotoRotateLeft,
    PhotoRotateRight,
    PhotoEditModeToggle,
    PhotoOrientStart,
    PhotoFlipH,
    PhotoFlipV,
    PhotoIntensity(f32),
    /// The Geo popup (S10-021..024).
    GeoOpen,
    /// Help > About: the running build's version.
    About,
    GeoLoadMap,
    GeoCoordinate,
    GeoAddress,
    /// The canvas-side photo interactions' commits (scale/orient clicks).
    PhotoScaleClick(sketch_core::geometry::Pt),
    PhotoOrientClick(sketch_core::geometry::Pt),
    /// The Orient Guide's Horizontal/Vertical answer (`h`/`v` prefix).
    PhotoOrientApply(bool),
    /// A Geo prompt's value (mode, text).
    GeoCoordinateValue(String),
    GeoAddressValue(String),
    /// The Keyboard Shortcuts cheat sheet (S12-015).
    Shortcuts(shortcuts::ShortcutsMsg),
    /// The User Guide (Help > User Guide / Integration Guide).
    Guide(guide::GuideMsg),
    /// Bluetooth (Disto) connect: Tools > Connect, the status-bar button, the
    /// palette, and the chooser's controls.
    Disto(disto::DistoMsg),
    /// The Flip Sketch / Canvas background / Interface theme popup menus.
    PopupMenu(popup_menu::PopupMsg),
    /// The palette's route to one of them: the anchor is the ribbon button
    /// when its tab shows, else a hidden element's zero rect.
    PaletteMenu(popup_menu::Kind),
    /// View > Mini-Map / the palette's "Mini map" (`toggleMiniMap`).
    ToggleMiniMap,
    /// The Pages navigator panel.
    PagesNav(pages_nav::NavMsg),
    /// The window screenshot the map's snapshot is cropped from.
    MiniMapShot(window::Screenshot),
    MiniMapClose,
    /// File > Print... / the QAT printer: the report PDF, opened for printing.
    Print,
    /// File > Print Preview... and its toolbar.
    PrintPreviewOpen,
    PrintPreview(print::PreviewMsg),
    /// `emailSupport()` — the mailto opener (S12-026/071/084).
    Email,
    /// Backstage actions (screens 11/2): the exporters, CAD import.
    ExportPng,
    ExportSvg,
    ExportDxf,
    ExportPdf,
    ImportCad,
    /// The Color dialog (screen 5d-3).
    Color(color_dialog::ColorMsg),
    /// Home › Draw Color: `toggleDrawColorPalette`.
    OpenColorDialog,
    /// The length/area formatting options (screen 12 will edit them; the e2e
    /// uses it to render a metric reference state).
    Units(format::Units),
    /// `options.wallSolids` — draw walls as filled thickness solids.
    WallSolids(bool),
    /// `options.defineFirst` — block exterior drawing until a type is chosen.
    DefineFirst(bool),
    /// `state.selectedAreaType` / `selectedAreaName` (the dock's area panel).
    AreaType(Option<String>, Option<String>),
    /// Edit Tools: apply the length typed in the field (`apply-line-length`).
    ApplyLineLength,
    /// Screen 8: flip the selected hosted openings.
    FlipOpening,
    /// Screen 8: widen the selected openings.
    OpeningWider,
    /// Screen 8: narrow the selected openings.
    OpeningNarrower,
    SetSelectedTextStyle(String, String),
    ToggleSelectedTextStyle(String),
    AdjustSelectedTextSize(i32),
    /// The length field's own text, while it is being edited.
    LineLengthInput(String),
    /// Edit Tools › Line Style: solid -> dashed -> dotted -> solid.
    CycleLineStyle,
    /// Edit Tools › Line Color: open the Color dialog for the selected walls.
    LineColorClicked,
    /// The text group's Font select.
    TextFontPicked(String),
    /// Text Color: open the Color dialog for the selected label / area text.
    TextColorClicked,
    /// One of the text group's typed boxes changed / was submitted.
    TextFieldInput(ribbon::TextField, String),
    TextFieldApply(ribbon::TextField),
    /// Edit Tools › an area's Fill Pattern / Line Style / Line Thickness
    /// select: the next option applies (`setSelectedAreaStyle`).
    AreaCycle(area_props::AreaCycle),
    /// Edit Tools › an area's Fill Color (true) / Line Color (false): open
    /// the Color dialog.
    AreaColorClicked(bool),
    /// Edit Tools › Curve: switch to the curve tool (`curve-selected-line`).
    CurveSelected,
    /// `undo` / `redo` / `undo-history`.
    Undo,
    Redo,
    UndoHistory,
    /// A history row: rewind to that stack index (`undoTo`).
    UndoTo(usize),
    /// `cutSelected` / `copySelected` / `paste` / `selectAll` / `deleteSelected`.
    Cut,
    Copy,
    Paste,
    SelectAll,
    Delete,
    /// Edit Tools › Select: open the by-type menu (`select-by-type`).
    OpenSelectMenu,
    /// Edit Tools › Area Props (`area-properties`).
    AreaProps,
    AreaWorkflow(area_workflow::Msg),
    /// A row of that menu (`selectByType`).
    SelectByType(select_menu::SelectKind),
    /// A canvas context-menu row.
    ContextAction(context_menu::Action),
    /// The Area Properties dialog's controls.
    AreaDialog(area_dialog::AreaDialogMsg),
    /// The Area Attributes dialog's controls.
    AreaAttributes(attr_dialog::AttrMsg),
    /// Open the Area Attributes dialog (`editable` false is the Home view).
    OpenAreaAttributes(bool),
    /// View › View Calcs (`toggle-calcs`).
    ToggleCalcs,
    /// Tools › Area Codes (`area-code-settings`).
    OpenCodesEditor,
    /// Tools › Area Breakdown (`area-breakdown`).
    OpenBreakdown,
    /// The Options dialog (Tools › Options; File backstage › Options).
    Options(options_dialog::OptMsg),
    /// The Breakdown Details dialog's controls.
    Breakdown(breakdown_dialog::BreakdownMsg),
    /// The area-code table editor's controls.
    CodesEditor(codes_editor::CodesMsg),
    /// Areas dock › New (`showAddAreaCode`).
    OpenNewCode,
    /// The New Area Code dialog's controls.
    NewCode(new_code_dialog::NewCodeMsg),
    /// Areas dock › Edit areas… (`showAreaDefinitionEditor`).
    OpenDefEditor,
    /// The area-definition editor's controls.
    DefEditor(def_editor::DefMsg),
    /// A calcs group header (collapse/expand).
    CalcsGroup(String),
    /// A calcs area row: expand its details and select the area.
    CalcsArea(i64),
    /// A rail tool button.
    Rail(rail::RailTool),
    /// Canvas pan drag (window px): Pan tool, middle button, Space + left.
    PanStart(f32, f32),
    PanMove(f32, f32),
    PanEnd,
    /// Mouse wheel over the canvas (canvas px).
    Wheel {
        up: bool,
        x: f32,
        y: f32,
        /// Shift bows the last wall while drawing (S5-040).
        shift: bool,
        /// Ctrl uses the fine step.
        ctrl: bool,
    },
    /// A keyboard shortcut (InputController).
    Key(input::KeyCmd),
    /// The wall-shape menu (screen 5d-2).
    Shapes(shapes::ShapesMsg),
    /// A right press anywhere (the original's `contextmenu` handlers).
    RightPress(Point),
    /// Pointer over the canvas (canvas px) — `pointerWorld` + the live HUD.
    CanvasHover(f32, f32),
    /// A left click on the canvas (canvas px); `double` is the second click
    /// of a double-click (the canvas `dblclick` handler follows the click).
    CanvasClick {
        x: f32,
        y: f32,
        double: bool,
        /// Ctrl/Cmd: additive selection.
        ctrl: bool,
    },
    /// Right press on the canvas (`contextmenu`): the pen lifts first.
    CanvasContext,
}

/// The QAT's history button (`list-bullets`): 28x28, at x 20 + 28*3, y 1 —
/// the undo-history popup anchors under it.
/// `#file-open-input` / `#file-import-input` `accept=".skx,.apx,.json,.sketch"`.
const SKETCH_OPEN_EXTENSIONS: &[&str] = &["skx", "apx", "json", "sketch"];

/// A sketch file named on the command line, opened once the window is up.
static STARTUP_FILE: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Help > About: the version this exe was built as.
fn about_text() -> String {
    format!(
        "Sketch Rust {}
Windows x64 native compatibility preview",
        env!("CARGO_PKG_VERSION")
    )
}

fn is_sketch_file(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| {
        SKETCH_OPEN_EXTENSIONS
            .iter()
            .any(|s| ext.eq_ignore_ascii_case(s))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SketchFileFormat {
    Skx,
    Json,
    /// An opened `sketchy.json` file; Save never overwrites it (Save As only).
    SketchyJson,
}

/// A decoded sketch file: its save model, container format and an optional
/// notice about content the app keeps but does not display.
struct DecodedSketch {
    model: serde_json::Value,
    format: SketchFileFormat,
    notice: Option<String>,
}

/// Longest error text a toast shows.
const TOAST_ERROR_MAX: usize = 200;

/// Truncate an error message for a toast (on a character boundary).
fn toast_error(message: &str) -> String {
    if message.chars().count() <= TOAST_ERROR_MAX {
        return message.to_owned();
    }
    let cut: String = message.chars().take(TOAST_ERROR_MAX).collect();
    format!("{cut}...")
}

/// "Opened. Not shown in Sketch Rust: 12 openings, 1 structure."
fn unshown_notice(summary: &[(String, usize)]) -> Option<String> {
    if summary.is_empty() {
        return None;
    }
    let parts: Vec<String> = summary.iter().map(|(l, n)| format!("{n} {l}")).collect();
    Some(format!("Opened. Not shown in Sketch Rust: {}.", parts.join(", ")))
}

/// Decode `.sketchy.json`, `.skx` or save-model JSON bytes into a save model.
fn decode_sketch_bytes(bytes: &[u8]) -> Result<DecodedSketch, String> {
    // Only JSON text can be Sketchy; binary `.skx` bytes skip the Sketchy read.
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let looks_json = text.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{');
    let sketchy = if looks_json {
        sketch_core::sketchy::read(bytes)
    } else {
        Ok(None)
    };
    match sketchy {
        Ok(Some(doc)) => {
            let model =
                sketch_core::sketchy::convert::from_sketchy(&doc).map_err(|e| e.to_string())?;
            let notice = unshown_notice(&sketch_core::sketchy::unshown_summary(&doc));
            return Ok(DecodedSketch {
                model,
                format: SketchFileFormat::SketchyJson,
                notice,
            });
        }
        Ok(None) => {}
        Err(e) => return Err(e.to_string()),
    }
    let (model, format) = if let Ok(read) = sketch_core::skx::read(bytes) {
        (read.model, SketchFileFormat::Skx)
    } else if let Ok(model) = serde_json::from_slice::<serde_json::Value>(bytes) {
        (model, SketchFileFormat::Json)
    } else {
        return Err("Invalid sketch file format.".into());
    };
    Ok(DecodedSketch {
        model,
        format,
        notice: None,
    })
}

/// True when `path` names a Sketchy JSON file (`*.sketchy.json`).
fn is_sketchy_name(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".sketchy.json"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CurrentFile {
    path: std::path::PathBuf,
    format: SketchFileFormat,
}

const QAT_HISTORY_X: f32 = 104.0;
const QAT_Y: f32 = 1.0;
const QAT_SIZE: f32 = 28.0;

/// The Edit Tools › Select button: x 213, y 68, 46x49 in the ribbon body; the
/// by-type popup opens 2px under it.
const SELECT_BUTTON_X: f32 = 213.0;
const SELECT_BUTTON_Y: f32 = 68.40625;
const SELECT_BUTTON_H: f32 = 49.1875;

/// How far the pointer travels before a press on a dock item is a drag.
const DRAG_START_PX: f32 = 5.0;

/// A dock symbol / text item being dragged toward the canvas.
#[derive(Debug, Clone)]
struct DockDrag {
    item: dock::DragItem,
    /// Where the pointer first moved after the press.
    origin: Option<Point>,
    /// Past the threshold: it is a drag, not a click.
    active: bool,
    /// The last pointer position (window CSS px).
    cursor: Option<Point>,
    /// An area-code drag: the region under the pointer (its ring) that a drop
    /// would define, and the code's colour.
    hover: Option<(Vec<sketch_core::geometry::Pt>, iced::Color)>,
}

pub struct App {
    apex_snapshot_epoch: u64,
    apex_snapshot_busy: bool,
    apex_snapshot_replies: Vec<std::path::PathBuf>,
    apex_check_pending: bool,
    apex_last_signature: u64,
    apex_pending_signature: u64,
    apex_changed_at: std::time::Instant,
    apex_geo_pending: Option<(usize, Vec<sketch_core::geometry::Pt>)>,
    apex_session: Option<std::path::PathBuf>,
    apex_target: String,
    apex_returning: bool,
    symbol_changes: symbol_manager::Changes,
    symbol_manager: symbol_manager::Manager,
    autosave: autosave::State,
    autosave_prompt: bool,
    window: Option<window::Id>,
    /// CSS px → logical px factor handed to iced (`dpr / system scale`).
    ui_scale: f32,
    close_hover: bool,
    /// The hash of the save model as last saved, opened or reset.
    saved_hash: String,
    /// The document differs from `saved_hash` (see `refresh_dirty`).
    dirty: bool,
    dirty_revision: u64,
    dirty_checked: std::time::Instant,
    dirty_cost: std::time::Duration,
    /// The window is maximized — the title bar shows the restore icon.
    maximized: bool,
    /// The window's restored bounds + maximized flag, as they will be
    /// stored (`window_state`); `None` until the window reports them.
    placement: Option<window_state::Placement>,
    /// What `window.json` already holds, so it is written only on change.
    placement_stored: Option<window_state::Placement>,
    /// The latest reported position / size, maximized or not.
    last_window_pos: Option<Point>,
    last_window_size: Option<Size>,
    /// The tab carrying `.active` (File too, once clicked).
    active_tab: TabId,
    /// `state.symbolFavorites` — the saved list, None while the dock lists
    /// the library defaults (`loadLibraryState`).
    symbol_favorites: Option<Vec<String>>,
    /// `isEntityVisible`'s layer half, refreshed at the end of every
    /// update (the view's render + hit gates read it).
    gates_cache: crate::layers::LayerGates,
    canvas_pointer: std::cell::Cell<Option<(f32, f32)>>,
    canvas_cache: iced::widget::canvas::Cache,
    pointer_scene_ready: bool,
    /// The `.ribbon-content` on display.
    body: Body,
    /// `#file-menu` shown.
    file_menu: bool,
    /// `#sidebar-dock` state (`state.sidebar` + library panels).
    dock: dock::DockState,
    /// The external symbol library (`symbols.json`, read at runtime).
    symbols: Arc<Result<symlib::SymIndex, String>>,
    /// `#app-toast`.
    toasts: toast::Toasts,
    /// Pan/zoom, scale, grid/snap, mode, fence, filters.
    view: view_state::View,
    /// The walls and the live chain (screen 5b).
    drawing: drawing::Drawing,
    /// The Color dialog (screen 5d-3).
    color: color_dialog::ColorDialog,
    /// The selection (screen 6).
    selection: selection::Selection,
    /// `state.undoStack` / `redoStack` (screen 6c).
    undo: undo::UndoStack,
    /// `state.clipboard` (screen 6d).
    clipboard: Option<clipboard::Clipboard>,
    /// The `undo-history` popup.
    history: undo_history::HistoryView,
    /// The `select-by-type` popup (screen 6e).
    select_menu: select_menu::SelectMenu,
    /// The canvas context menu (screen 6f).
    context: context_menu::ContextMenu,
    /// The Area Properties dialog (screen 7).
    area_dialog: area_dialog::AreaDialog,
    /// The Area Attributes dialog (screen 7).
    attr_dialog: attr_dialog::AttrDialog,
    /// The area-code table editor (screen 7).
    codes_editor: codes_editor::CodesEditor,
    /// The Options dialog (Screen 11, S11-001..032).
    options_dialog: options_dialog::OptionsDialog,
    /// `state.pageLayout` (Screen 11's File > Page Layout).
    page_layout: page_layout::PageLayout,
    /// The Page Layout dialog (Screen 11, S11-033..037).
    page_layout_dialog: page_layout::PageLayoutDialog,
    /// The Text Library manager (S9-008..013, the dock's Edit button).
    text_manager: text_manager::TextManager,
    /// The "Organize Favorite Symbols" dialog (S8-008 as captured).
    symbol_organizer: symbol_organizer::SymbolOrganizer,
    /// Recent Files (S11-049..053): newest first, capped at 10.
    recent: Vec<recent::RecentFile>,
    /// The Recent Files submenu's open state (the File backstage's).
    recent_open: bool,
    /// The reference image ("photometrics", screen 10).
    bg_image: reference_image::BgImage,
    /// The Image Edit Mode's drag: the drag's last world point.
    photo_drag: Option<sketch_core::geometry::Pt>,
    /// The Geo popup's open state.
    geo_menu: reference_image::GeoMenu,
    /// The Keyboard Shortcuts cheat sheet (S12-015).
    shortcuts: shortcuts::Shortcuts,
    /// The User Guide overlay.
    guide: guide::Guide,
    /// The Print Preview overlay (S11-055).
    print_preview: print::PrintPreview,
    integration_setup: integration_setup::Setup,
    /// The Mini-Map's snapshot while it is open (S12-018).
    mini_map: Option<iced::widget::image::Handle>,
    /// The open ribbon popup menu (Flip / Canvas background / Interface).
    popup_menu: popup_menu::PopupMenu,
    /// Bluetooth (Disto) chooser and the `#disto-status` chip.
    disto: disto::Disto,
    /// The New Area Code dialog (screen 7).
    new_code: new_code_dialog::NewCodeDialog,
    /// The area-definition editor (screen 7).
    def_editor: def_editor::DefEditor,
    /// The Breakdown Details dialog (screen 7).
    breakdown: breakdown_dialog::BreakdownDialog,
    /// The symbol currently being placed (screen 8). `Some(id)` arms
    /// placement mode; canvas clicks call `placeSymbolAt`.
    placing_symbol: Option<String>,
    placing_text: Option<String>,
    /// The dock item being dragged to the canvas (`application/x-sketch-item`).
    dock_drag: Option<DockDrag>,
    /// A press-and-release on a dock item that no drag took (its click),
    /// held for the item's own release that follows in the same event.
    dock_click: Option<dock::DragItem>,
    /// The wall faces a drop looks regions up in, until a wall changes.
    region_cache: define_area::RegionCache,
    workflow: area_workflow::State,
    /// `app.areaCodes` — the live table the editor and the dialogs read.
    area_codes: Vec<sketch_core::geometry::AreaCode>,
    /// `app.areaTotals` — refreshed by `recalculate`.
    ///
    /// The recalculation itself mirrors the original's `paletteSignature`
    /// guard: it runs only when the polygons or the code table changed, so
    /// pointer movement and panning cannot trigger a pass.
    calcs_totals: Option<calcs::AreaTotals>,
    /// Hash of the last recalculation's inputs.
    last_recalc_sig: u64,
    /// The last recalculation's per-polygon values.
    last_recalc: Option<sketch_core::area_calc::RecalcResult>,
    /// `last_recalc`'s per-polygon values, kept for the canvas (labels read the net area).
    computed: std::collections::BTreeMap<i64, calcs::Computed>,
    /// Whether the open drag has already banked its undo step.
    drag_banked: bool,
    /// The length field's text while it is being edited.
    line_length_edit: Option<String>,
    /// What the Color dialog's OK applies to.
    color_target: color_dialog::ColorTarget,
    /// The tab showing before the contextual Edit Tools tab took over.
    tab_before_edit_tools: Option<(ribbon::TabId, ribbon::Body)>,
    /// `options.stepUnit` — the arrow keys' step.
    step_unit: drawing::StepUnit,
    /// The wall-shape menu and its prompt chain (screen 5d-2).
    shapes: shapes::Shapes,
    /// Layers (screen 10).
    layers: layers::Layers,
    /// Room shapes dialog (screen 9).
    room_shapes: room_shapes::RoomShapes,
    /// Pending room shape awaiting placement.
    pending_shape: Option<room_shapes::PendingShape>,
    subject_info: subject_info::SubjectInfo,
    /// The Subject Info dialog's draft (`editSubjectInfo`'s form).
    subject_dialog: subject_info::SubjectDialog,
    /// `localStorage sketch.defaultSubjectComments`.
    default_comments: String,
    /// `this.fileName` — set by the first save / Save As / open file name.
    file_name: String,
    /// Session-only destination; never embedded in portable sketch data.
    current_file: Option<CurrentFile>,
    /// `state.subjectInfoAutoFilled`.
    subject_auto_filled: bool,
    /// `state.extendedData` (the suite sets it; carried verbatim here).
    extended_data: String,
    native_apex: serde_json::Value,
    /// Settings keys this port does not yet model (`pageLayout`,
    /// `canvasTheme`, `autoPost` defaults, `symbolFavorites`,
    /// `dimensionsEnabled`, `dimensionPlacement`): read on `applySaveModel`
    /// and written back verbatim, never invented.
    settings_extra: std::collections::BTreeMap<String, serde_json::Value>,
    /// The status bar's vertical scrollbar is showing (see
    /// `sync_status_scroll`).
    status_scroll: bool,
    /// The 1:N input's text while it is being edited.
    scale_edit: Option<String>,
    /// Pages, the current page and the underlay.
    pages: pages::Pages,
    /// `#area-status-select.disabled` — set once a page load has run
    /// updateCalcsPanel with no areas.
    areas_disabled: bool,
    /// The open modal dialog.
    dialog: Option<dialog::Dialog>,
    /// Command palette (Ctrl+K).
    command_palette_open: bool,
    command_palette_query: String,
    command_palette_selected: usize,
    touch_panel_open: bool,
    touch_distance: f64,
    /// The window's size in CSS px.
    window_size: Size,
    /// The startup maximize is pending: the original would have loaded at
    /// the maximized size, so that one resize recenters the pan.
    recenter_on_resize: bool,
    /// `body.pages-nav-open`: the Pages navigator column beside the rail.
    pages_nav_open: bool,
    /// The navigator row under the pointer (its delete button shows).
    pages_nav_hover: Option<usize>,
    /// The area last picked in the status bar's select.
    status_area: Option<i64>,
    /// The text group's box being typed in (until Enter applies it).
    text_edit: Option<(ribbon::TextField, String)>,
    /// `UIManager.defaultOptions`: the bundled options with any saved
    /// defaults laid over them - what Restore Defaults applies.
    default_options: crate::options::Options,
}

/// The canvas size inside a `w` x `h` (CSS px) window with the dock at
/// `dock_w` and the Pages navigator `nav_w` wide: minus the 41px rail column
/// and the 157px of chrome rows.
fn canvas_size(w: f32, h: f32, dock_w: f32, nav_w: f32) -> (f32, f32) {
    (w - 41.0 - nav_w - dock_w, h - 30.0 - 99.0 - 28.0)
}

/// `areaCodes` in the save model (`window.AreaCodes` serialisation).
fn area_codes_save(codes: &[sketch_core::geometry::AreaCode]) -> serde_json::Value {
    match sketch_core::geometry::area_codes::to_json(codes.to_vec()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
}

/// `new Date().toISOString()` — UTC ISO-8601 from the Unix epoch without
/// pulling in a date dependency (Howard Hinnant's `civil_from_days`).
pub fn iso_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (hours, minutes, seconds) = ((secs % 86_400) / 3_600, (secs % 3_600) / 60, secs % 60);
    let days = (secs / 86_400) as i64;
    // days → civil date
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (yoe + era * 400) + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.000Z")
}

impl App {
    /// `recalcAutoSubtract` → `recalculate()` — the shared pipeline
    /// (`AreaCalc.recalculate`), guarded by a signature like the original's
    /// `paletteSignature`: only a real geometry or code-table change triggers
    /// a pass, so pointer movement and panning cannot.
    fn recalculate_if_needed(&mut self) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.drawing.options.wall_solids.hash(&mut hasher);
        self.drawing
            .options
            .exterior_thickness
            .to_bits()
            .hash(&mut hasher);
        self.drawing
            .options
            .interior_thickness
            .to_bits()
            .hash(&mut hasher);
        for line in &self.drawing.lines {
            line.id.hash(&mut hasher);
            std::mem::discriminant(&line.kind).hash(&mut hasher);
            line.thickness.map(f64::to_bits).hash(&mut hasher);
        }
        sketch_core::geometry::area_codes::to_json(self.area_codes.clone())
            .unwrap_or_default()
            .hash(&mut hasher);
        for poly in &self.drawing.polygons {
            poly.id.hash(&mut hasher);
            poly.kind.hash(&mut hasher);
            poly.name.hash(&mut hasher);
            poly.measure_to.hash(&mut hasher);
            poly.factor_override.map(|v| v.to_bits()).hash(&mut hasher);
            poly.subtract_mode.hash(&mut hasher);
            poly.subtract_target_id.hash(&mut hasher);
            for pt in &poly.points {
                pt.x.to_bits().hash(&mut hasher);
                pt.y.to_bits().hash(&mut hasher);
            }
            for edge in &poly.edges {
                edge.line_id.hash(&mut hasher);
                edge.bow_angle.to_bits().hash(&mut hasher);
            }
        }
        let sig = hasher.finish();
        if self.last_recalc_sig != 0 && sig == self.last_recalc_sig {
            return;
        }
        self.invalidate_canvas();
        self.last_recalc_sig = sig;
        let result = self.drawing.recalculate_areas(
            &self.drawing.polygons,
            &self.drawing.lines,
            &self.area_codes,
        );
        for (poly, calc) in self.drawing.polygons.iter_mut().zip(&result.polygons) {
            poly.gross = calc.gross;
            poly.area = calc.area;
        }
        self.calcs_totals = Some(calcs::AreaTotals {
            total_gla: result.totals.total_gla,
            gba: result
                .totals
                .totals_by_group
                .get("GBA")
                .copied()
                .unwrap_or(0.0),
        });
        self.last_recalc = Some(result);
        self.computed = self.recomputed();
    }

    /// The computed per-polygon values the calcs tree reads.
    fn recomputed(&self) -> std::collections::BTreeMap<i64, calcs::Computed> {
        self.last_recalc
            .as_ref()
            .map(|r| {
                r.polygons
                    .iter()
                    .map(|c| {
                        (
                            c.id,
                            calcs::Computed {
                                subtracted: c.subtracted,
                                factor: c.factor,
                                net: c.net,
                                perimeter: c.perimeter,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `toText` over the last recalculation's values — the report the dialog
    /// shows and copies. Empty when the pass has not run yet.
    fn breakdown_report(&self) -> String {
        let results = self.breakdown_results();
        sketch_core::breakdown::to_text(&results, &self.area_codes, &self.breakdown.options(), true)
    }

    /// The last recalculation as compute_all's results (the downloads and
    /// the report read the same numbers).
    fn breakdown_results(&self) -> Vec<sketch_core::breakdown::Result> {
        let recalc = match &self.last_recalc {
            Some(r) => r,
            None => return Vec::new(),
        };
        let sources: Vec<sketch_core::breakdown::Source> = recalc
            .polygons
            .iter()
            .map(|c| sketch_core::breakdown::Source {
                id: c.id,
                kind: &c.kind,
                name: &c.name,
                points: &c.points,
                edges: &c.edges,
                gross: Some(c.gross),
                container_id: c.container_id,
                subtracted_from_id: c.subtracted_from_id,
                subtracted: c.subtracted,
                multiplier: c.factor,
                sign: c.sign,
                net: Some(c.net),
                perimeter: Some(c.perimeter),
            })
            .collect();
        sketch_core::breakdown::compute_all(&sources, &self.breakdown.options())
    }

    /// The geometry engine, for the recalculation pass.
    pub fn geo(&self) -> &sketch_core::geometry::GeometryEngine {
        self.drawing.engine()
    }

    fn boot(ui_scale: f32) -> (Self, Task<Message>) {
        let dock = dock::DockState {
            area_codes: sketch_core::geometry::area_codes::default_area_codes(),
            ..dock::DockState::default()
        };
        // The window opens where it was left (see `program`), so the first
        // layout uses that size rather than the default.
        let placement = window_state::load(ui_scale, system_scale());
        let window_size = placement.map_or(Size::new(WINDOW_W, WINDOW_H), |p| p.size());
        let (cw, ch) = canvas_size(window_size.width, window_size.height, dock.width(), 0.0);
        (
            // The real app reads the recent-files sidecar at boot; the e2e
            // boots through this factory too, so the read gates on the
            // env main() sets.
            {
                let mut app = App {
                    apex_snapshot_epoch: 0,
                    apex_snapshot_busy: false,
                    apex_snapshot_replies: Vec::new(),
                    apex_check_pending: true,
                    apex_last_signature: 0,
                    apex_pending_signature: 0,
                    apex_changed_at: std::time::Instant::now(),
                    apex_geo_pending: None,
                    apex_session: None,
                    apex_target: String::new(),
                    apex_returning: false,
                    symbol_changes: Default::default(),
                    symbol_manager: Default::default(),
                    autosave: Default::default(),
                    autosave_prompt: false,
                    window: None,
                    ui_scale,
                    close_hover: false,
                    saved_hash: String::new(),
                    dirty: false,
                    dirty_revision: 0,
                    dirty_checked: std::time::Instant::now(),
                    dirty_cost: std::time::Duration::ZERO,
                    maximized: placement.is_some_and(|p| p.maximized),
                    placement,
                    placement_stored: placement,
                    last_window_pos: None,
                    last_window_size: None,
                    active_tab: TabId::Home,
                    symbol_favorites: None,
                    gates_cache: crate::layers::LayerGates::default(),
                    canvas_pointer: Default::default(),
                    canvas_cache: Default::default(),
                    pointer_scene_ready: false,
                    body: Body::Home,
                    file_menu: false,
                    dock,
                    symbols: symlib::library(),
                    toasts: toast::Toasts::default(),
                    view: view_state::View::new(cw as f64, ch as f64),
                    drawing: drawing::Drawing::default(),
                    color: color_dialog::ColorDialog::new(),
                    selection: selection::Selection::default(),
                    undo: undo::UndoStack::default(),
                    clipboard: None,
                    history: undo_history::HistoryView::default(),
                    select_menu: select_menu::SelectMenu::default(),
                    context: context_menu::ContextMenu::default(),
                    area_dialog: area_dialog::AreaDialog::default(),
                    attr_dialog: attr_dialog::AttrDialog::default(),
                    codes_editor: codes_editor::CodesEditor::default(),
                    options_dialog: options_dialog::OptionsDialog::default(),
                    page_layout: page_layout::PageLayout::default(),
                    page_layout_dialog: page_layout::PageLayoutDialog::default(),
                    text_manager: text_manager::TextManager::default(),
                    symbol_organizer: symbol_organizer::SymbolOrganizer::default(),
                    recent: Vec::new(),
                    recent_open: false,
                    bg_image: reference_image::BgImage::default(),
                    photo_drag: None,
                    geo_menu: reference_image::GeoMenu::default(),
                    shortcuts: shortcuts::Shortcuts::default(),
                    guide: guide::Guide::default(),
                    print_preview: print::PrintPreview::default(),
                    integration_setup: integration_setup::Setup::default(),
                    mini_map: None,
                    popup_menu: popup_menu::PopupMenu::default(),
                    disto: disto::Disto::default(),
                    new_code: new_code_dialog::NewCodeDialog::default(),
                    def_editor: def_editor::DefEditor::default(),
                    breakdown: breakdown_dialog::BreakdownDialog::default(),
                    area_codes: sketch_core::geometry::area_codes::default_area_codes(),
                    calcs_totals: None,
                    last_recalc_sig: 0,
                    last_recalc: None,
                    computed: Default::default(),
                    drag_banked: false,
                    line_length_edit: None,
                    color_target: color_dialog::ColorTarget::DrawColor,
                    tab_before_edit_tools: None,
                    step_unit: drawing::StepUnit::from_option(drawing::DEFAULT_STEP_UNIT),
                    shapes: shapes::Shapes::default(),
                    layers: layers::Layers::default(),
                    room_shapes: room_shapes::RoomShapes::default(),
                    pending_shape: None,
                    subject_info: subject_info::SubjectInfo::default(),
                    subject_dialog: subject_info::SubjectDialog::default(),
                    default_comments: String::new(),
                    file_name: "Untitled".to_string(),
                    current_file: None,
                    subject_auto_filled: false,
                    extended_data: String::new(),
                    native_apex: serde_json::Value::Null,
                    settings_extra: std::collections::BTreeMap::new(),
                    status_scroll: false,
                    scale_edit: None,
                    pages: pages::Pages::default(),
                    areas_disabled: false,
                    dialog: None,
                    command_palette_open: false,
                    command_palette_query: String::new(),
                    command_palette_selected: 0,
                    touch_panel_open: false,
                    touch_distance: 1.0,
                    placing_symbol: None,
                    placing_text: None,
                    dock_drag: None,
                    dock_click: None,
                    region_cache: Default::default(),
                    workflow: Default::default(),
                    window_size,
                    // A window restored maximized is laid out for its
                    // restored size until the first resize arrives.
                    recenter_on_resize: placement.is_some_and(|p| p.maximized),
                    pages_nav_open: false,
                    pages_nav_hover: None,
                    status_area: None,
                    text_edit: None,
                    default_options: crate::options::Options::default(),
                };
                if std::env::var_os("SKETCH_PERSIST").is_some() {
                    app.load_recent();
                    app.load_default_comments();
                    app.load_interface_theme();
                }
                // Self-gated (`persist::enabled`): the real app, or a test's
                // private store.
                app.load_stored_settings();
                app.symbol_changes = persist::read_json("symbol-library.json").unwrap_or_default();
                app.rebuild_symbols();
                if let Err(error) = app.apply_host_codes() {
                    let _ = app.toasts.show(error);
                }
                app.autosave_boot();
                if let Some(root) = std::env::var_os("SKETCH_APEX_SESSION") {
                    if let Err(error) = app.load_apex_context(std::path::Path::new(&root)) {
                        let _ = app.toasts.show(error);
                    }
                }
                app
            },
            window::latest().map(Message::WindowReady),
        )
    }

    /// The canvas's CSS size follows the window and the dock width.
    fn sync_canvas(&mut self) {
        let (cw, ch) = canvas_size(
            self.window_size.width,
            self.window_size.height,
            self.dock.width(),
            self.pages_nav_width(),
        );
        self.view.canvas = (cw as f64, ch as f64);
    }

    /// The view's side of the measurement-label tests.
    fn dim_view(&self) -> handles::DimView<'_> {
        handles::DimView {
            drawing: &self.drawing,
            pan: self.view.pan,
            zoom: self.view.zoom,
            placement: self.view.dimension_placement,
            enabled: self.view.dimensions,
            filter_on: self.view.filters[3],
        }
    }

    /// The measurement label under `world` in Select mode - after the
    /// 4 px on-the-wall pass, which wins (`hitTestAt`).
    fn dimension_hit_at(&self, world: sketch_core::geometry::Pt) -> Option<i64> {
        if self.view.mode != view_state::Mode::Select {
            return None;
        }
        let gates = self.layer_gates();
        let selectable = |id: i64| gates.selectable(id);
        if selection::on_wall(world, &self.drawing.lines, self.view.zoom, &selectable) {
            return None;
        }
        handles::hit_dimension(&self.dim_view(), world, &selectable)
    }

    /// What a fence or lasso picks by position.
    fn pickable(&self) -> selection::Pickable<'_> {
        let gates = self.layer_gates();
        let view = self.dim_view();
        let dimensions = handles::visible_dimension_ids(&view, &|id| gates.selectable(id))
            .into_iter()
            .filter_map(|id| {
                let line = self.drawing.lines.iter().find(|l| l.id == id)?;
                Some((id, handles::dimension_label_world(&view, line)?))
            })
            .collect();
        let area_text = self
            .drawing
            .polygons
            .iter()
            .filter(|p| !p.kind.is_empty() && p.kind != "UND")
            .map(|p| {
                (
                    p.id,
                    handles::label_position(&self.drawing, p),
                    handles::size_position(&self.drawing, p),
                )
            })
            .collect();
        selection::Pickable {
            texts: &self.drawing.texts,
            symbols: &self.drawing.symbols,
            area_text,
            dimensions,
        }
    }

    /// `moveSelected(dx, dy)`: the geometry moves, and a picked measurement
    /// label (whose wall is not itself moving) pins where it draws now - plus
    /// the move - against the wall's middle, so it follows the wall after.
    fn move_selection(&mut self, dx: f64, dy: f64) {
        let pins: Vec<(i64, sketch_core::geometry::Pt)> = {
            let view = self.dim_view();
            self.drawing
                .lines
                .iter()
                .filter(|l| {
                    self.selection.dimensions.contains(&l.id)
                        && !self.selection.lines.contains(&l.id)
                })
                .filter_map(|l| {
                    let now = drawing::pinned_dimension_point(l)
                        .or_else(|| handles::dimension_label_world(&view, l))?;
                    Some((l.id, now))
                })
                .collect()
        };
        transform::move_selected(&mut self.drawing, &self.selection, dx, dy);
        for (id, now) in pins {
            if let Some(line) = self.drawing.lines.iter_mut().find(|l| l.id == id) {
                line.dimension_position =
                    Some(sketch_core::geometry::Pt::new(now.x + dx, now.y + dy));
                line.dimension_anchor = Some(sketch_core::geometry::Pt::new(
                    (line.start.x + line.end.x) / 2.0,
                    (line.start.y + line.end.y) / 2.0,
                ));
            }
        }
    }

    /// `toggleSelectedDimensions`: each selected wall (or wall whose label is
    /// picked) flips its label hidden / shown; hiding drops the pin. One
    /// `Toggle Dimensions` undo step.
    fn toggle_selected_dimensions(&mut self) {
        let ids: std::collections::HashSet<i64> = self
            .selection
            .lines
            .iter()
            .chain(self.selection.dimensions.iter())
            .copied()
            .collect();
        if ids.is_empty() {
            return;
        }
        self.push_undo("Toggle Dimensions");
        for line in &mut self.drawing.lines {
            if !ids.contains(&line.id) {
                continue;
            }
            line.dimension_hidden = !line.dimension_hidden;
            if line.dimension_hidden {
                line.dimension_position = None;
                line.dimension_anchor = None;
            }
            self.selection.dimensions.remove(&line.id);
        }
        self.sync_canvas();
    }

    /// The area text block under `world`, if any (`hitTestAt`'s first pass).
    fn area_text_at(&self, world: sketch_core::geometry::Pt) -> Option<(handles::TextKind, i64)> {
        let gates = self.layer_gates();
        handles::area_text_hit(
            &self.drawing,
            &self.computed,
            &|id| gates.selectable(id),
            world,
            self.view.pan,
            self.view.zoom,
        )
    }

    /// Writes a rotation to a text entity (`entity.rotation` /
    /// `labelRotation` / `areaSizeRotation`).
    fn set_text_rotation(&mut self, kind: handles::TextKind, id: i64, degrees: f64) {
        match kind {
            handles::TextKind::Label => {
                if let Some(t) = self.drawing.texts.iter_mut().find(|t| t.id == id) {
                    t.rotation = degrees;
                }
            }
            handles::TextKind::AreaLabel => {
                if let Some(p) = self.drawing.polygons.iter_mut().find(|p| p.id == id) {
                    p.label_rotation = Some(degrees);
                }
            }
            handles::TextKind::AreaSize => {
                if let Some(p) = self.drawing.polygons.iter_mut().find(|p| p.id == id) {
                    p.area_size_rotation = Some(degrees);
                }
            }
        }
    }

    /// The defined-areas select's options and the area it shows
    /// (`updateCalcsPanel`): every defined area as `code  name  gross`, the
    /// shown one being the selected area, else the last pick, else the last.
    fn status_areas(&self) -> (Vec<statusbar::AreaOption>, Option<i64>) {
        let defined: Vec<&sketch_core::model::Polygon> = self
            .drawing
            .polygons
            .iter()
            .filter(|p| !p.kind.is_empty() && p.kind != "UND")
            .collect();
        let rows: Vec<(i64, String, String, f64)> = defined
            .iter()
            .map(|p| {
                let name = if p.name.is_empty() {
                    p.kind.clone()
                } else {
                    p.name.clone()
                };
                let area = if p.gross != 0.0 { p.gross } else { p.area };
                (p.id, p.kind.clone(), name, area)
            })
            .collect();
        let selected = defined
            .iter()
            .find(|p| self.selection.polys.contains(&p.id))
            .map(|p| p.id)
            .or(self
                .status_area
                .filter(|id| defined.iter().any(|p| p.id == *id)));
        (statusbar::area_options(&rows), selected)
    }

    /// The Pages navigator's width while open (0 closed).
    fn pages_nav_width(&self) -> f32 {
        if self.pages_nav_open {
            pages_nav::WIDTH
        } else {
            0.0
        }
    }

    /// The Pages navigator (S10-040..045).
    fn pages_nav_msg(&mut self, m: pages_nav::NavMsg) -> Task<Message> {
        use pages_nav::NavMsg as N;
        match m {
            N::Toggle => {
                self.pages_nav_open = !self.pages_nav_open;
                self.pages_nav_hover = None;
                self.sync_canvas();
            }
            N::Close => {
                self.pages_nav_open = false;
                self.pages_nav_hover = None;
                self.sync_canvas();
            }
            N::Go(index) => {
                if index != self.pages.current {
                    self.go_to_page(index);
                }
            }
            N::Ghost(index) => {
                let next = if self.pages.underlay == Some(index) {
                    None
                } else {
                    Some(index)
                };
                self.pages.set_underlay(next);
            }
            N::Delete(index) => {
                // The last page cannot go (the button does nothing);
                // `deletePage()` removes the CURRENT page, so go there first.
                if self.pages.len() <= 1 {
                    return Task::none();
                }
                if index != self.pages.current {
                    self.go_to_page(index);
                }
                return self.status(statusbar::StatusMsg::PageDelete);
            }
            N::Underlay(index) => self.pages.set_underlay(index),
            N::Hover(row) => self.pages_nav_hover = row,
        }
        Task::none()
    }

    /// Bounds of the live page, including the visible sweep of bowed walls.
    fn sketch_view_bounds(&self) -> Option<(view_state::World, view_state::World)> {
        use sketch_core::geometry::{Edge, GeometryEngine, Pt};
        let geometry = GeometryEngine::new();
        let mut min = Pt::new(f64::INFINITY, f64::INFINITY);
        let mut max = Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut include = |p: Pt| {
            if p.x.is_finite() && p.y.is_finite() {
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
            }
        };
        for line in &self.drawing.lines {
            include(line.start);
            include(line.end);
            if line.start.x.is_finite()
                && line.start.y.is_finite()
                && line.end.x.is_finite()
                && line.end.y.is_finite()
                && line.bow_angle != 0.
                && geometry.validate_bow_angle(line.bow_angle).is_ok()
            {
                for p in geometry.flatten_ring(
                    &[line.start, line.end],
                    &[Edge {
                        bow_angle: line.bow_angle,
                        ..Edge::default()
                    }],
                    0.01,
                ) {
                    include(p);
                }
            }
        }
        // Imported polygons can have their boundary points without wall records.
        if self.drawing.lines.is_empty() {
            for polygon in &self.drawing.polygons {
                for &p in &polygon.points {
                    include(p);
                }
            }
        }
        let library = self.symbols_lib();
        for symbol in &self.drawing.symbols {
            if let Some(def) = library.symbol(&symbol.symbol_type) {
                for p in sketch_core::symbols::world_corners(
                    def.bounds,
                    symbol.x,
                    symbol.y,
                    symbol.rotation,
                    symbol.scale,
                    symbol.flip_h,
                ) {
                    include(p);
                }
            }
        }
        min.x.is_finite().then_some((
            view_state::World { x: min.x, y: min.y },
            view_state::World { x: max.x, y: max.y },
        ))
    }

    /// Status bar controls (refs/ui-manager.js bindEvents).
    fn status(&mut self, m: statusbar::StatusMsg) -> Task<Message> {
        use statusbar::StatusMsg as S;
        match m {
            S::CenterSketch => self.view.center_sketch(self.sketch_view_bounds()),
            S::CenterFit => self.view.center_and_fit(self.sketch_view_bounds()),
            S::ZoomFence => return self.set_mode(view_state::Mode::ZoomFence),
            S::ToggleGrid => {
                self.view.grid = !self.view.grid;
                self.drawing.options.grid_enabled = self.view.grid;
            }
            S::CycleDimensions => {
                let toast = self.view.cycle_dimensions();
                return self.toasts.show(toast.into());
            }
            S::ToggleSnap => self.view.snap = !self.view.snap,
            S::ToggleAreaSnap => self.view.area_snap = !self.view.area_snap,
            S::Pitch(p) => self.view.set_area_snap_size(p.value),
            S::ToggleFilter => self.view.filter_open = !self.view.filter_open,
            S::Filter(i) => self.view.filters[i] = !self.view.filters[i],
            // showPanel('panel-calcs') — always opens the dock.
            S::TotalsChip => {
                self.dock.update(dock::DockMsg::Show(dock::DockTab::Calcs));
                self.sync_canvas();
            }
            S::ZoomOut => self.view.zoom_out(),
            S::ZoomIn => self.view.zoom_in(),
            S::ZoomSlider(v) => self.view.zoom_slider(v),
            S::ScaleEdit(s) => self.scale_edit = Some(s),
            S::North => {
                // `String(Math.round(northAngle))` as the default value.
                let current = format!("{}", self.pages.page().north_angle.round() as i64);
                return self.open_dialog(dialog::Dialog::prompt(
                    "North direction in degrees (0 = up, clockwise):",
                    &current,
                    dialog::Action::SetNorth,
                ));
            }
            S::PagePrev => {
                if self.pages.can_go_prev() {
                    self.go_to_page(self.pages.current - 1);
                } else {
                    return self.toasts.show("This is the first page.".into());
                }
            }
            S::PageNext => {
                if !self.pages.is_last() {
                    self.go_to_page(self.pages.current + 1);
                } else {
                    return self.open_dialog(dialog::Dialog::confirm(
                        "Add a page after the current page?",
                        dialog::Action::AddPage,
                    ));
                }
            }
            S::PageFirst => self.go_to_page(0),
            S::PageLast => self.go_to_page(self.pages.len() - 1),
            S::PageAdd => self.add_page(),
            S::PageDelete => {
                let d = if self.pages.len() <= 1 {
                    dialog::Dialog::alert("Cannot delete the last page.")
                } else {
                    dialog::Dialog::confirm(
                        &format!("Delete page {}?", self.pages.current + 1),
                        dialog::Action::DeletePage,
                    )
                };
                return self.open_dialog(d);
            }
            S::Underlay(i) => self.pages.set_underlay(i),
            S::AreaPick(id) => {
                // `#area-status-select` change: that area alone is selected.
                if self.drawing.polygons.iter().any(|p| p.id == id) {
                    self.selection.clear();
                    self.selection.polys.insert(id);
                    self.status_area = Some(id);
                    self.sync_edit_tools();
                }
            }
            S::ScaleSubmit => {
                if let Some(s) = self.scale_edit.take() {
                    // Number(value): blank or junk re-syncs the input.
                    if let Ok(n) = s.trim().parse::<f64>() {
                        self.view.set_drawing_scale(n);
                    }
                }
            }
        }
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        // Window bookkeeping arrives on its own schedule (at startup, while
        // the window is dragged) and never touches the document: keep it
        // off the path below, which ends a pending dock click, repaints the
        // canvas and marks a host session as changed.
        match &message {
            Message::Maximized(_) | Message::WindowMoved(_) => {
                return self.update_inner(message);
            }
            Message::WindowResized(size)
                if *size == self.window_size && !self.recenter_on_resize =>
            {
                self.last_window_size = Some(*size);
                return match self.window {
                    Some(id) => window::is_maximized(id).map(Message::Maximized),
                    None => Task::none(),
                };
            }
            Message::OpenStartupFile if STARTUP_FILE.lock().is_ok_and(|file| file.is_none()) => {
                return Task::none();
            }
            _ => {}
        }
        if let Message::ApexSnapshotReady(ready) = message {
            return self.apex_snapshot_completed(ready);
        }
        if matches!(message, Message::ApexPoll) && self.apex_poll_is_read_only() {
            return self.apex_poll();
        }
        if self.apex_session.is_some()
            && !matches!(message, Message::ApexPoll | Message::CanvasHover(..))
        {
            self.apex_check_pending = true;
            self.apex_changed_at = std::time::Instant::now();
        }
        // A pending dock click lives only until the next message: the tap's
        // release and the item's release come from one event.
        if !matches!(
            message,
            Message::DockDragRelease | Message::DockItemRelease(_)
        ) {
            self.dock_click = None;
        }
        // Hover changes the live pointer/HUD, never document geometry. Keep
        // layer order, calculations and the cached floorplan from the last edit.
        if self.pointer_scene_ready {
            if let Message::CanvasHover(x, y) = &message {
                let world = self.world(*x, *y);
                self.workflow_hover(world);
                self.drawing.hover(world, self.draw_ctx());
                self.sync_status_scroll();
                return Task::none();
            }
        }
        self.invalidate_canvas();
        let task = self.update_inner(message);
        // Settle new/deleted areas before the next hover takes the fast path.
        self.ensure_area_layers();
        self.recalculate_if_needed();
        self.workflow_refresh();
        self.sync_status_scroll();
        // The view's render + right-press gates read this (a frame-fresh
        // isEntityVisible/isEntityLocked).
        self.gates_cache = self.layer_gates();
        self.pointer_scene_ready = true;
        self.refresh_dirty_if_due();
        task
    }

    /// The title bar pill: where the document stands against its file.
    fn save_state(&self) -> &'static str {
        let sketchy_source = self
            .current_file
            .as_ref()
            .is_some_and(|f| f.format == SketchFileFormat::SketchyJson);
        if self.dirty || sketchy_source {
            "Unsaved"
        } else if self.current_file.is_some() {
            "Saved"
        } else {
            "New document"
        }
    }

    /// Every in-app way out of the program.
    fn exit(&mut self) -> Task<Message> {
        self.store_placement();
        iced::exit()
    }

    /// Writes the window placement if it changed since it was last stored.
    fn store_placement(&mut self) {
        if let Some(placement) = self.placement {
            if self.placement_stored != Some(placement) {
                window_state::save(&placement);
                self.placement_stored = Some(placement);
            }
        }
    }

    /// Open a sketch the user picked or dropped — unless another window
    /// already has that file, in which case that window comes forward:
    /// two windows on one file would overwrite each other's saves.
    fn open_requested(&mut self, path: std::path::PathBuf) -> Task<Message> {
        let target = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
        let mine = self
            .current_file
            .as_ref()
            .is_some_and(|file| file.path == target);
        if !mine && platform::activate_existing(&target) {
            return self
                .toasts
                .show("That sketch is already open in another window.".into());
        }
        self.open_sketch_path(path)
    }

    fn invalidate_canvas(&mut self) {
        self.canvas_cache.clear();
        self.pointer_scene_ready = false;
    }

    /// The status bar is `overflow-x: auto`: once its centre overflows, the
    /// readout wraps (30px tall), the bar overflows vertically and a 10px
    /// scrollbar appears — which keeps the content overflowing until it fits
    /// the narrower bar again (Chrome's layout, measured on the original).
    fn sync_status_scroll(&mut self) {
        let natural = statusbar::center_natural(&self.view, &self.drawing.hud);
        let max = statusbar::center_max(&self.pages, self.window_size.width);
        if self.status_scroll {
            if natural <= max - statusbar::SCROLLBAR_W {
                self.status_scroll = false;
            }
        } else if natural > max {
            self.status_scroll = true;
        }
    }

    /// The space a window may occupy on its monitor, in CSS px (the app's
    /// layout unit). On Windows this is the work area — the monitor minus
    /// the taskbar and other docked bars; elsewhere the whole monitor.
    fn available_size(&self, id: window::Id) -> Task<Option<Size>> {
        #[cfg(windows)]
        {
            let ui_scale = self.ui_scale;
            window::run(id, move |window| {
                platform::work_area(window)
                    .map(|(w, h)| Size::new(w / ui_scale, h / ui_scale))
            })
        }
        #[cfg(not(windows))]
        {
            window::monitor_size(id)
        }
    }

    fn update_inner(&mut self, message: Message) -> Task<Message> {
        self.sync_canvas();
        // The panel/page-sync lazy layer sync (`getAreaLayersForCurrentPage`
        // runs assignAreaToDedicatedLayer per drifted polygon). Idempotent
        // and undo-free, as in the original.
        self.ensure_area_layers();
        if std::env::var_os("SKETCH_TRACE").is_some() && !matches!(message, Message::Noop) {
            eprintln!("TRACE msg={message:?} file_menu={}", self.file_menu);
        }
        match message {
            Message::Noop => Task::none(),
            Message::WindowReady(id) => {
                self.window = id;
                match id {
                    // The real size first (a restored or maximized window
                    // is not the default size), then the startup file, so
                    // it is fitted to the canvas it will be shown in.
                    Some(id) => Task::batch([
                        self.available_size(id).map(Message::MonitorSize),
                        window::size(id)
                            .map(Message::WindowResized)
                            .chain(Task::done(Message::OpenStartupFile)),
                    ]),
                    None => Task::none(),
                }
            }
            Message::MonitorSize(size) => {
                // A 1400x860 CSS px window that does not fit the monitor's
                // work area (the monitor minus the taskbar; e.g. 250%
                // scaling) opens maximized instead of overflowing it — the
                // browser original would simply fill the screen. `size` is
                // already in CSS px. Maximizing an undecorated window keeps
                // it inside the work area, so the taskbar stays visible.
                // A remembered placement was already checked against the
                // monitors when it was loaded.
                match (self.window, size) {
                    (Some(id), Some(m))
                        if self.placement_stored.is_none()
                            && (WINDOW_W > m.width || WINDOW_H > m.height) =>
                    {
                        self.recenter_on_resize = true;
                        window::maximize(id, true)
                    }
                    _ => Task::none(),
                }
            }
            Message::TitleDrag => match self.window {
                Some(id) => window::drag(id),
                None => Task::none(),
            },
            Message::Minimize => match self.window {
                Some(id) => window::minimize(id, true),
                None => Task::none(),
            },
            // Maximize / restore, not exclusive fullscreen: a maximized
            // window stops at the work area, so the taskbar stays visible.
            Message::Maximized(maximized) => {
                self.maximized = maximized;
                // The restored bounds are only what the window reports
                // while it is NOT maximized; maximizing keeps the old ones.
                let mut placement = self.placement.unwrap_or(window_state::Placement {
                    pos: None,
                    width: WINDOW_W,
                    height: WINDOW_H,
                    maximized,
                });
                placement.maximized = maximized;
                if !maximized {
                    if let Some(p) = self.last_window_pos {
                        placement.pos = Some((p.x, p.y));
                    }
                    if let Some(s) = self.last_window_size {
                        placement.width = s.width;
                        placement.height = s.height;
                    }
                }
                self.placement = Some(placement);
                Task::none()
            }
            Message::ResizeDrag(direction) => match self.window {
                Some(id) => window::drag_resize(id, direction),
                None => Task::none(),
            },
            Message::WindowMoved(position) => {
                // A minimized window reports (-32000, -32000).
                if position.x <= -30000.0 || position.y <= -30000.0 {
                    return Task::none();
                }
                self.last_window_pos = Some(position);
                match self.window {
                    Some(id) => window::is_maximized(id).map(Message::Maximized),
                    None => Task::none(),
                }
            }
            Message::FileDropped(path) => {
                if !is_sketch_file(&path) {
                    return self.toasts.show(format!(
                        "Only sketch files (.{}) can be opened by dropping them here.",
                        SKETCH_OPEN_EXTENSIONS.join(", .")
                    ));
                }
                self.refresh_dirty();
                if !self.dirty {
                    return self.open_requested(path);
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.open_dialog(dialog::Dialog::confirm(
                    &format!("Open \"{name}\"? Unsaved changes will be lost."),
                    dialog::Action::OpenDropped(path),
                ))
            }
            Message::OpenStartupFile => {
                let path = STARTUP_FILE.lock().ok().and_then(|mut file| file.take());
                match path {
                    Some(path) => self.open_sketch_path(path),
                    None => Task::none(),
                }
            }
            Message::ActivateWindow => match self.window {
                Some(id) => window::minimize(id, false).chain(window::gain_focus(id)),
                None => Task::none(),
            },
            Message::Pinch {
                factor,
                x,
                y,
                dx,
                dy,
            } => {
                self.view.pan.0 += dx as f64;
                self.view.pan.1 += dy as f64;
                if factor.is_finite() && factor > 0.0 {
                    self.view
                        .zoom_to(self.view.zoom * factor as f64, x as f64, y as f64);
                }
                Task::none()
            }
            Message::ToggleMaximize => match self.window {
                Some(id) => window::toggle_maximize(id),
                None => Task::none(),
            },
            // `exitApp` (S11-080): the confirm first ("Close this sketch?
            // Save your work first if needed."), then the close.
            Message::ApexPoll => self.apex_poll(),
            Message::ApexSnapshotReady(ready) => self.apex_snapshot_completed(ready),
            Message::IntegrationSetup(msg) => self.integration_setup.update(msg),
            Message::ApexReturn => self.return_to_host(),
            Message::ApexSaved(result) => {
                self.apex_returning = false;
                match result {
                    Ok(()) => self
                        .toasts
                        .show("Sketch and measurements saved back to the host.".into()),
                    Err(error) => self.toasts.show(error),
                }
            }
            Message::ApexReturned(result) => {
                self.apex_returning = false;
                match result {
                    Ok(()) => self.exit(),
                    Err(error) => self.toasts.show(error),
                }
            }
            Message::Close if self.integration_setup.busy => self
                .toasts
                .show("Finish integration setup before closing Sketch.".into()),
            Message::Close if self.apex_returning => self
                .toasts
                .show("Wait for the report transfer to finish before closing Sketch.".into()),
            Message::Close => {
                if self.apex_session.is_some() {
                    return self.open_dialog(dialog::Dialog::report_close());
                }
                // Nothing to lose: close without asking. Otherwise offer
                // Save / Don't save / Cancel.
                self.refresh_dirty();
                if !self.dirty {
                    return self.exit();
                }
                self.open_dialog(dialog::Dialog::save_before_close(&self.file_name))
            }
            Message::CloseHover(h) => {
                self.close_hover = h;
                Task::none()
            }
            Message::TabClicked(id) => {
                self.click_tab(id);
                Task::none()
            }
            Message::PointerDown(p) => {
                // ui-manager.js: a pointerdown outside `#file-menu` and the
                // File tab closes the menu (the press still reaches its target).
                let on_file_tab = p.x < ribbon::FILE_TAB_W
                    && p.y >= ribbon::TAB_Y
                    && p.y < ribbon::TAB_Y + ribbon::TAB_H;
                if !backstage::contains(p.x, p.y) && !on_file_tab {
                    self.close_file_menu();
                }
                Task::none()
            }
            Message::Escape => {
                if self.apex_geo_pending.take().is_some() {
                    return self
                        .toasts
                        .show("Reference-point selection cancelled.".into());
                }
                if self.symbol_manager.open {
                    if self.symbol_manager.enlarged {
                        self.symbol_manager.enlarged = false;
                    } else {
                        self.symbol_manager.open = false;
                    }
                    return Task::none();
                }
                if self.autosave_prompt {
                    self.autosave_prompt = false;
                    return Task::none();
                }
                self.close_file_menu();
                Task::none()
            }
            Message::Dock(m) => {
                // The two library managers open App-level dialogs.
                match m {
                    dock::DockMsg::EditTextLibrary => {
                        self.text_manager.open();
                        return iced::widget::operation::focus(text_manager::INPUT_ID);
                    }
                    dock::DockMsg::OrganizeSymbols => {
                        return self.manager_msg(symbol_manager::Msg::Open);
                    }
                    _ => {}
                }
                let mut task = match self.dock.update(m) {
                    dock::Effect::None => Task::none(),
                    dock::Effect::Toast(text) => self.toasts.show(text),
                };
                // SymbolPick arms placement mode via the dock's preview.
                if let Some(id) = self.dock.preview.clone() {
                    self.placing_symbol = Some(id);
                    task = task.chain(self.set_mode(view_state::Mode::PlaceSymbol));
                }
                // TextPick/TextSubmit arms text placement. The dock keeps the
                // pick `.active` (parity — it's cleared only by a search or
                // range change), so arming reads it without consuming it.
                if let Some(label) = self.dock.text_active.clone() {
                    self.placing_text = Some(label);
                    task = task.chain(self.set_mode(view_state::Mode::PlaceText));
                }
                task
            }
            Message::PointerMoved(p, viewport_w) => {
                self.dock.drag_to(p.x, viewport_w);
                Task::none()
            }
            Message::PointerUp => {
                self.dock.end_drag();
                Task::none()
            }
            Message::DockDragPress(item) => {
                self.dock_drag = Some(DockDrag {
                    item,
                    origin: None,
                    active: false,
                    cursor: None,
                    hover: None,
                });
                self.dock_click = None;
                Task::none()
            }
            Message::DockItemRelease(item) => {
                // The release over a dock item: a click when its press began
                // here and travelled nowhere (a drag's release is not one).
                let pending = self.dock_click.take().or_else(|| {
                    // The release came before the tap was listening.
                    self.dock_drag.take().filter(|d| !d.active).map(|d| d.item)
                });
                if pending.as_ref() != Some(&item) {
                    return Task::none();
                }
                match item {
                    dock::DragItem::Symbol(id) => {
                        self.update_inner(Message::Dock(dock::DockMsg::SymbolPick(id)))
                    }
                    dock::DragItem::Text(label) => {
                        self.update_inner(Message::Dock(dock::DockMsg::TextPick(label)))
                    }
                    // A click on a tile picks the code (`selectedAreaType`).
                    dock::DragItem::Area { code, .. } => {
                        self.workflow_msg(area_workflow::Msg::Arm(code))
                    }
                }
            }
            Message::DockDragMove(p, _) => {
                // `dragstart` fires once the pointer has travelled a few px.
                if let Some(drag) = &mut self.dock_drag {
                    let origin = *drag.origin.get_or_insert(p);
                    if (p.x - origin.x).hypot(p.y - origin.y) > DRAG_START_PX {
                        drag.active = true;
                    }
                    drag.cursor = Some(p);
                }
                self.update_area_drop_hint();
                Task::none()
            }
            Message::DockDragRelease => {
                // `drop` on the canvas: a symbol goes through the regular
                // placement (wall snap, swing, openings); a text becomes a
                // plain 12px label.
                let Some(drag) = self.dock_drag.take() else {
                    return Task::none();
                };
                if !drag.active {
                    self.dock_click = Some(drag.item);
                    return Task::none();
                }
                let Some((x, y)) = drag.cursor.and_then(|p| self.canvas_point(p)) else {
                    return Task::none();
                };
                let world = self.world(x, y);
                match drag.item {
                    dock::DragItem::Area { code, .. } => return self.drop_area_code(world, &code),
                    dock::DragItem::Symbol(symbol_type) => self.place_symbol_at(world, symbol_type),
                    dock::DragItem::Text(text) => {
                        self.push_undo("Place Text");
                        let id = placed_text::place_text_at(&mut self.drawing, world, text);
                        if let Some(label) = self.drawing.texts.iter_mut().find(|t| t.id == id) {
                            label.font_size = 12.0;
                            label.font_family = "Arial".into();
                            label.bold = false;
                            label.italic = false;
                            label.underline = false;
                            label.color = String::new();
                        }
                        self.sync_canvas();
                    }
                }
                Task::none()
            }
            Message::ToastExpired(id) => {
                self.toasts.expire(id);
                Task::none()
            }
            Message::WindowResized(size) => {
                // A minimized window can report an empty size.
                if size.width < 1.0 || size.height < 1.0 {
                    return Task::none();
                }
                self.workflow_close();
                // iced reports the size in CSS px already: it divides the
                // physical size by the OS scale times the app's own.
                self.window_size = size;
                self.last_window_size = Some(size);
                self.sync_canvas();
                if self.recenter_on_resize {
                    let (cw, ch) = self.view.canvas;
                    self.view.pan = (cw / 2.0, ch / 2.0);
                    self.recenter_on_resize = false;
                }
                // Maximize, restore, snap and a title-bar drag out of the
                // maximized state all resize the window, so this keeps the
                // title bar's maximize/restore icon in step with all of them.
                match self.window {
                    Some(id) => window::is_maximized(id).map(Message::Maximized),
                    None => Task::none(),
                }
            }
            Message::Status(m) => self.status(m),
            Message::FenceStart(x, y) => {
                self.view.fence_start(x as f64, y as f64);
                Task::none()
            }
            Message::FenceMove(x, y) => {
                self.view.fence_move(x as f64, y as f64);
                Task::none()
            }
            Message::FenceEnd => {
                // `fenceSelect` / `lassoSelect` / the zoom-fence.
                let additive = false;
                // Hidden/locked layers refuse the pick (the original's
                // `selectable` gate).
                let gates = self.layer_gates();
                let selectable = |id: i64| gates.selectable(id);
                match self.view.fence_end() {
                    view_state::FenceOutcome::None | view_state::FenceOutcome::Zoom => {}
                    view_state::FenceOutcome::Select { min, max } => {
                        let (lo, hi) = (
                            sketch_core::geometry::Pt::new(min.x, min.y),
                            sketch_core::geometry::Pt::new(max.x, max.y),
                        );
                        selection::fence_select(
                            &mut self.selection,
                            &self.drawing.lines,
                            &self.drawing.polygons,
                            lo,
                            hi,
                            additive,
                            &selectable,
                        );
                        let found = {
                            let pick = self.pickable();
                            let mut found = selection::Selection::default();
                            selection::pick_by_position(
                                &mut found,
                                &pick,
                                &|p| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y,
                                &selectable,
                            );
                            found
                        };
                        self.selection.absorb(found);
                        self.sync_edit_tools();
                    }
                    view_state::FenceOutcome::Lasso(points) => {
                        let pts: Vec<sketch_core::geometry::Pt> = points
                            .into_iter()
                            .map(|p| sketch_core::geometry::Pt::new(p.x, p.y))
                            .collect();
                        selection::lasso_select(
                            &mut self.selection,
                            &self.drawing.lines,
                            &self.drawing.polygons,
                            &pts,
                            additive,
                            &selectable,
                        );
                        if pts.len() >= 3 {
                            let geo = sketch_core::geometry::GeometryEngine::new();
                            let found = {
                                let pick = self.pickable();
                                let mut found = selection::Selection::default();
                                selection::pick_by_position(
                                    &mut found,
                                    &pick,
                                    &|p| geo.point_in_polygon(p, &pts),
                                    &selectable,
                                );
                                found
                            };
                            self.selection.absorb(found);
                        }
                        self.sync_edit_tools();
                    }
                }
                Task::none()
            }
            Message::CanvasPress(x, y) => {
                let world = self.world(x, y);
                if let Some((page, points)) = self.apex_geo_pending.as_mut() {
                    points.push(world);
                    if points.len() == 2 {
                        self.pages.pages[*page].geo_reference_points = points
                            .iter()
                            .map(|p| pages::GeoPoint {
                                x: p.x,
                                y: p.y,
                                coordinates: String::new(),
                            })
                            .collect();
                        self.apex_geo_pending = None;
                        if let Some(root) = self.apex_session.clone() {
                            if let Err(error) = self.write_apex_snapshot(&root, false) {
                                return self.toasts.show(error);
                            }
                        }
                        return self.toasts.show("Reference points selected.".into());
                    }
                    return self.toasts.show("Click the second reference point.".into());
                }
                // Image Edit Mode's press starts the photo drag
                // (canvas-engine.js:846: `dragState = {type:'photo'}`).
                if self.bg_image.edit_mode {
                    self.photo_drag = Some(world);
                    return Task::none();
                }
                // A rotation handle (canvas-engine.js:922): a selected text
                // label / area block, or the selected areas' own handle.
                if self.view.mode == view_state::Mode::Select {
                    let screen = {
                        let k = sketch_core::geometry::PIXELS_PER_FOOT * self.view.zoom;
                        (self.view.pan.0 + world.x * k, self.view.pan.1 - world.y * k)
                    };
                    let hit = handles::hit_handle(
                        &self.drawing,
                        &self.selection,
                        &self.computed,
                        self.view.pan,
                        self.view.zoom,
                        screen,
                    );
                    match hit {
                        Some(handles::HandleHit::Area { center_world }) => {
                            let center = view_state::World {
                                x: center_world.x,
                                y: center_world.y,
                            };
                            self.view.drag = Some(view_state::Drag::RotateArea {
                                center,
                                start_angle: (world.y - center.y).atan2(world.x - center.x),
                                preview: 0.0,
                            });
                            self.drag_banked = false;
                            return Task::none();
                        }
                        Some(handles::HandleHit::Text {
                            kind,
                            id,
                            center_world,
                            start_rotation,
                        }) => {
                            let center = view_state::World {
                                x: center_world.x,
                                y: center_world.y,
                            };
                            self.view.drag = Some(view_state::Drag::RotateText {
                                kind,
                                id,
                                center,
                                start_angle: (world.y - center.y).atan2(world.x - center.x),
                                start_rotation,
                                preview: 0.0,
                            });
                            self.drag_banked = false;
                            return Task::none();
                        }
                        None => {}
                    }
                    // Pressing an area's label / size text or a free text label
                    // picks it (unless it is already part of the selection) and
                    // starts moving the selection (`hitTestAt`'s first passes,
                    // then the selection drag).
                    let area_text = self.area_text_at(world);
                    let text = if area_text.is_some() {
                        None
                    } else {
                        self.drawing
                            .texts
                            .iter()
                            .find(|t| placed_text::point_in_text(t, world, self.view.zoom))
                            .map(|t| t.id)
                    };
                    if area_text.is_some() || text.is_some() {
                        let already = match (area_text, text) {
                            (Some((handles::TextKind::AreaSize, id)), _) => {
                                self.selection.area_sizes.contains(&id)
                            }
                            (Some((_, id)), _) => self.selection.area_labels.contains(&id),
                            (_, Some(id)) => self.selection.texts.contains(&id),
                            _ => false,
                        };
                        if !already {
                            self.selection.clear();
                            match (area_text, text) {
                                (Some((handles::TextKind::AreaSize, id)), _) => {
                                    self.selection.area_sizes.insert(id);
                                }
                                (Some((_, id)), _) => {
                                    self.selection.area_labels.insert(id);
                                }
                                (_, Some(id)) => {
                                    self.selection.texts.insert(id);
                                }
                                _ => {}
                            }
                            self.sync_edit_tools();
                        }
                        let (bx, by) = self.selection_bounds();
                        self.view.drag_start_move(
                            view_state::World {
                                x: world.x,
                                y: world.y,
                            },
                            view_state::World { x: bx, y: by },
                        );
                        return Task::none();
                    }
                }
                // Curve Wall tool: pressing a wall selects it and starts
                // bowing it (canvas-engine.js:854); a press elsewhere only
                // selects.
                if self.view.mode == view_state::Mode::Curve {
                    let gates = self.layer_gates();
                    let selectable = |id: i64| gates.selectable(id);
                    let hit = selection::hit_test_at(
                        world,
                        &self.drawing.lines,
                        &self.drawing.polygons,
                        self.view.zoom,
                        &selectable,
                    );
                    selection::select_at(
                        &mut self.selection,
                        world,
                        &self.drawing.lines,
                        &self.drawing.polygons,
                        self.view.zoom,
                        false,
                        &selectable,
                    );
                    self.sync_edit_tools();
                    if let Some(selection::Hit::Line(id)) = hit {
                        self.start_curve_drag(id, world);
                    }
                    return Task::none();
                }
                // The green curve handle of a selected wall bows it
                // (canvas-engine.js:958).
                let handle_tolerance = drawing::Drawing::tolerance(9.0, self.view.zoom);
                if let Some(id) = self
                    .drawing
                    .lines
                    .iter()
                    .filter(|l| self.selection.lines.contains(&l.id))
                    .find(|l| {
                        sketch_core::geometry::distance(world, drawing::curve_handle(l))
                            <= handle_tolerance
                    })
                    .map(|l| l.id)
                {
                    self.start_curve_drag(id, world);
                    return Task::none();
                }
                // Handle hit-test takes priority for resize/rotate (P4).
                if let Some((id, is_rotate)) = self.symbol_handle_at(world) {
                    let sym = self.drawing.symbols.iter().find(|s| s.id == id).cloned();
                    if let Some(sym) = sym {
                        if is_rotate {
                            let center = view_state::World { x: sym.x, y: sym.y };
                            let start_angle = (world.y - sym.y).atan2(world.x - sym.x);
                            self.view.drag = Some(view_state::Drag::SymbolRotate {
                                id,
                                start_rotation: sym.rotation,
                                start_angle,
                                center,
                                preview: 0.0,
                            });
                        } else {
                            let center = view_state::World { x: sym.x, y: sym.y };
                            let start_dist = ((world.x - sym.x).powi(2)
                                + (world.y - sym.y).powi(2))
                            .sqrt()
                            .max(1e-6);
                            self.view.drag = Some(view_state::Drag::SymbolResize {
                                id,
                                start_scale: sym.scale,
                                start_dist,
                                center,
                            });
                        }
                        self.drag_banked = false;
                        return Task::none();
                    }
                }

                // Symbol hit-test takes priority for drag (P4).
                let mut symbol_hit: Option<i64> = None;
                for sym in &self.drawing.symbols {
                    let bounds = self.symbols_lib().symbol(&sym.symbol_type).map(|d| {
                        (
                            d.bounds.min_x,
                            d.bounds.min_y,
                            d.bounds.max_x,
                            d.bounds.max_y,
                        )
                    });
                    if placed_symbol::point_in_symbol(sym, bounds, world, self.view.zoom) {
                        symbol_hit = Some(sym.id);
                        break;
                    }
                }
                if let Some(id) = symbol_hit {
                    if !self.selection.symbols.contains(&id) {
                        self.selection.clear();
                        self.selection.symbols.insert(id);
                        self.sync_edit_tools();
                    }
                    let (bx, by) = self.selection_bounds();
                    let w = view_state::World {
                        x: world.x,
                        y: world.y,
                    };
                    self.view
                        .drag_start_move(w, view_state::World { x: bx, y: by });
                    return Task::none();
                }
                let gates = self.layer_gates();
                let selectable = |id: i64| gates.selectable(id);
                // A measurement label (after the on-the-wall pass): picked,
                // and the selection drags - the label pins where it lands.
                if let Some(id) = self.dimension_hit_at(world) {
                    if !self.selection.dimensions.contains(&id) {
                        self.selection.clear();
                        self.selection.dimensions.insert(id);
                        self.sync_edit_tools();
                    }
                    let (bx, by) = self.selection_bounds();
                    self.view.drag_start_move(
                        view_state::World {
                            x: world.x,
                            y: world.y,
                        },
                        view_state::World { x: bx, y: by },
                    );
                    return Task::none();
                }
                let hit = selection::hit_test_at(
                    world,
                    &self.drawing.lines,
                    &self.drawing.polygons,
                    self.view.zoom,
                    &selectable,
                );
                let w = view_state::World {
                    x: world.x,
                    y: world.y,
                };
                match hit {
                    // A press on an endpoint handle of a selected wall drags
                    // that vertex (`type: 'line-endpoint'`).
                    Some(selection::Hit::Line(id)) if self.selection.lines.contains(&id) => {
                        if let Some((start, at)) = self.endpoint_handle(id, world) {
                            self.view.drag_start_endpoint(id, start, at);
                        }
                    }
                    Some(_) => {
                        // Pressing something already selected keeps the whole
                        // group; a fresh hit selects it first.
                        let already = match hit {
                            Some(selection::Hit::Line(id)) => self.selection.lines.contains(&id),
                            Some(selection::Hit::Polygon(id)) => self.selection.polys.contains(&id),
                            None => false,
                        };
                        if !already {
                            selection::select_at(
                                &mut self.selection,
                                world,
                                &self.drawing.lines,
                                &self.drawing.polygons,
                                self.view.zoom,
                                false,
                                &selectable,
                            );
                            self.sync_edit_tools();
                        }
                        let (bx, by) = self.selection_bounds();
                        self.view
                            .drag_start_move(w, view_state::World { x: bx, y: by });
                    }
                    // A press that hits nothing begins the marquee.
                    None => self.view.marquee_start(w),
                }
                Task::none()
            }
            Message::CanvasDrag(x, y, ctrl) => {
                let world = self.world(x, y);
                // The photo drag repositions the image (bgimage-drag:
                // `bgImage.x += current.x - drag.lastWorld.x`).
                if let Some(last) = self.photo_drag.take() {
                    self.bg_image.x += world.x - last.x;
                    self.bg_image.y += world.y - last.y;
                    self.photo_drag = Some(world);
                    return Task::none();
                }
                let w = view_state::World {
                    x: world.x,
                    y: world.y,
                };
                match self.view.drag.clone() {
                    Some(view_state::Drag::Endpoint {
                        line_id,
                        start,
                        start_point,
                        ..
                    }) => {
                        let target = sketch_core::geometry::Pt::new(world.x, world.y);
                        if (target.x - start_point.x).hypot(target.y - start_point.y) > 0.02
                            && transform::move_line_endpoint(
                                &mut self.drawing,
                                &mut self.selection,
                                line_id,
                                start,
                                target,
                            )
                            && !self.drag_banked
                        {
                            self.push_undo("Edit Wall Endpoint");
                            self.drag_banked = true;
                        }
                    }
                    Some(view_state::Drag::ChainOffset {
                        chain_id,
                        run_start,
                        run_normal,
                        ..
                    }) => {
                        if !self.drag_banked {
                            self.push_undo("Move Dimension Chain");
                            self.drag_banked = true;
                        }
                        let signed = (world.x - run_start.x) * run_normal.x
                            + (world.y - run_start.y) * run_normal.y;
                        if let Some(chain) = self
                            .drawing
                            .dimension_chains
                            .iter_mut()
                            .find(|c| c.id == chain_id)
                        {
                            chain.side = if signed >= 0.0 { 1 } else { -1 };
                            chain.offset = signed.abs().max(0.5);
                        }
                    }
                    Some(view_state::Drag::Move { .. }) => {
                        let grid = if self.view.area_snap {
                            self.view.area_snap_size
                        } else {
                            self.view.grid_size
                        };
                        let snap = self.view.area_snap
                            || (self.view.snap
                                && (!self.selection.lines.is_empty()
                                    || !self.selection.polys.is_empty()));
                        if let Some((dx, dy)) = self.view.drag_move(w, grid, snap) {
                            if (dx != 0.0 || dy != 0.0) && !self.drag_banked {
                                self.push_undo("Move");
                                self.drag_banked = true;
                            }
                            self.move_selection(dx, dy);
                        }
                    }
                    Some(view_state::Drag::Curve {
                        line_id,
                        start_arc_height,
                        start_pointer_normal,
                    }) => {
                        let Some(line) = self.drawing.lines.iter().find(|l| l.id == line_id) else {
                            return Task::none();
                        };
                        let (dx, dy) = (line.end.x - line.start.x, line.end.y - line.start.y);
                        let chord = dx.hypot(dy);
                        if chord > 0.01 {
                            let mid = (
                                (line.start.x + line.end.x) / 2.0,
                                (line.start.y + line.end.y) / 2.0,
                            );
                            let normal = (world.x - mid.0) * (-dy / chord)
                                + (world.y - mid.1) * (dx / chord);
                            let height = start_arc_height + normal - start_pointer_normal;
                            let threshold = drawing::Drawing::tolerance(1.0, self.view.zoom);
                            if self.drag_banked || (height - start_arc_height).abs() > threshold {
                                if !self.drag_banked {
                                    self.push_undo("Curve Wall");
                                    self.drag_banked = true;
                                }
                                let bow = 4.0 * (2.0 * height).atan2(chord).to_degrees();
                                self.drawing.set_line_bow(line_id, bow);
                            }
                        }
                    }
                    Some(view_state::Drag::RotateArea {
                        center,
                        start_angle,
                        ..
                    }) => {
                        let current = (world.y - center.y).atan2(world.x - center.x);
                        let preview = handles::area_drag_preview(start_angle, current, ctrl);
                        let moved =
                            self.drag_banked || preview.abs() >= if ctrl { 1.0 } else { 15.0 };
                        self.drag_banked = moved;
                        if let Some(view_state::Drag::RotateArea { preview: p, .. }) =
                            &mut self.view.drag
                        {
                            *p = preview;
                        }
                    }
                    Some(view_state::Drag::RotateText {
                        kind,
                        id,
                        center,
                        start_angle,
                        start_rotation,
                        ..
                    }) => {
                        let current = (world.y - center.y).atan2(world.x - center.x);
                        let next =
                            handles::text_drag_rotation(start_rotation, start_angle, current, ctrl);
                        let step = if ctrl { 1.0 } else { 15.0 };
                        if self.drag_banked || (next - start_rotation).abs() >= step {
                            if !self.drag_banked {
                                self.push_undo("Rotate");
                                self.drag_banked = true;
                            }
                            self.set_text_rotation(kind, id, next);
                        }
                        if let Some(view_state::Drag::RotateText { preview: p, .. }) =
                            &mut self.view.drag
                        {
                            *p = handles::normalize_degrees(next - start_rotation);
                        }
                    }
                    Some(view_state::Drag::SymbolResize {
                        id,
                        start_scale,
                        start_dist,
                        center,
                    }) => {
                        // Steps of 5% (1% with Ctrl); the first one to
                        // change the scale banks the `Scale` undo step.
                        let cur_dist =
                            ((world.x - center.x).powi(2) + (world.y - center.y).powi(2)).sqrt();
                        let new_scale =
                            handles::symbol_drag_scale(start_scale, start_dist, cur_dist, ctrl);
                        let step = if ctrl { 0.01 } else { 0.05 };
                        if self.drag_banked || (new_scale - start_scale).abs() >= step / 2.0 {
                            if !self.drag_banked {
                                self.push_undo("Scale");
                                self.drag_banked = true;
                            }
                            if let Some(sym) = self.drawing.symbols.iter_mut().find(|s| s.id == id)
                            {
                                sym.set_scale(new_scale);
                            }
                        }
                    }
                    Some(view_state::Drag::SymbolRotate {
                        id,
                        start_rotation,
                        start_angle,
                        center,
                        ..
                    }) => {
                        // The start rotation minus the pointer's turn, in 15
                        // degree steps (1 with Ctrl); the first full step banks
                        // the undo and frees the symbol from its wall.
                        let cur_angle = (world.y - center.y).atan2(world.x - center.x);
                        let next = handles::symbol_drag_rotation(
                            start_rotation,
                            start_angle,
                            cur_angle,
                            ctrl,
                        );
                        let step = if ctrl { 1.0 } else { 15.0 };
                        if self.drag_banked || (next - start_rotation).abs() >= step {
                            if !self.drag_banked {
                                self.push_undo("Rotate Symbol");
                                self.drag_banked = true;
                            }
                            if let Some(sym) = self.drawing.symbols.iter_mut().find(|s| s.id == id)
                            {
                                sym.rotation = next;
                                sym.wall_id = None;
                                sym.wall_fraction = None;
                                sym.wall_side_explicit = false;
                            }
                        }
                        if let Some(view_state::Drag::SymbolRotate { preview: p, .. }) =
                            &mut self.view.drag
                        {
                            *p = handles::normalize_degrees(next - start_rotation);
                        }
                    }
                    None => {}
                }
                Task::none()
            }
            Message::CanvasDragEnd => {
                let moved = match &self.view.drag {
                    Some(view_state::Drag::Move { moved, .. }) => *moved,
                    Some(view_state::Drag::Endpoint { moved, .. }) => *moved,
                    Some(view_state::Drag::ChainOffset { moved, .. }) => *moved,
                    Some(view_state::Drag::SymbolResize { .. }) => self.drag_banked,
                    Some(view_state::Drag::SymbolRotate { .. }) => self.drag_banked,
                    // A bowed wall re-derives its areas through the recalc;
                    // it never closes loops.
                    Some(view_state::Drag::Curve { .. }) => false,
                    Some(view_state::Drag::RotateText { .. }) => false,
                    Some(view_state::Drag::RotateArea { .. }) => false,
                    None => false,
                };
                // The area handle lands its rotation on release
                // (`rotateSelected(previewDegrees)`).
                let landing = match &self.view.drag {
                    Some(view_state::Drag::RotateArea { preview, .. })
                        if self.drag_banked && preview.abs() > 0.001 =>
                    {
                        Some(*preview)
                    }
                    _ => None,
                };
                self.view.drag = None;
                self.drag_banked = false;
                if let Some(degrees) = landing {
                    return self.update(Message::RotateSelected(degrees));
                }
                if moved {
                    // Walls dropped into a closed arrangement enclose an area
                    // just as if the last wall had been drawn there.
                    self.drawing.close_loops(self.view.zoom);
                }
                Task::none()
            }
            Message::Dialog(m) => self.dialog_msg(m),
            Message::NewFile => {
                self.drawing = drawing::Drawing::default();
                self.selection.clear();
                self.pages = pages::Pages::default();
                self.file_name = "Untitled".to_string();
                self.current_file = None;
                self.subject_info = subject_info::SubjectInfo::default();
                self.subject_auto_filled = false;
                self.extended_data = String::new();
                self.native_apex = serde_json::Value::Null;
                self.layers = layers::Layers::default();
                // `resetToBlank` leaves the area-code table alone.
                self.settings_extra.clear();
                self.sync_edit_tools();
                self.autosave_baseline();
                Task::none()
            }
            Message::OpenFile => {
                if let Some(path) = file_dialogs::open_sketch() {
                    return self.open_requested(path);
                }
                Task::none()
            }
            Message::SaveFile => {
                if self.apex_session.is_some() {
                    return self.save_to_host();
                }
                self.save_to_file(false)
            }
            Message::SaveFileAs => self.save_to_file(true),
            Message::ExportPng => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG image", &["png"])
                    .set_file_name(format!("{}.png", self.file_name))
                    .save_file()
                {
                    match self.png_export_bytes() {
                        Some(bytes) => {
                            let _ = std::fs::write(&path, bytes);
                        }
                        None => {
                            return self
                                .toasts
                                .show("Export as PNG could not fit the sketch.".to_string())
                        }
                    }
                }
                Task::none()
            }
            Message::ExportSvg => {
                let north = self.pages.page().north_angle;
                let page = exporter::build_export_page(&self.drawing, north, &self.area_codes);
                let options = exporter::export_options(&self.drawing);
                let codes = self.area_codes.clone();
                let library = self
                    .symbols
                    .as_ref()
                    .as_ref()
                    .ok()
                    .map(|index| &index.library);
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("SVG image", &["svg"])
                    .set_file_name(format!("{}.svg", self.file_name))
                    .save_file()
                {
                    let svg = sketch_core::export_svg::export_svg(&page, &options, &codes, library);
                    let _ = std::fs::write(&path, svg);
                }
                Task::none()
            }
            Message::ExportDxf => {
                let north = self.pages.page().north_angle;
                let page = exporter::build_export_page(&self.drawing, north, &self.area_codes);
                let options = exporter::export_options(&self.drawing);
                let codes = self.area_codes.clone();
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("DXF drawing", &["dxf"])
                    .set_file_name(format!("{}.dxf", self.file_name))
                    .save_file()
                {
                    let dxf = sketch_core::export_dxf::export_dxf(&page, &options, &codes);
                    let _ = std::fs::write(&path, dxf);
                }
                Task::none()
            }
            Message::ExportPdf => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PDF document", &["pdf"])
                    .set_file_name(format!("{}.pdf", self.file_name))
                    .save_file()
                {
                    let _ = std::fs::write(&path, self.report_pdf());
                }
                Task::none()
            }
            Message::ImportCad => {
                // `importFromFile` for a DXF: read, scale via the unit
                // choice, offset by (2, 2) like the original, and append the
                // walls / labels / UND areas. IDs regenerate; the stays part
                // of history via the wall commit (the browser pushes an
                // Import undo step — the port re-banks it as a snapshot).
                // The picker shows .dxf and .dwg; a .dwg is rejected with the
                // conversion hint (`runImport`).
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("CAD drawing", &["dxf", "dwg"])
                    .pick_file()
                {
                    return self.import_cad_file(&path);
                }
                Task::none()
            }
            Message::CommandPaletteOpen => {
                // `open()` toggles: a second Ctrl+K / magnifier press closes.
                if self.command_palette_open {
                    self.command_palette_open = false;
                    return Task::none();
                }
                self.command_palette_open = true;
                self.command_palette_query.clear();
                self.command_palette_selected = 0;
                iced::widget::operation::focus(command_palette::INPUT_ID)
            }
            Message::CommandPaletteClose => {
                self.command_palette_open = false;
                Task::none()
            }
            Message::CommandPaletteQuery(q) => {
                // `render(q)` keeps the highlight where it was, clamped to
                // the new matches.
                self.command_palette_query = q;
                let count = command_palette::matches(&self.command_palette_query).len();
                self.command_palette_selected =
                    self.command_palette_selected.min(count.saturating_sub(1));
                Task::none()
            }
            Message::CommandPaletteMove(d) => {
                let count = command_palette::matches(&self.command_palette_query).len();
                self.command_palette_selected =
                    command_palette::step(self.command_palette_selected, count, d);
                Task::none()
            }
            Message::CommandPaletteRunAt(pos) => {
                self.command_palette_selected = pos;
                self.update(Message::CommandPaletteRun)
            }
            Message::RotateSketch => self.update(Message::RotateSketchBy(90.0)),
            Message::RotateSelected(degrees) => {
                // `rotateSelected`: with no wall or area selected it turns
                // the selected text labels and symbols in place; otherwise the
                // selected geometry turns about its centre.
                if self.selection.is_empty() {
                    return Task::none();
                }
                self.push_undo("Rotate");
                if self.selection.lines.is_empty() && self.selection.polys.is_empty() {
                    for sym in &mut self.drawing.symbols {
                        if self.selection.symbols.contains(&sym.id) {
                            sym.rotation += degrees;
                            sym.wall_id = None;
                            sym.wall_fraction = None;
                            sym.wall_side_explicit = false;
                        }
                    }
                    for t in &mut self.drawing.texts {
                        if self.selection.texts.contains(&t.id) {
                            t.rotation += degrees;
                        }
                    }
                    // An area's label and size blocks turn on their own; the
                    // size falls back to the label's rotation.
                    for poly in &mut self.drawing.polygons {
                        let label = poly.label_rotation.unwrap_or(0.0);
                        let size = poly.area_size_rotation.unwrap_or(label);
                        if self.selection.area_labels.contains(&poly.id) {
                            poly.label_rotation = Some(label + degrees);
                        }
                        if self.selection.area_sizes.contains(&poly.id) {
                            poly.area_size_rotation = Some(size + degrees);
                        }
                    }
                } else {
                    transform::rotate_selected(&mut self.drawing, &self.selection, degrees);
                }
                self.sync_canvas();
                Task::none()
            }
            Message::ScaleSelected(factor) => {
                // `scaleSelected`: labels' font size 6-72, symbols' scale
                // 0.25-8; nothing selected of either returns first.
                if self.selection.symbols.is_empty()
                    && self.selection.texts.is_empty()
                    && self.selection.area_labels.is_empty()
                    && self.selection.area_sizes.is_empty()
                {
                    return Task::none();
                }
                self.push_undo("Scale");
                for t in &mut self.drawing.texts {
                    if self.selection.texts.contains(&t.id) {
                        t.font_size = (t.font_size * factor).clamp(6.0, 72.0);
                    }
                }
                // An area's blocks scale 0.5-4; the size falls back to the
                // label's scale (read before the label's changes).
                for poly in &mut self.drawing.polygons {
                    let label_scale = poly.label_scale.filter(|s| *s != 0.0).unwrap_or(1.0);
                    let size_scale = poly
                        .area_size_scale
                        .or(poly.label_scale)
                        .filter(|s| *s != 0.0)
                        .unwrap_or(1.0);
                    if self.selection.area_labels.contains(&poly.id) {
                        poly.label_scale = Some((label_scale * factor).clamp(0.5, 4.0));
                    }
                    if self.selection.area_sizes.contains(&poly.id) {
                        poly.area_size_scale = Some((size_scale * factor).clamp(0.5, 4.0));
                    }
                }
                for sym in &mut self.drawing.symbols {
                    if self.selection.symbols.contains(&sym.id) {
                        sym.set_scale((sym.scale * factor).clamp(0.25, 8.0));
                    }
                }
                self.sync_canvas();
                Task::none()
            }
            Message::RotateSketchBy(degrees) => {
                // `rotateSketch(degrees)`: nothing drawn returns early;
                // otherwise the undo step, the whole sketch about its centre
                // with north turning along, and the selection clears.
                if self.drawing.lines.is_empty() && self.drawing.polygons.is_empty() {
                    return Task::none();
                }
                self.push_undo("Rotate Sketch");
                let mut north = self.pages.page().north_angle;
                transform::rotate_sketch(&mut self.drawing, degrees, &mut north);
                let current = self.pages.current;
                self.pages.pages[current].north_angle = north;
                self.selection.clear();
                self.sync_edit_tools();
                Task::none()
            }
            Message::TouchPanelToggle => {
                self.touch_panel_open = !self.touch_panel_open;
                Task::none()
            }
            Message::TouchDistance(s) => {
                if let Ok(v) = s.parse::<f64>() {
                    self.touch_distance = v.max(0.1);
                }
                Task::none()
            }
            Message::TouchAction(action) => {
                let d = self.touch_distance;
                let (mx, my) = match action.as_str() {
                    "up" => (0.0, 1.0),
                    "down" => (0.0, -1.0),
                    "left" => (-1.0, 0.0),
                    "right" => (1.0, 0.0),
                    _ => (0.0, 0.0),
                };
                if matches!(action.as_str(), "up" | "down" | "left" | "right") {
                    if self.view.mode != view_state::Mode::DrawExterior
                        && self.view.mode != view_state::Mode::DrawInterior
                    {
                        let _ = self.set_mode(view_state::Mode::DrawExterior);
                    }
                    let world = self.drawing.cursor;
                    if !self.drawing.pen_down {
                        self.drawing.pen_down_at(world, self.draw_ctx());
                    }
                    let new_pos = sketch_core::geometry::Pt {
                        x: self.drawing.cursor.x + mx * d,
                        y: self.drawing.cursor.y + my * d,
                    };
                    self.push_undo("Draw Wall");
                    let previous = self.drawing.lines.last().map(|line| line.id);
                    self.drawing.click(new_pos, self.draw_ctx());
                    self.finish_wall_commit(previous);
                } else if action == "delete" {
                    let _ = self.update(Message::Delete);
                } else if action == "insert" {
                    let _ = self.set_mode(view_state::Mode::Select);
                }
                self.sync_canvas();
                Task::none()
            }
            Message::RoomShapeOpen => {
                self.room_shapes.open();
                Task::none()
            }
            Message::RoomShapeField(field, val) => {
                match field.as_str() {
                    "kind" => self.room_shapes.kind = val,
                    "width" => self.room_shapes.width = val,
                    "depth" => self.room_shapes.depth = val,
                    "cutw" => self.room_shapes.cut_width = val,
                    "cutd" => self.room_shapes.cut_depth = val,
                    "rotation" => self.room_shapes.rotation = val,
                    _ => {}
                }
                Task::none()
            }
            Message::RoomShapePlace => {
                let rs = &self.room_shapes;
                let pending = room_shapes::PendingShape {
                    kind: if rs.kind.is_empty() {
                        "rectangle".into()
                    } else {
                        rs.kind.clone()
                    },
                    width: room_shapes::parse_len(&rs.width),
                    depth: room_shapes::parse_len(&rs.depth),
                    cut_width: room_shapes::parse_len(&rs.cut_width),
                    cut_depth: room_shapes::parse_len(&rs.cut_depth),
                    rotation: room_shapes::parse_len(&rs.rotation),
                    define_as_area: rs.define_as_area,
                };
                self.pending_shape = Some(pending);
                self.room_shapes.close();
                let _ = self.set_mode(view_state::Mode::PlaceShape);
                Task::none()
            }
            Message::RoomShapeCancel => {
                self.room_shapes.close();
                Task::none()
            }
            Message::OpenSubjectInfo => self.subject_msg(subject_info::SubjectMsg::Open),
            Message::SubjectInfo(m) => self.subject_msg(m),
            Message::LayerPress(id) => {
                // dragstart: arm the row; a fresh drag clears stale markers
                // and closes the context menu (the original's document click).
                self.dock.layer_drag = Some(id);
                self.dock.layer_over = None;
                self.dock.layer_menu = None;
                Task::none()
            }
            Message::LayerZoneHover(id, pos) => {
                // dragover: mark the hovered drop row (never the drag source).
                let armed = self.dock.layer_drag.clone();
                if armed.as_deref() != Some(id.as_str()) {
                    self.dock.layer_over = Some((id, pos));
                }
                Task::none()
            }
            Message::LayerHoverRow(id) => {
                self.dock.layer_hover = Some(id);
                Task::none()
            }
            Message::LayerHoverRowGone(_) => {
                self.dock.layer_hover = None;
                Task::none()
            }
            Message::LayerDrop(row, pos) => {
                match self.dock.layer_drag.take() {
                    Some(drag) => self.commit_layer_drop(drag, Some((row, pos))),
                    // A release with no armed press is a plain row click.
                    None => {
                        self.layers.set_active(row);
                        Task::none()
                    }
                }
            }
            Message::LayerReleaseAnywhere => {
                // The release the widget tree never saw: the pointer left the
                // dock mid-drag. Commit against the last hovered zone, or
                // abort. No-op when a widget release already committed.
                if let Some(drag) = self.dock.layer_drag.take() {
                    let over = self.dock.layer_over.take();
                    if over.is_some() {
                        return self.commit_layer_drop(drag, over);
                    }
                }
                Task::none()
            }
            Message::LayerToggleVisible(id) => {
                let v = self.layers.layer(&id).map(|l| !l.visible).unwrap_or(true);
                // setLayerVisible: the toggle banks 'Layer Visibility' and
                // clears the selection of the hidden branch.
                if self
                    .layers
                    .layer(&id)
                    .map(|l| l.visible == v)
                    .unwrap_or(true)
                {
                    return Task::none();
                }
                self.push_undo("Layer Visibility");
                self.layers.set_visible(&id, v);
                if !v {
                    self.clear_branch_selection(&id);
                }
                self.dock.layer_menu = None;
                Task::none()
            }
            Message::LayerToggleLocked(id) => {
                let v = self.layers.layer(&id).map(|l| !l.locked).unwrap_or(true);
                // setLayerLocked: 'Layer Lock', and locking clears the
                // selection of the branch it protects.
                if self
                    .layers
                    .layer(&id)
                    .map(|l| l.locked == v)
                    .unwrap_or(true)
                {
                    return Task::none();
                }
                self.push_undo("Layer Lock");
                self.layers.set_locked(&id, v);
                if v {
                    self.clear_branch_selection(&id);
                }
                self.dock.layer_menu = None;
                Task::none()
            }
            Message::LayerToggleCollapsed(id) => {
                let v = self.layers.layer(&id).map(|l| l.collapsed).unwrap_or(false);
                self.layers.set_collapsed(&id, !v);
                Task::none()
            }
            Message::LayerSetActive(id) => {
                self.layers.set_active(id);
                Task::none()
            }
            Message::LayerStartRename(id) => {
                if let Some(l) = self.layers.layer(&id) {
                    let name = l.name.clone();
                    self.dock.layer_menu = None;
                    self.dock.layer_rename = Some((id, name));
                    return Task::batch([
                        iced::widget::operation::focus(dock::RENAME_ID),
                        iced::widget::operation::select_all(dock::RENAME_ID),
                    ]);
                }
                Task::none()
            }
            Message::LayerRename(id, value) => {
                if let Some((target, buffer)) = &mut self.dock.layer_rename {
                    if *target == id {
                        *buffer = value;
                    }
                }
                Task::none()
            }
            Message::LayerRenameCommit => {
                if let Some((id, buffer)) = self.dock.layer_rename.take() {
                    // `renameLayer`: a blank name is refused.
                    let clean = buffer.trim().to_string();
                    if !clean.is_empty() {
                        self.layers.rename(&id, clean);
                    }
                }
                Task::none()
            }
            Message::LayerRenameCancel => {
                self.dock.layer_rename = None;
                Task::none()
            }
            Message::LayerRowMenu(id) => {
                // The tree's contextmenu handler activates the row first.
                self.layers.set_active(id.clone());
                self.dock.layer_menu = Some(id);
                Task::none()
            }
            Message::LayerMenuClose => {
                self.dock.layer_menu = None;
                Task::none()
            }
            Message::LayerExport(id) => self.export_layer(&id),
            Message::LayerSendToBack => self.auto_assign_layer("Send to Back", true),
            Message::LayerBringToFront => self.auto_assign_layer("Bring to Front", false),
            Message::TextLibraryManager(m) => {
                if matches!(m, text_manager::TextManagerMsg::Close) {
                    self.text_manager.close();
                    return Task::none();
                }
                if matches!(m, text_manager::TextManagerMsg::Open) {
                    self.text_manager.open();
                    return iced::widget::operation::focus(text_manager::INPUT_ID);
                }
                let toast = self.text_manager.update(m, &mut self.dock.text_labels);
                self.persist_library();
                match toast {
                    Some(text) => self.toasts.show(text),
                    None => Task::none(),
                }
            }
            Message::SymbolOrganizer(m) => self.symbol_organizer_msg(m),
            Message::SymbolManager(m) => self.manager_msg(m),
            Message::ImportLayer => self.import_layer_file(),
            Message::AutoSaveTick => {
                self.store_placement();
                self.autosave_tick(false)
            }
            Message::RecoveryOpen => {
                self.close_file_menu();
                self.autosave.recovery = autosave::latest();
                self.autosave_prompt = self.autosave.recovery.is_some();
                if !self.autosave_prompt {
                    return self.toasts.show("No autosaved sketch is available.".into());
                }
                Task::none()
            }
            Message::RecoverAutosave(accept) => self.recover_autosave(accept),
            Message::RecentToggle => {
                self.recent_open = !self.recent_open;
                Task::none()
            }
            Message::RecentOpen(index) => {
                let Some(entry) = self.recent.get(index).cloned() else {
                    return Task::none();
                };
                if let Some(path) = entry.path.as_ref().filter(|p| p.is_file()) {
                    return self.open_sketch_path(path.clone());
                }
                // `openRecentFile` → loadFromFile → recordRecentFile.
                let Ok(model) = serde_json::from_str::<serde_json::Value>(&entry.data) else {
                    return self
                        .toasts
                        .show("Could not open that recent file.".to_string());
                };
                if !model.get("pages").and_then(|v| v.as_array()).is_some() {
                    return self
                        .toasts
                        .show("Could not open that recent file.".to_string());
                }
                let name = entry.name.trim_end_matches(".skx").to_string();
                let task = self.apply_model(&model, &name);
                self.record_recent(&entry.name, &entry.data);
                task
            }
            Message::RecentClear => {
                self.recent.clear();
                self.persist_recent();
                self.toasts.show("Recent files cleared.".to_string())
            }
            // Expand/Collapse All (S7-016/017): every group's key floods (or
            // clears) the dock's collapsed set.
            Message::CalcsExpandAll => {
                self.dock.calcs_collapsed.clear();
                Task::none()
            }
            Message::CalcsCollapseAll => {
                self.dock.calcs_collapsed =
                    calcs::build(&self.drawing.polygons, &self.recomputed())
                        .groups
                        .iter()
                        .map(|g| g.code.clone())
                        .collect();
                Task::none()
            }
            Message::New => self.open_dialog(dialog::Dialog::confirm(
                "Start a new sketch? Unsaved changes will be lost.",
                dialog::Action::New,
            )),
            // --- The reference image (screen 10) ---
            Message::PhotoLoad => self.photo_load(),
            Message::PhotoShow => {
                if !self.bg_image.loaded() {
                    return self.open_dialog(dialog::Dialog::alert(
                        "No image loaded. Use Load Image first.",
                    ));
                }
                self.bg_image.visible = true;
                Task::none()
            }
            Message::PhotoHide => {
                self.bg_image.visible = false;
                Task::none()
            }
            Message::PhotoRemove => self.open_dialog(dialog::Dialog::confirm(
                "Remove the background image?",
                dialog::Action::PhotoRemove,
            )),
            Message::PhotoScaleStart => {
                if !self.bg_image.loaded() {
                    return self.open_dialog(dialog::Dialog::alert("No image loaded."));
                }
                self.bg_image.scale_mode = true;
                self.bg_image.scale_p1 = None;
                self.bg_image.scale_p2 = None;
                let _ = self.set_mode(view_state::Mode::Select);
                self.open_dialog(dialog::Dialog::alert(
                    "Click two points on the image that represent a known distance, then enter the measurement.",
                ))
            }
            Message::PhotoScaleClick(world) => {
                if !self.bg_image.scale_mode {
                    return Task::none();
                }
                if self.bg_image.scale_p1.is_none() {
                    self.bg_image.scale_p1 = Some(world);
                    return Task::none();
                }
                // The second click stores and asks for the known distance.
                self.bg_image.scale_p2 = Some(world);
                self.open_dialog(dialog::Dialog::prompt(
                    "Enter the known distance between the two points (in feet):",
                    "",
                    dialog::Action::PhotoScale,
                ))
            }
            Message::PhotoSmaller => {
                self.bg_image.scale_by(0.8);
                Task::none()
            }
            Message::PhotoLarger => {
                self.bg_image.scale_by(1.25);
                Task::none()
            }
            Message::PhotoRotateLeft => {
                self.bg_image.rotate(-90.0);
                Task::none()
            }
            Message::PhotoRotateRight => {
                self.bg_image.rotate(90.0);
                Task::none()
            }
            Message::PhotoEditModeToggle => {
                if !self.bg_image.loaded() {
                    return self
                        .toasts
                        .show("Load an image before entering Image Edit Mode.".to_string());
                }
                self.bg_image.edit_mode = !self.bg_image.edit_mode;
                self.bg_image.orient_mode = false;
                let note = if self.bg_image.edit_mode {
                    "Image Edit Mode: drag the image to reposition it; use the wheel to resize."
                } else {
                    "Image Edit Mode off."
                };
                self.toasts.show(note.to_string())
            }
            Message::PhotoOrientStart => {
                if !self.bg_image.loaded() {
                    return self
                        .toasts
                        .show("Load an image before using Orient Guide.".to_string());
                }
                self.bg_image.edit_mode = false;
                self.bg_image.orient_mode = true;
                self.bg_image.orient_p1 = None;
                self.bg_image.orient_p2 = None;
                let _ = self.set_mode(view_state::Mode::Select);
                self.toasts
                    .show("Orient Guide: click two points along a line in the image.".to_string())
            }
            Message::PhotoOrientClick(world) => {
                if !self.bg_image.orient_mode {
                    return Task::none();
                }
                if self.bg_image.orient_p1.is_none() {
                    self.bg_image.orient_p1 = Some(world);
                    return Task::none();
                }
                // The two points measured; the prompt asks which way the
                // guide should sit.
                self.bg_image.orient_p2 = Some(world);
                self.open_dialog(dialog::Dialog::prompt(
                    "Orient this guide Horizontal or Vertical?",
                    "Horizontal",
                    dialog::Action::PhotoOrient,
                ))
            }
            Message::PhotoOrientApply(vertical) => {
                let (Some(p1), Some(p2)) = (self.bg_image.orient_p1, self.bg_image.orient_p2)
                else {
                    return Task::none();
                };
                self.bg_image.apply_orient(p1, p2, vertical);
                self.bg_image.orient_mode = false;
                self.bg_image.orient_p1 = None;
                self.bg_image.orient_p2 = None;
                self.toasts.show("Image orientation applied.".to_string())
            }
            Message::PhotoFlipH => {
                self.bg_image.flip_h = !self.bg_image.flip_h;
                Task::none()
            }
            Message::PhotoFlipV => {
                self.bg_image.flip_v = !self.bg_image.flip_v;
                Task::none()
            }
            Message::PhotoIntensity(v) => {
                self.bg_image.opacity = ((v / 100.0) as f64).clamp(0.0, 1.0);
                Task::none()
            }
            Message::GeoOpen => {
                self.geo_menu.open();
                Task::none()
            }
            Message::About => self.open_dialog(dialog::Dialog::alert(&about_text())),
            Message::Guide(m) => {
                self.guide.update(m);
                Task::none()
            }
            Message::GeoLoadMap => {
                self.geo_menu.close();
                self.photo_load()
            }
            Message::GeoCoordinate => {
                self.geo_menu.close();
                let prefill = self.subject_info.gps_coordinates.trim().to_string();
                self.open_dialog(dialog::Dialog::prompt(
                    "Latitude, Longitude",
                    &prefill,
                    dialog::Action::GeoCoordinate,
                ))
            }
            Message::GeoAddress => {
                self.geo_menu.close();
                let prefill = self.subject_info.property_address.clone();
                self.open_dialog(dialog::Dialog::prompt(
                    "Property address",
                    &prefill,
                    dialog::Action::GeoAddress,
                ))
            }
            Message::GeoCoordinateValue(value) => {
                let Some((lat, lon)) = reference_image::parse_gps_pair(&value) else {
                    return self.toasts.show(
                        "Enter coordinates as decimal degrees (47.7737, -122.6539) or DMS (47°46'24.92\"N 122°39'14.18\"W)."
                            .to_string(),
                    );
                };
                self.subject_info.gps_coordinates = value.trim().to_string();
                self.subject_info.gps_latitude = format!("{lat}");
                self.subject_info.gps_longitude = format!("{lon}");
                self.toasts.show(format!(
                    "Georeference saved at {lat}, {lon}. Load a map image to trace it."
                ))
            }
            Message::GeoAddressValue(value) => {
                self.subject_info.property_address = value.trim().to_string();
                self.toasts
                    .show("Georeference address saved. Load a map image to trace it.".to_string())
            }
            Message::Shortcuts(m) => {
                self.shortcuts.open = matches!(m, shortcuts::ShortcutsMsg::Open);
                Task::none()
            }
            Message::Disto(m) => {
                let effect = self.disto.update(m);
                let session = self.disto.session;
                disto::set_session(session);
                match effect {
                    disto::Effect::None => Task::none(),
                    disto::Effect::Toast(text) => self.toasts.show(text),
                    disto::Effect::Scan => {
                        let manual = self.drawing.options.disto_discovery == "manual";
                        Task::perform(disto::scan_devices(manual), move |r| {
                            Message::Disto(disto::DistoMsg::ScannedFor(session, r))
                        })
                    }
                    disto::Effect::Connect(device) => {
                        Task::perform(disto::connect_device(device.id, session), move |r| {
                            Message::Disto(disto::DistoMsg::ConnectedFor(session, r))
                        })
                    }
                    disto::Effect::Disconnect => {
                        disto::disconnect_device();
                        Task::none()
                    }
                    disto::Effect::Measurement(meters) => self.disto_measurement(meters),
                }
            }
            Message::PopupMenu(m) => self.popup_msg(m),
            Message::PaletteMenu(kind) => {
                use popup_menu::Kind;
                // `event?.currentTarget || querySelector('[data-command=..]')`:
                // a hidden ribbon body has a zero rect.
                let (visible, x, y) = match kind {
                    Kind::Flip => (self.body == Body::Home, 570.0, 124.0),
                    Kind::CanvasBackground => (self.body == Body::View, 510.0, 117.0),
                    Kind::InterfaceTheme => (self.body == Body::View, 596.0, 117.0),
                    Kind::QuickAccess => (true, 188.0, 29.0),
                };
                let (x, y) = if visible { (x, y) } else { (0.0, 0.0) };
                self.popup_msg(popup_menu::PopupMsg::Open(kind, x, y))
            }
            Message::ToggleMiniMap => {
                // `toggleMiniMap`: an open map closes; otherwise snapshot the
                // canvas (the window screenshot cropped to it) and show it.
                if self.mini_map.take().is_some() {
                    return Task::none();
                }
                match self.window {
                    Some(id) => window::screenshot(id).map(Message::MiniMapShot),
                    None => {
                        self.mini_map = Some(minimap::blank(self.view.canvas));
                        Task::none()
                    }
                }
            }
            Message::MiniMapShot(shot) => {
                self.mini_map = minimap::snapshot(
                    &shot.rgba,
                    (shot.size.width, shot.size.height),
                    shot.scale_factor,
                    self.dock.width(),
                    self.pages_nav_width(),
                )
                .or_else(|| Some(minimap::blank(self.view.canvas)));
                Task::none()
            }
            Message::PagesNav(m) => self.pages_nav_msg(m),
            Message::MiniMapClose => {
                self.mini_map = None;
                Task::none()
            }
            Message::Print => {
                // `exportPDF`: the report goes to the OS viewer, which owns
                // the print dialog (the browser's print window).
                let path = print::temp_report_path(&self.file_name);
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if std::fs::write(&path, self.report_pdf()).is_err() {
                    return self
                        .toasts
                        .show("Could not write the report to print.".to_string());
                }
                print::open_with_os(&path);
                self.toasts
                    .show("Opening the appraisal report to print.".to_string())
            }
            Message::PrintPreviewOpen => {
                let pdf = self.report_pdf();
                match self.print_preview.open(self.page_layout.paper_size, pdf) {
                    Ok(()) => Task::none(),
                    Err(message) => self.toasts.show(message),
                }
            }
            Message::PrintPreview(m) => match m {
                print::PreviewMsg::Close => {
                    self.print_preview.close();
                    Task::none()
                }
                print::PreviewMsg::Print => {
                    let path = print::temp_report_path(&self.file_name);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if std::fs::write(&path, &self.print_preview.pdf).is_err() {
                        return self
                            .toasts
                            .show("Could not write the report to print.".into());
                    }
                    print::open_with_os(&path);
                    self.toasts
                        .show("Opening the appraisal report to print.".into())
                }
            },
            Message::Email => {
                // `emailSupport()`: window.location.href = mailto:... — the
                // desktop equivalent hands it to the OS handler.
                platform::open_target(std::ffi::OsStr::new(
                    "mailto:thatappraiserx@gmail.com?subject=Appraisal%20Sketch%20Support",
                ));
                Task::none()
            }
            Message::ImportSketch => {
                // `importFromFile` (S11-044): the picker, then the merge.
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Sketch", SKETCH_OPEN_EXTENSIONS)
                    .pick_file()
                {
                    let decoded = match std::fs::read(&path) {
                        Ok(bytes) => decode_sketch_bytes(&bytes),
                        Err(e) => Err(e.to_string()),
                    };
                    let decoded = match decoded {
                        Ok(decoded) => decoded,
                        Err(e) => {
                            return self
                                .toasts
                                .show(toast_error(&format!("Error importing file: {e}")));
                        }
                    };
                    let model = decoded.model;
                    return self.import_state(&model, &path.to_string_lossy());
                }
                Task::none()
            }
            Message::RoomShapePlaceAt(x, y) => {
                if let Some(pending) = self.pending_shape.take() {
                    let world = self.world(x, y);
                    let pts = room_shapes::shape_points(&pending);
                    let cx = pts.iter().map(|p| p.x).sum::<f64>() / pts.len() as f64;
                    let cy = pts.iter().map(|p| p.y).sum::<f64>() / pts.len() as f64;
                    let pts: Vec<sketch_core::geometry::Pt> = pts
                        .into_iter()
                        .map(|p| sketch_core::geometry::Pt {
                            x: world.x + (p.x - cx),
                            y: world.y + (p.y - cy),
                        })
                        .collect();
                    self.push_undo("Insert Room Shape");
                    for i in 0..pts.len() {
                        let a = pts[i];
                        let b = pts[(i + 1) % pts.len()];
                        let id = self.drawing.gen_id();
                        self.drawing.lines.push(sketch_core::model::SketchLine {
                            id,
                            start: a,
                            end: b,
                            kind: sketch_core::model::WallType::Exterior,
                            color: Some(self.drawing.draw_color.clone()),
                            line_thickness: None,
                            thickness: None,
                            dimension_hidden: false,
                            layer_id: None,
                            bow_angle: 0.0,
                            arc_height: 0.0,
                            line_style: None,
                            dimension_position: None,
                            dimension_anchor: None,
                        });
                    }
                    let _ = self.set_mode(view_state::Mode::Select);
                }
                Task::none()
            }
            Message::CommandPaletteRun => {
                // `run()`: close first, then `executeCommand(id)`.
                let found = command_palette::matches(&self.command_palette_query);
                self.command_palette_open = false;
                let Some(&index) = found.get(self.command_palette_selected) else {
                    return Task::none();
                };
                let (id, _, _) = command_palette::CATALOG[index];
                match command_palette::action(id) {
                    command_palette::Action::Run(msg) => self.update(msg),
                    command_palette::Action::Unavailable => {
                        self.toasts.show(format!("{id} is not available."))
                    }
                }
            }
            Message::Color(m) => self.color_msg(m),
            Message::WallSolids(on) => {
                self.drawing.options.wall_solids = on;
                Task::none()
            }
            Message::DefineFirst(on) => {
                self.drawing.options.define_first = on;
                Task::none()
            }
            Message::AreaType(kind, name) => {
                self.drawing.selected_area_type = kind;
                self.drawing.selected_area_name = name;
                Task::none()
            }
            Message::Units(u) => {
                self.drawing.options.units = u;
                Task::none()
            }
            Message::AreaProps => self.open_area_properties(),
            Message::AreaWorkflow(m) => self.workflow_msg(m),
            Message::OpenSelectMenu => {
                // Anchored under the Edit Tools Select button (x 213, y 68,
                // 46x49 in the ribbon body).
                self.select_menu
                    .open_at(SELECT_BUTTON_X, SELECT_BUTTON_Y + SELECT_BUTTON_H);
                Task::none()
            }
            Message::SelectByType(kind) => {
                self.select_menu.close();
                let gates = self.layer_gates();
                selection::select_by_type(
                    &mut self.selection,
                    &self.drawing.lines,
                    &self.drawing.polygons,
                    &self.drawing.texts,
                    &self.drawing.symbols,
                    kind,
                    &|id| gates.selectable(id),
                );
                if kind == select_menu::SelectKind::Dimensions {
                    // Every visible measurement label, so each can be moved
                    // or deleted.
                    let ids = handles::visible_dimension_ids(&self.dim_view(), &|id| {
                        gates.selectable(id)
                    });
                    self.selection.dimensions = ids.into_iter().collect();
                }
                self.sync_edit_tools();
                Task::none()
            }
            Message::CalcsGroup(code) => {
                if !self.dock.calcs_collapsed.remove(&code) {
                    self.dock.calcs_collapsed.insert(code);
                }
                Task::none()
            }
            Message::CalcsArea(id) => {
                if !self.dock.calcs_expanded.remove(&id) {
                    self.dock.calcs_expanded.insert(id);
                }
                // Clicking an area row selects it (`clearSelection` first).
                self.selection.clear();
                self.selection.polys.insert(id);
                self.sync_edit_tools();
                Task::none()
            }
            Message::OpenDefEditor => {
                let code = self
                    .drawing
                    .selected_area_type
                    .clone()
                    .unwrap_or_else(|| "UND".to_string());
                match self.area_codes.iter().find(|c| c.code == code) {
                    Some(entry) => {
                        self.def_editor = def_editor::DefEditor::open_on(entry);
                    }
                    None => {
                        return self
                            .toasts
                            .show("Select an area definition to edit.".to_string());
                    }
                }
                Task::none()
            }
            Message::DefEditor(m) => {
                use def_editor::DefMsg;
                match m {
                    DefMsg::Cancel => self.def_editor.open = false,
                    DefMsg::Save => {
                        let code = self.def_editor.code.clone();
                        self.def_editor.apply(&mut self.area_codes);
                        self.def_editor
                            .rename_polygons(&mut self.drawing.polygons, &mut self.drawing.lines);
                        self.def_editor.open = false;
                        self.persist_area_codes();
                        return self
                            .toasts
                            .show(format!("{code} settings saved permanently."));
                    }
                    other => self.def_editor.update(other),
                }
                Task::none()
            }
            Message::OpenNewCode => {
                let group = self
                    .area_codes
                    .first()
                    .map(|c| c.group.clone())
                    .unwrap_or_else(|| "OTH".to_string());
                self.new_code = new_code_dialog::NewCodeDialog::open_with(&group);
                Task::none()
            }
            Message::NewCode(m) => {
                use new_code_dialog::NewCodeMsg;
                match m {
                    NewCodeMsg::Cancel => self.new_code.open = false,
                    NewCodeMsg::Create => match self.new_code.create(&self.area_codes) {
                        Ok(entry) => {
                            let code = entry.code.clone();
                            self.area_codes.push(entry);
                            self.persist_area_codes();
                            self.new_code.open = false;
                            return self.toasts.show(format!("Area code \"{code}\" created."));
                        }
                        Err(notice) => return self.toasts.show(notice),
                    },
                    other => self.new_code.update(other),
                }
                Task::none()
            }
            Message::OpenBreakdown => {
                // The guard is geometric only — UND / uncoded areas are
                // filtered by the core engine's `compute_all`, not here.
                if self.drawing.polygons.iter().all(|p| p.points.len() < 3) {
                    return self
                        .toasts
                        .show("Define at least one area first.".to_string());
                }
                self.breakdown.open = true;
                Task::none()
            }
            Message::Breakdown(m) => {
                use breakdown_dialog::BreakdownMsg;
                match m {
                    BreakdownMsg::Close => self.breakdown.open = false,
                    BreakdownMsg::ToggleStyle => self.breakdown.update(m),
                    BreakdownMsg::Copy => {
                        let text = self.breakdown_report();
                        return iced::clipboard::write(text)
                            .chain(self.toasts.show("Calculations copied.".to_string()));
                    }
                    BreakdownMsg::DownloadJson => {
                        // AreaBreakdown.toJSON → area-breakdown.json.
                        let results = self.breakdown_results();
                        let data = sketch_core::breakdown::to_json(&results, &self.area_codes);
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Breakdown JSON", &["json"])
                            .set_file_name("area-breakdown.json")
                            .save_file()
                        {
                            let _ = std::fs::write(
                                &path,
                                serde_json::to_vec_pretty(&data).unwrap_or_default(),
                            );
                        }
                        return Task::none();
                    }
                    BreakdownMsg::DownloadXml => {
                        // AreaBreakdown.toXml → area-breakdown.xml (the BOM
                        // + CRLF the original writes). The subject line is
                        // the report subject's address.
                        let results = self.breakdown_results();
                        let report_subject = exporter::report_subject(&self.subject_info);
                        let xml = sketch_core::breakdown::to_xml(
                            &self.file_name,
                            &report_subject.property_address,
                            &report_subject.comment_tables,
                            &results,
                            &self.area_codes,
                        );
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Breakdown XML", &["xml"])
                            .set_file_name("area-breakdown.xml")
                            .save_file()
                        {
                            let _ = std::fs::write(&path, xml);
                        }
                        return Task::none();
                    }
                }
                Task::none()
            }
            Message::OpenCodesEditor => {
                let json = sketch_core::geometry::area_codes::to_json(self.area_codes.clone())
                    .unwrap_or_default();
                self.codes_editor.open(json);
                Task::none()
            }
            Message::Options(m) => self.options_msg(m),
            Message::PageLayout(m) => self.page_layout_msg(m),
            Message::CodesEditor(m) => {
                use codes_editor::CodesMsg;
                match m {
                    CodesMsg::Close => self.codes_editor.close(),
                    CodesMsg::Import if self.codes_editor.open => {
                        let session = self.codes_editor.session;
                        return Task::perform(codes_editor::pick_import(), move |path| {
                            Message::CodesEditor(CodesMsg::ImportChosen(session, path))
                        });
                    }
                    CodesMsg::Export if self.codes_editor.open => {
                        let body = match self.codes_editor.export_json() {
                            Ok(body) => body,
                            Err(error) => return self.toasts.show(error.to_string()),
                        };
                        let session = self.codes_editor.session;
                        return Task::perform(codes_editor::pick_export(), move |path| {
                            Message::CodesEditor(CodesMsg::ExportChosen {
                                session,
                                body,
                                path,
                            })
                        });
                    }
                    CodesMsg::ImportChosen(session, path) => {
                        if self.codes_editor.open && session == self.codes_editor.session {
                            if let Some(path) = path {
                                match std::fs::read_to_string(&path) {
                                    Ok(body) => {
                                        self.codes_editor.content =
                                            Some(iced::widget::text_editor::Content::with_text(
                                                body.trim_start_matches('\u{feff}'),
                                            ));
                                    }
                                    Err(error) => {
                                        return self
                                            .toasts
                                            .show(format!("Could not import area codes: {error}"))
                                    }
                                }
                            }
                        }
                    }
                    CodesMsg::ExportChosen {
                        session,
                        body,
                        path,
                    } => {
                        if self.codes_editor.open && session == self.codes_editor.session {
                            if let Some(path) = path {
                                if let Err(error) = std::fs::write(&path, body) {
                                    return self
                                        .toasts
                                        .show(format!("Could not export area codes: {error}"));
                                }
                            }
                        }
                    }
                    CodesMsg::Apply => {
                        if !self.codes_editor.open {
                            return Task::none();
                        }
                        // `AreaCodes.fromJSON` then `setAreaCodes`; the error
                        // messages are the catalogue's own.
                        match sketch_core::geometry::area_codes::from_json(
                            &self.codes_editor.text(),
                        ) {
                            Ok(codes) => {
                                self.area_codes = codes;
                                self.persist_area_codes();
                                self.codes_editor.close();
                                return self.toasts.show("Area code table updated.".to_string());
                            }
                            Err(e) => {
                                return self.toasts.show(e.to_string());
                            }
                        }
                    }
                    other => self.codes_editor.update(other),
                }
                Task::none()
            }
            Message::ToggleCalcs => {
                // `toggleCalcsPanel`: close the dock when Calcs is already
                // showing, otherwise open it on the Calcs tab.
                if self.dock.open && self.dock.active == dock::DockTab::Calcs {
                    self.dock.open = false;
                } else {
                    self.dock.open = true;
                    self.dock.active = dock::DockTab::Calcs;
                }
                self.sync_canvas();
                Task::none()
            }
            Message::OpenAreaAttributes(editable) => {
                let Some(poly) = self
                    .drawing
                    .polygons
                    .iter()
                    .find(|p| self.selection.polys.contains(&p.id))
                else {
                    return self
                        .toasts
                        .show("Select an area before editing its attributes.".to_string());
                };
                self.attr_dialog = attr_dialog::AttrDialog::open_with(&poly.attributes, editable);
                Task::none()
            }
            Message::AreaAttributes(m) => {
                use attr_dialog::AttrMsg;
                match m {
                    AttrMsg::Cancel => self.attr_dialog.open = false,
                    AttrMsg::Ok => {
                        let payload = self.attr_dialog.payload();
                        let editable = self.attr_dialog.editable;
                        self.attr_dialog.open = false;
                        // The undo step holds the state before the change.
                        let before = self.snapshot_model("Area Attributes");
                        if editable
                            && area_props::apply_attributes(
                                &mut self.drawing,
                                &self.selection,
                                &payload,
                            )
                        {
                            self.undo.push(before);
                        }
                    }
                    other => self.attr_dialog.update(other),
                }
                Task::none()
            }
            Message::AreaDialog(m) => {
                use area_dialog::AreaDialogMsg;
                match m {
                    AreaDialogMsg::Cancel => {
                        self.area_dialog.close();
                    }
                    AreaDialogMsg::Apply => {
                        let props = self.area_dialog.payload();
                        self.area_dialog.close();
                        let before = self.snapshot_model("Area Properties");
                        if area_props::apply_properties(&mut self.drawing, &self.selection, &props)
                        {
                            self.undo.push(before);
                            self.sync_edit_tools();
                        }
                    }
                    other => self.area_dialog.update(other),
                }
                Task::none()
            }
            Message::ContextAction(action) => {
                let area_target = self.context.area_target;
                self.context.close();
                use context_menu::Action;
                match action {
                    Action::EditDefinition => {
                        return area_target.map_or_else(Task::none, |id| {
                            self.workflow_msg(area_workflow::Msg::Open(id))
                        })
                    }
                    Action::Delete => return self.update(Message::Delete),
                    Action::Cut => return self.update(Message::Cut),
                    Action::Copy => return self.update(Message::Copy),
                    Action::Paste => return self.update(Message::Paste),
                    // The three selection transforms bank their own undo step.
                    Action::Rotate90 | Action::FlipHorz | Action::FlipVert => {
                        // The undo step captures the state BEFORE the change.
                        let before = self.snapshot_model(if action == Action::Rotate90 {
                            "Rotate"
                        } else {
                            "Flip Area"
                        });
                        let (_label, changed) = match action {
                            Action::Rotate90 => (
                                "Rotate",
                                transform::rotate_selected(
                                    &mut self.drawing,
                                    &self.selection,
                                    90.0,
                                ),
                            ),
                            Action::FlipHorz => (
                                "Flip Area",
                                transform::flip_selected(
                                    &mut self.drawing,
                                    &self.selection,
                                    transform::Axis::Horizontal,
                                ),
                            ),
                            _ => (
                                "Flip Area",
                                transform::flip_selected(
                                    &mut self.drawing,
                                    &self.selection,
                                    transform::Axis::Vertical,
                                ),
                            ),
                        };
                        if changed {
                            self.undo.push(before);
                        }
                    }
                    Action::Properties => {
                        return self.open_area_properties();
                    }
                    // sendToBack/bringToFront (S10-037/038): the selection
                    // auto-assigns onto a named layer at the stack's edge
                    // (autoAssignLayer banks its own undo step + toast).
                    Action::SendToBack => return self.auto_assign_layer("Send to Back", true),
                    Action::BringToFront => return self.auto_assign_layer("Bring to Front", false),
                    Action::AutoSubtract => {
                        // The undo step holds the state before the toggle.
                        let before = self.snapshot_model("Auto-Subtract");
                        if let Some(toast) =
                            area_props::toggle_auto_subtract(&mut self.drawing, &self.selection)
                        {
                            self.undo.push(before);
                            return self.toasts.show(toast);
                        }
                    }
                    Action::MakeNegative => return self.make_negative(),
                    Action::CloneArea => return self.clone_selected_areas(),
                }
                Task::none()
            }
            Message::Copy => {
                // `copySelected` does not bank undo; only paste and delete do.
                if let Some(clip) = clipboard::copy(&self.drawing, &self.selection) {
                    self.clipboard = Some(clip);
                }
                Task::none()
            }
            Message::Cut => {
                // `cutSelected`: copy, then `deleteSelected` (with its
                // `promptOnDeletion` confirm and the label / symbol removal).
                if let Some(clip) = clipboard::copy(&self.drawing, &self.selection) {
                    self.clipboard = Some(clip);
                    return self.update(Message::Delete);
                }
                Task::none()
            }
            Message::Paste => {
                let Some(clip) = self.clipboard.clone() else {
                    return Task::none();
                };
                self.push_undo("Paste");
                self.selection = clipboard::paste(&mut self.drawing, &clip);
                self.sync_edit_tools();
                Task::none()
            }
            Message::SelectAll => {
                let gates = self.layer_gates();
                selection::select_all(
                    &mut self.selection,
                    &self.drawing.lines,
                    &self.drawing.polygons,
                    &self.drawing.texts,
                    &self.drawing.symbols,
                    &|id| gates.selectable(id),
                );
                self.sync_edit_tools();
                Task::none()
            }
            Message::Delete => {
                // `deleteSelected`: nothing selected returns first; then
                // `options.promptOnDeletion` asks before anything is banked.
                if self.selection.is_empty() {
                    return Task::none();
                }
                if self.drawing.options.prompt_on_deletion {
                    return self.open_dialog(dialog::Dialog::confirm(
                        "Delete the selected sketch items?",
                        dialog::Action::DeleteSelected,
                    ));
                }
                self.delete_selected();
                Task::none()
            }
            Message::UndoTo(index) => {
                self.history.close();
                let current = self.snapshot_model("Edit");
                let states = self.undo.undo_to(index, current);
                if let Some(snap) = states.last() {
                    self.restore_snapshot(snap);
                    self.selection.clear();
                    self.sync_edit_tools();
                }
                Task::none()
            }
            Message::Undo => self.undo_step(),
            Message::Redo => self.redo_step(),
            Message::UndoHistory => {
                if !self.undo.can_undo() {
                    return self.toasts.show("Nothing to undo yet.".to_string());
                }
                // Anchored under the QAT's history button.
                self.history.open_at(QAT_HISTORY_X, QAT_Y + QAT_SIZE);
                Task::none()
            }
            Message::ApplyLineLength => {
                self.push_undo("Set Length");
                let text = self
                    .line_length_edit
                    .clone()
                    .or_else(|| self.edit_tools_data().map(|d| d.length_text));
                if let Some(text) = text {
                    self.drawing
                        .set_selected_line_length(&self.selection, &text);
                }
                Task::none()
            }
            Message::FlipOpening => {
                self.push_undo("Flip Opening");
                placed_symbol::flip_opening(&mut self.drawing.symbols, &self.selection.symbols);
                self.sync_canvas();
                Task::none()
            }
            Message::OpeningWider => {
                self.push_undo("Opening Width");
                placed_symbol::adjust_opening_width(
                    &mut self.drawing.symbols,
                    &self.selection.symbols,
                    0.5,
                );
                self.sync_canvas();
                Task::none()
            }
            Message::OpeningNarrower => {
                self.push_undo("Opening Width");
                placed_symbol::adjust_opening_width(
                    &mut self.drawing.symbols,
                    &self.selection.symbols,
                    -0.5,
                );
                self.sync_canvas();
                Task::none()
            }
            Message::SetSelectedTextStyle(k, v) => {
                self.set_selected_text_style(&k, v);
                Task::none()
            }
            Message::ToggleSelectedTextStyle(k) => {
                self.toggle_selected_text_style(&k);
                Task::none()
            }
            Message::AdjustSelectedTextSize(d) => {
                self.adjust_selected_text_size(d);
                Task::none()
            }
            Message::LineLengthInput(text) => {
                self.line_length_edit = Some(text);
                Task::none()
            }
            Message::CycleLineStyle => {
                self.push_undo("Wall Appearance");
                let next = match self
                    .drawing
                    .first_selected_line(&self.selection)
                    .and_then(|l| l.line_style.clone())
                    .as_deref()
                {
                    Some("dashed") => "dotted",
                    Some("dotted") => "solid",
                    _ => "dashed",
                };
                self.drawing
                    .set_selected_line_appearance(&self.selection, Some(next), None);
                Task::none()
            }
            Message::AreaCycle(which) => {
                use area_props::{AreaCycle, AreaStyle};
                let Some(poly) = self
                    .drawing
                    .polygons
                    .iter()
                    .find(|p| self.selection.polys.contains(&p.id))
                else {
                    return Task::none();
                };
                let style = match which {
                    AreaCycle::FillPattern => AreaStyle::FillPattern(
                        area_props::next_after(
                            &area_props::FILL_PATTERNS,
                            area_props::shown_pattern(poly),
                        )
                        .to_string(),
                    ),
                    AreaCycle::LineStyle => AreaStyle::LineStyle(
                        area_props::next_after(
                            &area_props::LINE_STYLES,
                            area_props::shown_line_style(poly),
                        )
                        .to_string(),
                    ),
                    AreaCycle::LineThickness => AreaStyle::LineThickness(area_props::next_after(
                        &area_props::LINE_THICKNESSES,
                        area_props::shown_thickness(poly),
                    )),
                };
                self.push_undo("Area Style");
                area_props::set_selected_area_style(&mut self.drawing, &self.selection, &style);
                self.sync_canvas();
                Task::none()
            }
            Message::AreaColorClicked(fill) => {
                let Some(poly) = self
                    .drawing
                    .polygons
                    .iter()
                    .find(|p| self.selection.polys.contains(&p.id))
                else {
                    return Task::none();
                };
                let hex = if fill {
                    poly.fill_color.clone().filter(|c| !c.is_empty())
                } else {
                    poly.line_color.clone().filter(|c| !c.is_empty())
                }
                .unwrap_or_else(|| if fill { "#4299e1" } else { "#3182ce" }.to_string());
                self.color_target = if fill {
                    color_dialog::ColorTarget::AreaFill
                } else {
                    color_dialog::ColorTarget::AreaLine
                };
                self.color.open_with(&hex);
                Task::none()
            }
            Message::TextFontPicked(family) => {
                self.set_selected_text_style("fontFamily", family);
                Task::none()
            }
            Message::TextColorClicked => {
                let hex = self
                    .edit_tools_data()
                    .and_then(|d| d.text)
                    .map(|t| {
                        format!(
                            "#{:02x}{:02x}{:02x}",
                            (t.color.r * 255.0).round() as u8,
                            (t.color.g * 255.0).round() as u8,
                            (t.color.b * 255.0).round() as u8
                        )
                    })
                    .unwrap_or_else(|| "#e2e8f0".to_string());
                self.color_target = color_dialog::ColorTarget::SelectedText;
                self.color.open_with(&hex);
                Task::none()
            }
            Message::TextFieldInput(field, text) => {
                self.text_edit = Some((field, text));
                Task::none()
            }
            Message::TextFieldApply(field) => {
                // The input's `change`: Size is a number (blank / junk reads
                // as 12 inside 6-72), Text the new wording (blank is kept off
                // an area's name by the caller's rules).
                if let Some((f, text)) = self.text_edit.take() {
                    if f == field {
                        match field {
                            ribbon::TextField::Size => {
                                self.set_selected_text_style("fontSize", text)
                            }
                            ribbon::TextField::Value => self.set_selected_text_style("text", text),
                        }
                    }
                }
                Task::none()
            }
            Message::LineColorClicked => {
                let hex = self
                    .edit_tools_data()
                    .map(|d| {
                        format!(
                            "#{:02x}{:02x}{:02x}",
                            (d.color.r * 255.0).round() as u8,
                            (d.color.g * 255.0).round() as u8,
                            (d.color.b * 255.0).round() as u8
                        )
                    })
                    .unwrap_or_else(|| "#3182ce".to_string());
                self.color_target = color_dialog::ColorTarget::SelectedLines;
                self.color.open_with(&hex);
                Task::none()
            }
            Message::CurveSelected => self.set_mode(view_state::Mode::Curve),
            Message::OpenColorDialog => {
                self.color_target = color_dialog::ColorTarget::DrawColor;
                self.color.open_with(&self.drawing.draw_color.clone());
                Task::none()
            }
            Message::Rail(tool) => self.rail(tool),
            Message::PanStart(x, y) => {
                self.view.pan_start(x as f64, y as f64);
                Task::none()
            }
            Message::PanMove(x, y) => {
                self.view.pan_move(x as f64, y as f64);
                Task::none()
            }
            Message::PanEnd => {
                self.view.pan_end();
                Task::none()
            }
            Message::Wheel {
                up,
                x,
                y,
                shift,
                ctrl,
            } => {
                // Shift+wheel while drawing = bow the last wall into a curve
                // (CanvasEngine's wheel handler). Selection scaling and the
                // select-mode curve steps join with screen 6.
                if self.bg_image.edit_mode {
                    // Image Edit Mode's wheel RESIZES instead of zooming
                    // (canvas-engine.js:1115).
                    self.bg_image.scale_by(if up { 1.05 } else { 0.95 });
                    return Task::none();
                }
                let height = self.view.curve_mode == view_state::CurveMode::Height;
                if self.view.mode == view_state::Mode::Select {
                    // Select mode: the wheel bows the selected walls (15 deg,
                    // Shift 5, Ctrl 1; height mode 1 ft, Shift/Ctrl 0.1), else
                    // scales the selected labels and symbols x1.08 / x0.92.
                    let step = match (height, ctrl, shift) {
                        (true, true, _) | (true, _, true) => 0.1,
                        (true, false, false) => 1.0,
                        (false, true, _) => 1.0,
                        (false, false, true) => 5.0,
                        (false, false, false) => 15.0,
                    };
                    let walls: Vec<i64> = self
                        .drawing
                        .lines
                        .iter()
                        .filter(|l| self.selection.lines.contains(&l.id))
                        .map(|l| l.id)
                        .collect();
                    if !walls.is_empty() {
                        self.push_undo("Curve Wall");
                        self.drawing
                            .adjust_curves(&walls, if up { step } else { -step }, height);
                        self.sync_canvas();
                        return Task::none();
                    }
                    if !self.selection.texts.is_empty()
                        || !self.selection.symbols.is_empty()
                        || !self.selection.area_labels.is_empty()
                        || !self.selection.area_sizes.is_empty()
                    {
                        return self.update(Message::ScaleSelected(if up { 1.08 } else { 0.92 }));
                    }
                }
                if shift && self.view.mode.is_drawing() && !self.drawing.lines.is_empty() {
                    let step = match (height, ctrl) {
                        (true, true) => 0.1,
                        (true, false) => 1.0,
                        (false, true) => 1.0,
                        (false, false) => 15.0,
                    };
                    self.push_undo("Bow");
                    self.drawing
                        .bow_last_wall(if up { step } else { -step }, height);
                    return Task::none();
                }
                self.view.wheel_zoom(up, x as f64, y as f64);
                Task::none()
            }
            Message::Key(cmd) => self.key(cmd),
            Message::Shapes(m) => self.shapes_msg(m),
            Message::RightPress(at) => {
                self.workflow.menu_at = at;
                if self.workflow.chooser.is_some() {
                    return Task::none();
                }
                // The Layers tree's contextmenu: iced's right presses reach
                // mouse_area with no cursor, so the row lookup rides the
                // tap's last-cursor point (see dock::layer_row_at).
                let dock_left = self.window_size.width - self.dock.width();
                if let Some(id) = dock::layer_row_at(
                    &self.dock,
                    &self.layers,
                    &self.area_layer_filter(),
                    dock_left,
                    at.x,
                    at.y,
                ) {
                    return self.update(Message::LayerRowMenu(id));
                }
                if self.canvas_point(at).is_none() && at.x > 41.0 {
                    return Task::none();
                }
                // The original opens the wall-shape menu from `contextmenu`
                // on the two draw tools as well as from their caret.
                let rail_button =
                    |y: f32| at.x >= 4.0 && at.x <= 36.0 && at.y >= y && at.y <= y + 32.0;
                if rail_button(283.0) {
                    self.shapes
                        .open_at(sketch_core::model::WallType::Exterior, 283.0);
                } else if rail_button(317.0) {
                    self.shapes
                        .open_at(sketch_core::model::WallType::Interior, 317.0);
                } else if self.view.mode != view_state::Mode::Measure {
                    // `contextmenu` on the canvas: end the pen, select what
                    // was clicked so the menu acts on it, then open the menu.
                    self.drawing.pen_up();
                    if self.view.mode == view_state::Mode::Select {
                        // `isHitSelected(hit)`: only a hit that is not already
                        // selected re-selects; empty space keeps the selection.
                        let gates = self.layer_gates();
                        let selectable = |id: i64| gates.selectable(id);
                        let (x, y) = self.canvas_point(at).unwrap_or((at.x, at.y));
                        let world = self.world(x, y);
                        let hit = selection::hit_test_at(
                            world,
                            &self.drawing.lines,
                            &self.drawing.polygons,
                            self.view.zoom,
                            &selectable,
                        );
                        let already = match hit {
                            Some(selection::Hit::Line(id)) => self.selection.lines.contains(&id),
                            Some(selection::Hit::Polygon(id)) => self.selection.polys.contains(&id),
                            None => true,
                        };
                        if hit.is_some() && !already {
                            selection::select_at(
                                &mut self.selection,
                                world,
                                &self.drawing.lines,
                                &self.drawing.polygons,
                                self.view.zoom,
                                false,
                                &selectable,
                            );
                            self.sync_edit_tools();
                        }
                    }
                    self.context.open_at(at.x, at.y);
                    let (x, y) = self.canvas_point(at).unwrap_or((at.x, at.y));
                    let world = self.world(x, y);
                    let gates = self.layer_gates();
                    if let Some(define_area::Region::Area(id)) =
                        define_area::resolve(&self.drawing, &gates, &mut self.region_cache, world)
                    {
                        self.context.area_target = Some(id);
                    }
                }
                Task::none()
            }
            Message::CanvasHover(x, y) => {
                let world = self.world(x, y);
                self.workflow_hover(world);
                self.drawing.hover(world, self.draw_ctx());
                Task::none()
            }
            Message::CanvasClick { x, y, double, ctrl } => {
                if self.view.mode == view_state::Mode::DefineArea {
                    return self.workflow_click(self.world(x, y));
                }
                // The reference image's modes intercept the click before the
                // regular logic (canvas-engine.js:740-760).
                let world = self.world(x, y);
                if self.bg_image.orient_mode {
                    return self.update(Message::PhotoOrientClick(world));
                }
                if self.bg_image.scale_mode {
                    return self.update(Message::PhotoScaleClick(world));
                }
                // Image Edit Mode: the drag ended (the release clears it).
                if self.bg_image.edit_mode {
                    self.photo_drag = None;
                    return Task::none();
                }
                // Place-symbol mode: click commits the symbol.
                if let Some(symbol_type) = self.placing_symbol.take() {
                    let world = self.world(x, y);
                    self.place_symbol_at(world, symbol_type);
                    return Task::none();
                }
                // Place-shape mode: click places the pending room shape.
                if self.view.mode == view_state::Mode::PlaceShape {
                    return self.update(Message::RoomShapePlaceAt(x, y));
                }
                // Place-text mode: click places the armed label.
                if let Some(label) = self.placing_text.take() {
                    let world = self.world(x, y);
                    self.push_undo("Place Text");
                    placed_text::place_text_at(&mut self.drawing, world, label);
                    let _ = self.set_mode(view_state::Mode::Select);
                    return Task::none();
                }
                // Measure mode: click adds a point.
                if self.view.mode == view_state::Mode::Measure {
                    let world = self.world(x, y);
                    self.drawing.measure_points.push(world);
                    self.sync_canvas();
                    return Task::none();
                }
                // Insert Vertex: click a wall to split it there.
                if self.view.mode == view_state::Mode::InsertVertex {
                    let world = self.world(x, y);
                    let before = self.snapshot_model("Insert Vertex");
                    return match self.drawing.insert_vertex(world, self.view.zoom) {
                        Ok(()) => {
                            self.undo.push(before);
                            self.sync_canvas();
                            Task::none()
                        }
                        Err(notice) => self.toasts.show(notice.to_string()),
                    };
                }
                // Chain-dim mode: click a wall to dimension its full run
                // (S9-040; `handleChainClick`).
                if self.view.mode == view_state::Mode::ChainDim {
                    let world = self.world(x, y);
                    return match chains::seed_wall(&self.drawing.lines, world, self.view.zoom) {
                        Some((seed, side)) => {
                            self.push_undo("Dimension Chain");
                            self.drawing.add_chain(seed, side);
                            self.sync_canvas();
                            Task::none()
                        }
                        None => self
                            .toasts
                            .show("Chain dimensions attach to straight walls.".to_string()),
                    };
                }
                // Select mode: click to select, Ctrl to add (selectLineAt).
                if self.view.mode == view_state::Mode::Select {
                    let world = self.world(x, y);
                    // Double-clicking a text label edits its wording.
                    if double {
                        if let Some(t) = self
                            .drawing
                            .texts
                            .iter()
                            .find(|t| placed_text::point_in_text(t, world, self.view.zoom))
                        {
                            let (id, current) = (t.id, t.text.clone());
                            return self.open_dialog(dialog::Dialog::prompt(
                                "Text:",
                                &current,
                                dialog::Action::EditText(id),
                            ));
                        }
                    }
                    if double {
                        let gates = self.layer_gates();
                        if let Some(define_area::Region::Area(id)) = define_area::resolve(
                            &self.drawing,
                            &gates,
                            &mut self.region_cache,
                            world,
                        ) {
                            self.workflow.menu_at =
                                Point::new(x + 41.0 + self.pages_nav_width(), y + 129.0);
                            return self.workflow_msg(area_workflow::Msg::Open(id));
                        }
                    }
                    // An area's label / size block (`hitTestAt`'s first pass).
                    if let Some((kind, id)) = self.area_text_at(world) {
                        if !ctrl {
                            self.selection.clear();
                        }
                        let set = if kind == handles::TextKind::AreaSize {
                            &mut self.selection.area_sizes
                        } else {
                            &mut self.selection.area_labels
                        };
                        if ctrl && set.contains(&id) {
                            set.remove(&id);
                        } else {
                            set.insert(id);
                        }
                        self.sync_edit_tools();
                        return Task::none();
                    }
                    // A dimension chain's tier wins first (S9-035): Ctrl
                    // toggles membership; a joined click arms the offset drag.
                    if let Some(chain_id) = self.drawing.chain_hit(world, self.view.zoom) {
                        if !ctrl {
                            self.selection.clear();
                        }
                        if ctrl && self.selection.chains.contains(&chain_id) {
                            self.selection.chains.remove(&chain_id);
                        } else {
                            self.selection.chains.insert(chain_id);
                        }
                        if self.selection.chains.contains(&chain_id) {
                            let tiers = self
                                .drawing
                                .dimension_chains
                                .iter()
                                .find(|c| c.id == chain_id)
                                .and_then(|c| self.chain_tiers(c));
                            if let Some(t) = tiers {
                                self.view.drag = Some(view_state::Drag::ChainOffset {
                                    chain_id,
                                    run_start: view_state::World {
                                        x: t.computed.start.x,
                                        y: t.computed.start.y,
                                    },
                                    run_normal: view_state::World {
                                        x: t.computed.normal.x,
                                        y: t.computed.normal.y,
                                    },
                                    moved: false,
                                });
                            }
                        }
                        self.sync_edit_tools();
                        self.sync_canvas();
                        return Task::none();
                    }
                    // Text hit-test takes priority.
                    let mut text_hit: Option<i64> = None;
                    for t in &self.drawing.texts {
                        if placed_text::point_in_text(t, world, self.view.zoom) {
                            text_hit = Some(t.id);
                            break;
                        }
                    }
                    if let Some(id) = text_hit {
                        if !ctrl {
                            self.selection.clear();
                        }
                        self.selection.texts.insert(id);
                        self.sync_edit_tools();
                        return Task::none();
                    }
                    // Symbol hit-test takes priority (P4: symbols before walls).
                    let mut symbol_hit: Option<i64> = None;
                    for sym in &self.drawing.symbols {
                        let bounds = self.symbols_lib().symbol(&sym.symbol_type).map(|d| {
                            (
                                d.bounds.min_x,
                                d.bounds.min_y,
                                d.bounds.max_x,
                                d.bounds.max_y,
                            )
                        });
                        if placed_symbol::point_in_symbol(sym, bounds, world, self.view.zoom) {
                            symbol_hit = Some(sym.id);
                            break;
                        }
                    }
                    if let Some(id) = symbol_hit {
                        if !ctrl {
                            self.selection.clear();
                        }
                        self.selection.symbols.insert(id);
                    } else if let Some(id) = self.dimension_hit_at(world) {
                        // A measurement label: Ctrl toggles it in or out.
                        if !ctrl {
                            self.selection.clear();
                        }
                        if ctrl && self.selection.dimensions.contains(&id) {
                            self.selection.dimensions.remove(&id);
                        } else {
                            self.selection.dimensions.insert(id);
                        }
                        self.sync_edit_tools();
                        return Task::none();
                    } else {
                        if !ctrl {
                            self.selection.symbols.clear();
                        }
                        let gates = self.layer_gates();
                        selection::select_at(
                            &mut self.selection,
                            world,
                            &self.drawing.lines,
                            &self.drawing.polygons,
                            self.view.zoom,
                            ctrl,
                            &|id| gates.selectable(id),
                        );
                    }
                    self.sync_edit_tools();
                    return Task::none();
                }
                if self.view.mode.is_drawing() {
                    let world = self.world(x, y);
                    // `pushUndo('Draw')` before the click changes the model.
                    self.push_undo("Draw");
                    let previous = self.drawing.lines.last().map(|line| line.id);
                    self.drawing.click(world, self.draw_ctx());
                    let closed = self.finish_wall_commit(previous);
                    if let Some(text) = self.drawing.pending_notice.take() {
                        return self.toasts.show(text);
                    }
                    if double {
                        self.drawing.pen_up();
                    }
                    if closed > 0 {
                        return self.toasts.show("Area closed and ready to define.".into());
                    }
                }
                Task::none()
            }
            Message::CanvasContext => {
                // The context menu itself arrives with selection (screen 6).
                self.drawing.pen_up();
                Task::none()
            }
        }
    }

    /// `setMode(mode)` plus the toast some modes announce.
    /// Canvas px -> world.
    fn world(&self, x: f32, y: f32) -> sketch_core::geometry::Pt {
        let w = self.view.to_world(x as f64, y as f64);
        sketch_core::geometry::Pt::new(w.x, w.y)
    }

    /// What drawing needs from the view and the mode.
    /// `exportPNG`: the page at `options.imageSize` (default 2048x1536), with
    /// the reference image beneath it when `options.saveImageWithSketch` is on
    /// and the image is visible.
    fn png_export_bytes(&self) -> Option<Vec<u8>> {
        let north = self.pages.page().north_angle;
        let page = exporter::build_export_page(&self.drawing, north, &self.area_codes);
        let size =
            sketch_core::export_png::parse_image_size(Some(&self.drawing.options.image_size));
        let backdrop = if self.drawing.options.save_image_with_sketch {
            self.export_backdrop()
        } else {
            None
        };
        let library = self.symbols_lib();
        let fit = sketch_core::export_png::fit_page_with_library(
            &page,
            size,
            backdrop.as_ref(),
            Some(&library.library),
        );
        let mut clean = self.drawing.clone();
        clean.pen_up();
        let colors = sketch_core::export_model::build_area_line_colors(&page, &self.area_codes);
        for line in &mut clean.lines {
            if let Some(c) = colors.get(&line.id) {
                line.color = Some(c.clone());
            }
        }
        let gates = self.layer_gates();
        let layers = self.export_layer_gates(&clean, &gates);
        let no_selection = selection::Selection::default();
        let no_photo = reference_image::BgImage::default();
        let bytes = png_overlay::render_clean(
            render::Scene {
                live_pointer: None,
                drawing: &clean,
                ctx: self.draw_ctx(),
                pan: (fit.pan_x, fit.pan_y),
                zoom: fit.zoom,
                filters: self.view.filters,
                dimensions: self.view.dimensions,
                placement: self.view.dimension_placement,
                selection: &no_selection,
                photo: if self.drawing.options.save_image_with_sketch {
                    &self.bg_image
                } else {
                    &no_photo
                },
                gates: &gates,
                area_codes: &self.area_codes,
                computed: &self.computed,
                symbols: &self.drawing.symbols,
                symbol_lib: Some(self.symbols_lib()),
                texts: &clean.texts,
                measure_points: &[],
                measure_runs: &[],
                underlay: None,
                curve_tool: false,
                curve_drag: None,
                select_mode: false,
                drag: None,
                drag_moved: false,
                text_zoom_floor: 2.0 * f64::from(fit.target.0) / 2048.0,
            },
            size,
            &layers,
        )?;
        // The headless renderer returns premultiplied RGBA, matching tiny-skia.
        let pixmap = sketch_core::export_png::tiny_skia::Pixmap::from_vec(
            bytes,
            sketch_core::export_png::tiny_skia::IntSize::from_wh(size.0, size.1)?,
        )?;
        sketch_core::export_png::encode_png(&pixmap)
    }

    fn export_layer_gates(
        &self,
        d: &drawing::Drawing,
        base: &layers::LayerGates,
    ) -> Vec<layers::LayerGates> {
        self.layers
            .flatten_layer_order()
            .iter()
            .filter(|l| self.layers.layer_visible(&l.id))
            .map(|layer| {
                let mut gates = base.clone();
                for line in &d.lines {
                    if self.entity_layer_id(line.layer_id.as_deref()) != layer.id {
                        gates.hidden.insert(line.id);
                    }
                }
                for poly in &d.polygons {
                    if self.entity_layer_id(poly.layer_id.as_deref()) != layer.id {
                        gates.hidden.insert(poly.id);
                    }
                }
                for text in &d.texts {
                    if self
                        .layer_save_name(text.layer_id)
                        .unwrap_or_else(|| self.layers.default_layer_id())
                        != layer.id
                    {
                        gates.hidden.insert(text.id);
                    }
                }
                for sym in &d.symbols {
                    if self
                        .layer_save_name(sym.layer_id)
                        .unwrap_or_else(|| self.layers.default_layer_id())
                        != layer.id
                    {
                        gates.hidden.insert(sym.id);
                    }
                }
                gates
            })
            .collect()
    }

    /// The loaded reference image as the PNG exporter wants it (decoded to
    /// RGBA); None when none is loaded, it is hidden, or it will not decode.
    fn export_backdrop(&self) -> Option<sketch_core::export_png::ExportBackdrop> {
        let bg = &self.bg_image;
        if !bg.visible {
            return None;
        }
        let iced::widget::image::Handle::Bytes(_, bytes) = bg.bytes.as_ref()? else {
            return None;
        };
        let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = decoded.dimensions();
        Some(sketch_core::export_png::ExportBackdrop {
            rgba: decoded.into_raw(),
            width,
            height,
            x: bg.x,
            y: bg.y,
            scale: bg.scale,
            rotation: bg.rotation,
            opacity: bg.opacity,
            flip_h: bg.flip_h,
            flip_v: bg.flip_v,
        })
    }

    /// `exportPdf` - the one-page report bytes; the paper choice is Page
    /// Layout's (`layout.paperSize`, Legal by default).
    fn report_pdf(&self) -> Vec<u8> {
        // `SketchReport.render`: one report page per sketch page, under the
        // Page Layout's captions and toggles.
        let pages = exporter::build_export_pages(&self.drawing, &self.pages, &self.area_codes);
        let options = exporter::export_options(&self.drawing);
        let subject = exporter::report_subject(&self.subject_info);
        let layout = exporter::report_layout(&self.page_layout);
        let sketches: Vec<_> = pages
            .iter()
            .enumerate()
            .map(|(index, page)| {
                let mut drawing = self.drawing.clone();
                if index != self.pages.current {
                    let p = &self.pages.pages[index];
                    drawing.lines = p.lines.clone();
                    drawing.polygons = p.polygons.clone();
                    drawing.texts = p.labels.clone();
                    drawing.symbols = p.symbols.clone();
                    drawing.dimension_chains = p.dimension_chains.clone();
                }
                let height = sketch_core::export_pdf::report_sketch_height(
                    page,
                    &self.area_codes,
                    &subject,
                    &layout,
                );
                let comment_inset =
                    sketch_core::export_pdf::sketch_comment_inset(&subject, &layout, height);
                report_sketch::raster(&report_sketch::svg(
                    &drawing,
                    page,
                    &self.area_codes,
                    &self.symbols_lib().library,
                    height,
                    comment_inset,
                ))
            })
            .collect();
        sketch_core::export_pdf::export_pdf_report_with_sketches(
            &pages,
            &options,
            &self.area_codes,
            &subject,
            &layout,
            &sketches,
        )
    }

    fn symbols_lib(&self) -> &crate::symlib::SymIndex {
        match self.symbols.as_ref() {
            Ok(lib) => lib,
            Err(_) => panic!("symbol library loaded at boot"),
        }
    }

    /// Use the same wall-completion path for mouse, keyboard, touch and bay
    /// mirror, without running it for pen-down/pen-up/no-op commands.
    fn finish_wall_commit(&mut self, previous_line_id: Option<i64>) -> usize {
        let current = self.drawing.lines.last().map(|line| line.id);
        match current.filter(|id| Some(*id) != previous_line_id) {
            Some(id) => self.drawing.finish_committed_wall(id, self.view.zoom),
            None => 0,
        }
    }

    fn draw_ctx(&self) -> drawing::DrawCtx {
        use sketch_core::model::WallType;
        drawing::DrawCtx {
            snap: self.view.snap,
            grid: self.view.grid_size,
            zoom: self.view.zoom,
            kind: match self.view.mode {
                view_state::Mode::DrawExterior => Some(WallType::Exterior),
                view_state::Mode::DrawInterior => Some(WallType::Interior),
                _ => None,
            },
            // Options › Step unit (the Options dialog arrives with screen 12).
            step_unit: self.step_unit,
        }
    }

    /// The wall-shape menu: open it from a caret, run a row, or advance the
    /// prompt chain (showWallShapeMenu).
    fn shapes_msg(&mut self, m: shapes::ShapesMsg) -> Task<Message> {
        use shapes::ShapesMsg as S;
        use sketch_core::model::WallType;
        match m {
            S::Open(kind, anchor_y) => self.shapes.open_at(kind, anchor_y),
            S::Line(kind) => {
                self.shapes.dismiss();
                return self.set_mode(match kind {
                    WallType::Exterior => view_state::Mode::DrawExterior,
                    WallType::Interior => view_state::Mode::DrawInterior,
                });
            }
            S::Choose(shape) => {
                if let Some((kind, _)) = self.shapes.open {
                    self.shapes.choose(shape, kind);
                }
                return self.open_shape_prompt();
            }
            S::Dismiss => self.shapes.dismiss(),
            S::Value(value) => return self.shape_value(&value),
        }
        Task::none()
    }

    /// Show the chain's current prompt (`promptDialog`).
    fn open_shape_prompt(&mut self) -> Task<Message> {
        match self.shapes.prompt() {
            Some((title, default)) => self.open_dialog(dialog::Dialog::prompt(
                title,
                default,
                dialog::Action::ShapeValue,
            )),
            None => Task::none(),
        }
    }

    /// A prompt's OK: finish the chain, or ask the next question.
    fn shape_value(&mut self, value: &str) -> Task<Message> {
        match self.shapes.submit(value) {
            Some(done) => {
                let centre = self.world(
                    (self.view.canvas.0 / 2.0) as f32,
                    (self.view.canvas.1 / 2.0) as f32,
                );
                self.drawing.add_wall_shape(
                    done.shape,
                    done.kind,
                    done.width,
                    done.height,
                    done.sides,
                    centre,
                );
                Task::none()
            }
            None => self.open_shape_prompt(),
        }
    }

    fn set_mode(&mut self, mode: view_state::Mode) -> Task<Message> {
        if mode != self.view.mode {
            self.workflow_close();
            self.workflow.hover = None;
        }
        if mode != view_state::Mode::DefineArea {
            self.workflow.armed = None;
        }
        // A chain belongs to one wall type: finish it before changing tools.
        if self.drawing.pen_down && mode != self.view.mode {
            self.drawing.pen_up();
        }
        match self.view.set_mode(mode) {
            Some(toast) => self.toasts.show(toast.into()),
            None => Task::none(),
        }
    }

    /// Commit a placed symbol at `world`, arming placement mode first.
    /// Hit-test a handle of a selected symbol. Returns (id, is_rotate).
    fn symbol_handle_at(&self, world: sketch_core::geometry::Pt) -> Option<(i64, bool)> {
        let screen = handles::to_screen(world, self.view.pan, self.view.zoom);
        for sym in &self.drawing.symbols {
            if !self.selection.symbols.contains(&sym.id) {
                continue;
            }
            let Some(def) = self.symbols_lib().symbol(&sym.symbol_type) else {
                continue;
            };
            let b = handles::symbol_box(
                sketch_core::geometry::Pt::new(sym.x, sym.y),
                sym.rotation,
                sym.scale,
                (
                    def.bounds.min_x,
                    def.bounds.min_y,
                    def.bounds.max_x,
                    def.bounds.max_y,
                ),
                self.view.pan,
                self.view.zoom,
            );
            match handles::hit_symbol_handle(&b, screen) {
                Some(handles::SymbolHandle::Rotate) => return Some((sym.id, true)),
                Some(handles::SymbolHandle::Resize) => return Some((sym.id, false)),
                None => {}
            }
        }
        None
    }

    /// `orientSelected(axis)`: selected area blocks, labels and symbols take
    /// the rotation outright - 0 for H, 90 for V (a symbol leaves its wall).
    fn orient_selected(&mut self, rotation: f64) {
        self.push_undo("Orient");
        for poly in &mut self.drawing.polygons {
            if self.selection.area_labels.contains(&poly.id) {
                poly.label_rotation = Some(rotation);
            }
            if self.selection.area_sizes.contains(&poly.id) {
                poly.area_size_rotation = Some(rotation);
            }
        }
        for t in &mut self.drawing.texts {
            if self.selection.texts.contains(&t.id) {
                t.rotation = rotation;
            }
        }
        for sym in &mut self.drawing.symbols {
            if self.selection.symbols.contains(&sym.id) {
                sym.rotation = rotation;
                sym.wall_id = None;
                sym.wall_fraction = None;
                sym.wall_side_explicit = false;
            }
        }
        self.sync_canvas();
    }

    /// Whether a text label or an area's label / size block is selected (the
    /// Edit Tools text group applies).
    fn has_text_selection(&self) -> bool {
        !self.selection.texts.is_empty()
            || !self.selection.area_labels.is_empty()
            || !self.selection.area_sizes.is_empty()
    }

    /// `setSelectedTextStyle(property, value)`: one `Text Style` step; labels
    /// take the property directly, an area's label and size blocks take the
    /// `label*` field of their area (`text` renames the area, label only).
    fn set_selected_text_style(&mut self, key: &str, value: String) {
        if !self.has_text_selection() {
            return;
        }
        self.push_undo("Text Style");
        let size = |v: &str| {
            v.parse::<f64>()
                .ok()
                .filter(|n| *n != 0.0)
                .unwrap_or(12.0)
                .clamp(6.0, 72.0)
        };
        let flag = |v: &str| v == "true";
        for t in &mut self.drawing.texts {
            if self.selection.texts.contains(&t.id) {
                match key {
                    "fontFamily" => t.font_family = value.clone(),
                    "fontSize" => t.font_size = size(&value),
                    "color" => t.color = value.clone(),
                    "text" => t.text = value.clone(),
                    "bold" => t.bold = flag(&value),
                    "italic" => t.italic = flag(&value),
                    "underline" => t.underline = flag(&value),
                    _ => {}
                }
            }
        }
        for poly in &mut self.drawing.polygons {
            let label = self.selection.area_labels.contains(&poly.id);
            if !label && !self.selection.area_sizes.contains(&poly.id) {
                continue;
            }
            match key {
                "fontFamily" => poly.label_font_family = Some(value.clone()),
                "fontSize" => poly.label_font_size = Some(size(&value)),
                "color" => poly.label_color = Some(value.clone()),
                "bold" => poly.label_bold = Some(flag(&value)),
                "italic" => poly.label_italic = Some(flag(&value)),
                "underline" => poly.label_underline = Some(flag(&value)),
                "text" if label => poly.name = value.clone(),
                _ => {}
            }
        }
        self.sync_canvas();
    }

    /// `toggleSelectedTextStyle(property)`: on unless everything selected
    /// already has it.
    fn toggle_selected_text_style(&mut self, key: &str) {
        if !self.has_text_selection() {
            return;
        }
        let texts_all = self
            .drawing
            .texts
            .iter()
            .filter(|t| self.selection.texts.contains(&t.id))
            .all(|t| match key {
                "bold" => t.bold,
                "italic" => t.italic,
                "underline" => t.underline,
                _ => false,
            });
        let areas_all = self
            .drawing
            .polygons
            .iter()
            .filter(|p| {
                self.selection.area_labels.contains(&p.id)
                    || self.selection.area_sizes.contains(&p.id)
            })
            .all(|p| {
                match key {
                    "bold" => p.label_bold,
                    "italic" => p.label_italic,
                    "underline" => p.label_underline,
                    _ => None,
                }
                .unwrap_or(false)
            });
        self.set_selected_text_style(key, (!(texts_all && areas_all)).to_string());
    }

    /// `adjustSelectedTextSize(delta)`: one `Text Size` step; an area block
    /// without a size of its own starts from the auto-post size (16).
    fn adjust_selected_text_size(&mut self, delta: i32) {
        if !self.has_text_selection() {
            return;
        }
        self.push_undo("Text Size");
        for t in &mut self.drawing.texts {
            if self.selection.texts.contains(&t.id) {
                t.font_size = (t.font_size + delta as f64).clamp(6.0, 72.0);
            }
        }
        let auto = if self.drawing.options.auto_post_font_size > 0.0 {
            self.drawing.options.auto_post_font_size
        } else {
            16.0
        };
        for poly in &mut self.drawing.polygons {
            if self.selection.area_labels.contains(&poly.id)
                || self.selection.area_sizes.contains(&poly.id)
            {
                let current = poly.label_font_size.filter(|s| *s != 0.0).unwrap_or(auto);
                poly.label_font_size = Some((current + f64::from(delta)).clamp(6.0, 72.0));
            }
        }
        self.sync_canvas();
    }

    fn place_symbol_at(&mut self, world: sketch_core::geometry::Pt, symbol_type: String) {
        self.push_undo("Place Symbol");
        let id = self.drawing.gen_id();
        let (def_wall_snap, def_gap_inches) = match self.symbols_lib().symbol(&symbol_type) {
            Some(def) => (def.wall_snap, def.wall_gap_width),
            None => (sketch_core::symbols::WallSnap::None, 0.0),
        };
        let placement = placed_symbol::resolve_symbol_placement(
            world,
            def_wall_snap,
            def_gap_inches,
            &self.drawing.lines,
            &self.drawing.polygons,
            None,
            self.view.zoom,
        );
        let wall_side_explicit = placement.wall_id.is_some();
        let placed = placed_symbol::PlacedSymbol {
            id,
            symbol_type,
            x: placement.x,
            y: placement.y,
            rotation: placement.rotation,
            scale: 1.0,
            flip_h: false,
            wall_id: placement.wall_id,
            wall_snap: placement.wall_snap,
            wall_gap_width: placement.wall_gap_width,
            wall_fraction: placement.wall_fraction,
            wall_side_flip: placement.wall_side_flip,
            wall_side_explicit,
            layer_id: 0,
        };
        let placed_id = placed.id;
        self.drawing.symbols.push(placed);
        let mut selected = std::collections::HashSet::new();
        selected.insert(placed_id);
        placed_symbol::refresh_hosted_symbols(
            &mut self.drawing.symbols,
            &selected,
            &self.drawing.lines,
        );
        self.selection.clear();
        let _ = self.set_mode(view_state::Mode::Select);
        self.selection.symbols.insert(placed_id);
        self.sync_edit_tools();
        self.dock.preview = None;
    }

    /// The rail's click handler (ui-manager.js): mode tools set the mode;
    /// Text / Symbols / Define Area open their dock panels. Shapes, Touch,
    /// Geo and Pages open their screens' dialogs (9, 12, 10, 10).
    fn rail(&mut self, tool: rail::RailTool) -> Task<Message> {
        use rail::RailTool as T;
        if tool == T::DefineArea {
            return self.workflow_msg(area_workflow::Msg::Activate);
        }
        if let Some(mode) = tool.mode() {
            return self.set_mode(mode);
        }
        if tool == T::Pages {
            return self.pages_nav_msg(pages_nav::NavMsg::Toggle);
        }
        let panel = match tool {
            T::Text => Some(dock::DockTab::Text),
            T::Symbols => Some(dock::DockTab::Symbols),
            T::DefineArea => Some(dock::DockTab::Areas),
            _ => None,
        };
        if let Some(tab) = panel {
            self.dock.update(dock::DockMsg::Show(tab));
            self.sync_canvas();
        }
        Task::none()
    }

    /// InputController's shortcuts.
    fn key(&mut self, cmd: input::KeyCmd) -> Task<Message> {
        if self.integration_setup.open {
            if matches!(cmd, input::KeyCmd::Escape) {
                return self.integration_setup.update(integration_setup::Msg::Close);
            }
            return Task::none();
        }
        if self.symbol_manager.open || self.autosave_prompt {
            if matches!(cmd, input::KeyCmd::Escape) {
                if self.symbol_manager.open && self.symbol_manager.confirm > 0 {
                    return self.manager_msg(symbol_manager::Msg::Cancel);
                }
                if self.symbol_manager.enlarged {
                    self.symbol_manager.enlarged = false;
                } else {
                    self.symbol_manager.open = false;
                }
                self.autosave_prompt = false;
            }
            return Task::none();
        }
        // A keyboard commit must use the last mouse event, including one
        // coalesced into the next canvas redraw rather than a full app update.
        if self.view.mode.is_drawing() && !self.drawing.keyboard_pointer {
            if let Some((x, y)) = self.canvas_pointer.get() {
                let world = self.world(x, y);
                if world != self.drawing.pointer {
                    self.drawing.hover(world, self.draw_ctx());
                }
            }
        }
        use input::KeyCmd as K;
        use view_state::Mode;
        match cmd {
            K::Save => return self.update(Message::SaveFile),
            K::Open => return self.update(Message::OpenFile),
            K::SaveAs => return self.update(Message::SaveFileAs),
            K::NewSketch => return self.update(Message::New),
            K::Shortcuts => {
                // F1 opens the sheet; F1/Escape close it (the original's
                // capture listener).
                self.shortcuts.open = !self.shortcuts.open;
                return Task::none();
            }
            K::Email => return self.update(Message::Email),
            // Pen up, else clear the selection, else the Select tool (S6-047).
            // The rename input's Escape cancels first (the original's input
            // keydown handler stops propagation and cancels).
            K::Escape => {
                // Escape cancels a dock drag in flight (HTML5 drag and drop).
                if self.dock_drag.as_ref().is_some_and(|d| d.active) {
                    self.dock_drag = None;
                    self.dock_click = None;
                    return Task::none();
                }
                if self.def_editor.open {
                    self.def_editor.open = false;
                    return Task::none();
                }
                if self.new_code.open {
                    self.new_code.open = false;
                    return Task::none();
                }
                if self.area_dialog.open {
                    self.area_dialog.close();
                    return Task::none();
                }
                if self.dialog.is_none()
                    && (self.workflow.advanced
                        || self.workflow.chooser.is_some()
                        || self.workflow.menu.is_some()
                        || self.view.mode == Mode::DefineArea)
                {
                    return self.workflow_escape();
                }
                if self.disto.open() {
                    // The chooser dismissed: `NotFoundError`, silent.
                    return self.update(Message::Disto(disto::DistoMsg::Cancel));
                }
                if self.print_preview.open {
                    self.print_preview.close();
                    return Task::none();
                }
                // The cheat sheet's capture listener closes first.
                if self.shortcuts.open {
                    self.shortcuts.open = false;
                    return Task::none();
                }
                if self.guide.open {
                    self.guide.open = false;
                    return Task::none();
                }
                if self.dock.layer_rename.is_some() {
                    self.dock.layer_rename = None;
                    return Task::none();
                }
                if self.view.mode == Mode::Measure {
                    self.drawing.measure_points.clear();
                    self.drawing.measure_runs.clear();
                    return self.set_mode(Mode::Select);
                }
                if self.drawing.pen_down {
                    self.drawing.pen_up();
                } else if !self.selection.is_empty() {
                    self.selection.clear();
                    self.sync_edit_tools();
                } else {
                    return self.set_mode(Mode::Select);
                }
            }
            // Ctrl+A selects everything (S6-043).
            // The keys run the same handlers as the Edit Tools buttons.
            K::Copy => return self.update(Message::Copy),
            K::Cut => return self.update(Message::Cut),
            K::Paste => return self.update(Message::Paste),
            K::Delete => {
                if self.view.mode == Mode::Measure {
                    self.drawing.measure_points.clear();
                    self.drawing.measure_runs.clear();
                    self.sync_canvas();
                    return Task::none();
                }
                return self.update(Message::Delete);
            }
            K::Undo => return self.undo_step(),
            K::Redo => return self.redo_step(),
            K::SelectAll => {
                let gates = self.layer_gates();
                selection::select_all(
                    &mut self.selection,
                    &self.drawing.lines,
                    &self.drawing.polygons,
                    &self.drawing.texts,
                    &self.drawing.symbols,
                    &|id| gates.selectable(id),
                );
                self.sync_edit_tools();
            }
            K::DrawExterior => return self.set_mode(Mode::DrawExterior),
            K::DrawInterior => return self.set_mode(Mode::DrawInterior),
            K::Select => return self.set_mode(Mode::Select),
            K::InsertVertex => return self.set_mode(Mode::InsertVertex),
            K::ToggleGrid => {
                self.view.grid = !self.view.grid;
                self.drawing.options.grid_enabled = self.view.grid;
            }
            K::ShowSymbols => return self.rail(rail::RailTool::Symbols),
            K::ShowText => return self.rail(rail::RailTool::Text),
            K::RoomShape => return self.update(Message::RoomShapeOpen),
            K::CommandPalette => return self.update(Message::CommandPaletteOpen),
            K::OrientH => self.orient_selected(0.0),
            K::RotateSelected(degrees) => return self.update(Message::RotateSelected(degrees)),
            K::ScaleSelected(factor) => return self.update(Message::ScaleSelected(factor)),
            K::FlipSketch(horizontal) => {
                return self.popup_msg(popup_menu::PopupMsg::Flip(if horizontal {
                    transform::Axis::Horizontal
                } else {
                    transform::Axis::Vertical
                }))
            }
            K::RotateSketch(degrees) => return self.update(Message::RotateSketchBy(degrees)),
            K::Nudge(dx, dy) => {
                // `moveSelected(dx, dy)` then `finalizeSelectionMove`.
                self.push_undo("Move");
                self.move_selection(dx, dy);
                let created = self.drawing.close_loops(self.view.zoom);
                self.sync_canvas();
                if created > 0 {
                    return self.toasts.show(if created == 1 {
                        "Area closed and ready to define.".to_string()
                    } else {
                        format!("{created} areas closed and ready to define.")
                    });
                }
            }
            K::OrientV => self.orient_selected(90.0),
            K::ShowAreas => return self.rail(rail::RailTool::DefineArea),
            K::CurveMode => {
                let toast = self.view.toggle_curve_mode();
                return self.toasts.show(toast.into());
            }
            K::CenterSketch => return self.status(statusbar::StatusMsg::CenterSketch),
            K::CenterFit => return self.status(statusbar::StatusMsg::CenterFit),
            K::SpaceDown => {
                if !self.view.space_pan {
                    self.view.space_pan = true;
                    self.view.space_panned = false;
                }
            }
            // --- drawing-mode keys (screen 5c) ---
            K::Buffer(key) => {
                self.drawing.buffer_key(key, self.draw_ctx());
            }
            K::DecimalPoint => {
                self.drawing.buffer_key("Tab", self.draw_ctx());
            }
            K::BufferBackspace => {
                if self.view.mode == Mode::Measure {
                    self.drawing.measure_points.pop();
                    self.sync_canvas();
                } else {
                    self.drawing.buffer_backspace();
                }
            }
            K::Enter => {
                if self.view.mode == Mode::Measure {
                    if !self.drawing.measure_points.is_empty() {
                        self.drawing
                            .measure_runs
                            .push(self.drawing.measure_points.clone());
                        self.drawing.measure_points.clear();
                    }
                    self.sync_canvas();
                } else {
                    if self.drawing.pen_down
                        && self.drawing.points.last().is_some_and(|anchor| {
                            *anchor != self.drawing.cursor
                                || drawing::parse_buffer(&self.drawing.buffer).is_some()
                        })
                    {
                        self.push_undo("Draw Line");
                    }
                    let previous = self.drawing.lines.last().map(|line| line.id);
                    self.drawing.enter_key(self.draw_ctx());
                    if self.finish_wall_commit(previous) > 0 {
                        return self.toasts.show("Area closed and ready to define.".into());
                    }
                }
            }
            K::AutoClose => {
                // Ctrl+F3 always; `A` only while the pen is down (the key map
                // cannot tell them apart, so the pen decides).
                if self.drawing.pen_down {
                    if let Some(kind) = self.draw_ctx().kind {
                        self.drawing.auto_close(kind);
                    }
                }
            }
            K::BayMirror => {
                if self.drawing.pen_down && self.drawing.lines.len() >= 2 {
                    self.push_undo("Bay Mirror");
                    let previous = self.drawing.lines.last().map(|line| line.id);
                    self.drawing.bay_window_mirror();
                    self.finish_wall_commit(previous);
                    self.sync_canvas();
                }
            }
            K::JumpEndpoint => {
                self.drawing.jump_to_endpoint();
            }
            K::Arrow { dir, ctrl, shift } => {
                let key = input::arrow_key_name(dir);
                self.drawing.arrow_move(key, ctrl, shift, self.draw_ctx());
            }
            // A tap without a drag toggles the last wall's dimension while
            // drawing (the selected dimensions join with screen 6).
            K::SpaceUp => {
                self.view.space_pan = false;
                if !self.view.space_panned {
                    if self.view.mode.is_drawing() {
                        let toast = self.drawing.toggle_last_dimension();
                        return self.toasts.show(toast.into());
                    }
                    // A tap with a selection toggles its measurement labels.
                    if !self.selection.is_empty() {
                        self.toggle_selected_dimensions();
                    }
                }
            }
        }
        Task::none()
    }

    /// Open a dialog; a prompt's input is focused with its value selected
    /// (`inputEl.focus(); inputEl.select()`).
    fn open_dialog(&mut self, d: dialog::Dialog) -> Task<Message> {
        let prompt = d.input.is_some();
        self.dialog = Some(d);
        if prompt {
            Task::batch([
                iced::widget::operation::focus(dialog::INPUT_ID),
                iced::widget::operation::select_all(dialog::INPUT_ID),
            ])
        } else {
            Task::none()
        }
    }

    /// `showSelectedAreaProperties`: the dialog for the first selected area,
    /// or the original's notice when the selection has no area.
    fn open_area_properties(&mut self) -> Task<Message> {
        let Some(poly) = self
            .drawing
            .polygons
            .iter()
            .find(|p| self.selection.polys.contains(&p.id))
        else {
            return self
                .toasts
                .show("Use Edit Tools to change the selected item.".to_string());
        };
        let props = area_props::AreaProperties::from_area(poly);
        let codes: Vec<String> = sketch_core::geometry::area_codes::default_area_codes()
            .iter()
            .map(|c| format!("{} - {}", c.code, c.name))
            .collect();
        self.area_dialog = area_dialog::AreaDialog::open_with(props, codes);
        Task::none()
    }

    /// The selection's bounding-box origin — the reference point the grid snap
    /// uses (`getSelectionBounds`).
    fn selection_bounds(&self) -> (f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        for line in self
            .drawing
            .lines
            .iter()
            .filter(|l| self.selection.lines.contains(&l.id))
        {
            min_x = min_x.min(line.start.x).min(line.end.x);
            min_y = min_y.min(line.start.y).min(line.end.y);
        }
        for poly in self
            .drawing
            .polygons
            .iter()
            .filter(|p| self.selection.polys.contains(&p.id))
        {
            for p in &poly.points {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
            }
        }
        for sym in self
            .drawing
            .symbols
            .iter()
            .filter(|s| self.selection.symbols.contains(&s.id))
        {
            min_x = min_x.min(sym.x);
            min_y = min_y.min(sym.y);
        }
        // `getSelectionBounds`: picked labels, pinned area blocks and
        // moved measurement labels count as well.
        for t in self
            .drawing
            .texts
            .iter()
            .filter(|t| self.selection.texts.contains(&t.id))
        {
            min_x = min_x.min(t.x);
            min_y = min_y.min(t.y);
        }
        for poly in &self.drawing.polygons {
            for at in [
                self.selection
                    .area_labels
                    .contains(&poly.id)
                    .then(|| handles::label_position(&self.drawing, poly)),
                self.selection
                    .area_sizes
                    .contains(&poly.id)
                    .then(|| handles::size_position(&self.drawing, poly)),
            ]
            .into_iter()
            .flatten()
            {
                min_x = min_x.min(at.x);
                min_y = min_y.min(at.y);
            }
        }
        for line in self.drawing.lines.iter().filter(|l| {
            self.selection.dimensions.contains(&l.id) && !self.selection.lines.contains(&l.id)
        }) {
            if line.dimension_position.is_some() {
                if let Some(p) = drawing::pinned_dimension_point(line) {
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                }
            }
        }
        if min_x.is_finite() {
            (min_x, min_y)
        } else {
            (0.0, 0.0)
        }
    }

    /// `createCurveDragState(line, pointerWorld)`: arms a curve drag on wall
    /// `id`, remembering its rise and where along the normal the pointer is.
    fn start_curve_drag(&mut self, id: i64, world: sketch_core::geometry::Pt) {
        let Some(line) = self.drawing.lines.iter().find(|l| l.id == id) else {
            return;
        };
        let (dx, dy) = (line.end.x - line.start.x, line.end.y - line.start.y);
        let chord = dx.hypot(dy);
        let mid = (
            (line.start.x + line.end.x) / 2.0,
            (line.start.y + line.end.y) / 2.0,
        );
        let start_pointer_normal = if chord > 0.001 {
            (world.x - mid.0) * (-dy / chord) + (world.y - mid.1) * (dx / chord)
        } else {
            0.0
        };
        self.view.drag = Some(view_state::Drag::Curve {
            line_id: id,
            start_arc_height: drawing::arc_height_of(line),
            start_pointer_normal,
        });
        self.drag_banked = false;
    }

    /// Which endpoint handle of wall `id` the pointer is on, if any (a 9px
    /// screen tolerance around each end).
    fn endpoint_handle(
        &self,
        id: i64,
        world: sketch_core::geometry::Pt,
    ) -> Option<(bool, view_state::World)> {
        let line = self.drawing.lines.iter().find(|l| l.id == id)?;
        let tol = 9.0 / (sketch_core::geometry::PIXELS_PER_FOOT * self.view.zoom).max(0.0001);
        let ds = (world.x - line.start.x).hypot(world.y - line.start.y);
        let de = (world.x - line.end.x).hypot(world.y - line.end.y);
        if ds <= tol {
            Some((
                true,
                view_state::World {
                    x: line.start.x,
                    y: line.start.y,
                },
            ))
        } else if de <= tol {
            Some((
                false,
                view_state::World {
                    x: line.end.x,
                    y: line.end.y,
                },
            ))
        } else {
            None
        }
    }

    /// `pushUndo(label)`: bank the model before an action.
    /// `handleChainClick`'s geometry helper for an App-side record.
    fn chain_tiers(&self, chain: &crate::chains::ChainRecord) -> Option<crate::chains::ChainTiers> {
        self.drawing.chain_tiers(chain)
    }

    /// `snapshotModel(label)` (the original's): the drawing plus the layer
    /// stack, which rides with every undo step.
    fn snapshot_model(&self, label: &str) -> crate::undo::Snapshot {
        let mut snap = self.drawing.snapshot(label);
        snap.layers = Some(self.layers.items.clone());
        snap.active_layer_id = Some(self.layers.active.clone());
        snap.north_angle = Some(self.pages.page().north_angle);
        snap
    }

    /// A snapshot restore: the drawing plus the layer stack it banked.
    fn restore_snapshot(&mut self, snap: &crate::undo::Snapshot) {
        self.drawing.restore(snap);
        if let Some(layers) = &snap.layers {
            self.layers.items = layers.clone();
        }
        if let Some(active) = &snap.active_layer_id {
            self.layers.active = active.clone();
        }
        // The compass of the page being edited (`Number.isFinite(prev.northAngle)`).
        if let Some(north) = snap.north_angle.filter(|n| n.is_finite()) {
            let current = self.pages.current;
            self.pages.pages[current].north_angle = north;
        }
    }

    fn push_undo(&mut self, label: &str) {
        let snap = self.snapshot_model(label);
        self.undo.push(snap);
    }

    // ------------------------------------------------------------------
    // The layer system (screen 10) — sketch-app.js 1115-1730.
    // ------------------------------------------------------------------

    /// `getEntityLayerId(entity)`: an entity's layer, falling back to the
    /// default one when the id is missing or no longer resolves to a live
    /// layer (files saved before layers existed, drained layers).
    fn entity_layer_id(&self, layer_id: Option<&str>) -> String {
        layer_id
            .filter(|lid| self.layers.layer(lid).is_some())
            .map(str::to_string)
            .unwrap_or_else(|| self.layers.default_layer_id())
    }

    /// `getAreaLayersForCurrentPage`'s lazy half (sketch-app.js:1204): every
    /// closed polygon whose dedicated layer drifted gets re-synced, exactly
    /// like the panel/page-sync call sites. Idempotent, never undo-banked,
    /// run at the top of every update — which also keeps the entity arrays
    /// layer-sorted (`applyLayerOrder` runs on every render in the original).
    fn ensure_area_layers(&mut self) {
        let drifted: Vec<i64> = {
            let areas: std::collections::HashMap<_, _> = self
                .layers
                .items
                .iter()
                .map(|l| (l.id.as_str(), l.area_poly_id.as_deref()))
                .collect();
            self.drawing
                .polygons
                .iter()
                .filter(|p| {
                    p.layer_id
                        .as_deref()
                        .and_then(|id| areas.get(id).copied().flatten())
                        != Some(p.id.to_string().as_str())
                })
                .map(|p| p.id)
                .collect()
        };
        for pid in drifted {
            self.assign_area_to_dedicated_layer(pid, false);
        }
        // applyLayerOrder (sketch-app.js:5608): the paint order is the layer
        // order; the stable sort keeps each layer's own draw order.
        let order = self.layers.flatten_layer_order();
        if order.is_empty() {
            return;
        }
        let mut ranks: std::collections::HashMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(rank, l)| (l.id.as_str(), rank))
            .collect();
        for layer in &self.layers.items {
            ranks.entry(layer.id.as_str()).or_insert(0);
        }
        let default_id = self.layers.default_layer_id();
        let default_rank = ranks.get(default_id.as_str()).copied().unwrap_or(0);
        let rank = |lid: Option<&str>| {
            lid.and_then(|id| ranks.get(id))
                .copied()
                .unwrap_or(default_rank)
        };
        self.drawing
            .lines
            .sort_by_key(|l| rank(l.layer_id.as_deref()));
        self.drawing
            .polygons
            .sort_by_key(|p| rank(p.layer_id.as_deref()));
    }

    /// `isEntityVisible`/`isEntityLocked` for the frame: which entity ids
    /// the render skips and the hit tests refuse.
    fn layer_gates(&self) -> crate::layers::LayerGates {
        let mut gates = crate::layers::LayerGates::default();
        let default_id = self.layers.default_layer_id();
        for line in &self.drawing.lines {
            let lid = line
                .layer_id
                .as_deref()
                .filter(|id| self.layers.layer(id).is_some())
                .unwrap_or(&default_id);
            if !self.layers.layer_visible(lid) {
                gates.hidden.insert(line.id);
            }
            if self.layers.layer_locked(lid) {
                gates.locked.insert(line.id);
            }
        }
        for poly in &self.drawing.polygons {
            let lid = poly
                .layer_id
                .as_deref()
                .filter(|id| self.layers.layer(id).is_some())
                .unwrap_or(&default_id);
            if !self.layers.layer_visible(lid) {
                gates.hidden.insert(poly.id);
            }
            if self.layers.layer_locked(lid) {
                gates.locked.insert(poly.id);
            }
        }
        // Symbols/texts keep the legacy numeric layer id (0 = the default
        // layer, exactly like the original's falsy layerId).
        for sym in &self.drawing.symbols {
            let on_default = sym.layer_id == 0
                || !self
                    .layers
                    .items
                    .iter()
                    .any(|l| l.id == sym.layer_id.to_string());
            let visible = if on_default {
                true
            } else {
                self.layers.layer_visible(&sym.layer_id.to_string())
            };
            if !visible {
                gates.hidden.insert(sym.id);
            }
        }
        for t in &self.drawing.texts {
            let on_default = t.layer_id == 0
                || !self
                    .layers
                    .items
                    .iter()
                    .any(|l| l.id == t.layer_id.to_string());
            let visible = if on_default {
                true
            } else {
                self.layers.layer_visible(&t.layer_id.to_string())
            };
            if !visible {
                gates.hidden.insert(t.id);
            }
        }
        gates
    }

    /// A pointer position (window CSS px) in canvas px, if it is over the canvas.
    fn canvas_point(&self, p: Point) -> Option<(f32, f32)> {
        let (ox, oy) = minimap::CANVAS_ORIGIN;
        let (x, y) = (p.x - ox - self.pages_nav_width(), p.y - oy);
        let (cw, ch) = self.view.canvas;
        (x >= 0.0 && y >= 0.0 && f64::from(x) < cw && f64::from(y) < ch).then_some((x, y))
    }

    /// While an area code is dragged: the region under the pointer that a drop
    /// would define, outlined in the code's colour (an existing area that is
    /// visible and unlocked, or a wall-enclosed face).
    fn update_area_drop_hint(&mut self) {
        let Some(drag) = &self.dock_drag else {
            return;
        };
        let dock::DragItem::Area { code, .. } = &drag.item else {
            return;
        };
        let color = self
            .area_codes
            .iter()
            .find(|c| c.code == *code)
            .map_or(theme::hex(0x858585), |c| theme::hex(dock::code_color(c)));
        let point = drag
            .cursor
            .filter(|_| drag.active)
            .and_then(|p| self.canvas_point(p));
        let hover = point.and_then(|(x, y)| {
            let world = self.world(x, y);
            let gates = self.layer_gates();
            let region =
                define_area::resolve(&self.drawing, &gates, &mut self.region_cache, world)?;
            if let define_area::Region::Area(id) = &region {
                if !gates.selectable(*id) {
                    return None;
                }
            }
            Some((define_area::region_ring(&self.drawing, &region), color))
        });
        if let Some(drag) = &mut self.dock_drag {
            drag.hover = hover;
        }
    }

    /// An area code dropped on the canvas (`defineAreaAt` through the live
    /// palette): the area under the point, or the enclosed wall region there,
    /// takes the code and its name.
    fn drop_area_code(&mut self, world: sketch_core::geometry::Pt, code: &str) -> Task<Message> {
        let gates = self.layer_gates();
        let Some(region) =
            define_area::resolve(&self.drawing, &gates, &mut self.region_cache, world)
        else {
            return self
                .toasts
                .show("Drop the area definition inside a closed shape.".to_string());
        };
        let id = match region {
            define_area::Region::Area(id) => {
                if !gates.selectable(id) {
                    return self.toasts.show(
                        "Show and unlock this area before changing its definition.".to_string(),
                    );
                }
                id
            }
            define_area::Region::Face(face) => {
                self.push_undo("Define Region");
                let id = define_area::materialize(&mut self.drawing, &face);
                self.assign_area_to_dedicated_layer(id, false);
                id
            }
        };
        let name = self
            .area_codes
            .iter()
            .find(|c| c.code == code)
            .map(|c| {
                if c.name.is_empty() {
                    c.code.clone()
                } else {
                    c.name.clone()
                }
            })
            .unwrap_or_else(|| code.to_string());
        self.push_undo("Define Area");
        define_area::define(&mut self.drawing, id, code, &name);
        // The area's auto-generated layer ("Area") takes its name.
        let layer = self
            .drawing
            .polygons
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.layer_id.clone());
        if let Some(layer) = layer {
            let generated = self
                .layers
                .layer(&layer)
                .is_some_and(|l| define_area::is_generated_layer_name(&l.name));
            if generated {
                let unique =
                    self.layers
                        .unique_layer_name(if name.is_empty() { "Area" } else { &name });
                self.layers.rename(&layer, unique);
            }
        }
        self.sync_canvas();
        self.toasts.show(
            if code == "UND" {
                "Classification cleared."
            } else {
                "Area defined. Drag another type, or click one to keep assigning."
            }
            .to_string(),
        )
    }

    /// `assignAreaToDedicatedLayer(poly, {activate})` (sketch-app.js:1313):
    /// the polygon and its boundary walls live on one dedicated layer at the
    /// top of the stack. Reuses the layer a re-closed area keeps (by
    /// areaPolyId), else creates one; drained source layers are removed
    /// (`removeEmptyAreaLayer`). Returns the dedicated layer's id.
    fn assign_area_to_dedicated_layer(&mut self, poly_id: i64, activate: bool) -> Option<String> {
        let poly = self.drawing.polygons.iter().find(|p| p.id == poly_id)?;
        let edge_ids: Vec<i64> = poly.edges.iter().filter_map(|e| e.line_id).collect();
        let base = if !poly.name.is_empty() && poly.name != "Undefined Area" {
            poly.name.clone()
        } else {
            "Area".to_string()
        };
        let poly_id_s = poly_id.to_string();

        // The walls' current layers are the move's sources.
        let mut source_ids: Vec<String> = Vec::new();
        for line in &self.drawing.lines {
            if edge_ids.contains(&line.id) {
                let lid = self.entity_layer_id(line.layer_id.as_deref());
                if !source_ids.contains(&lid) {
                    source_ids.push(lid);
                }
            }
        }

        // Repeated synchronization of an already dedicated area is idempotent.
        let mut layer_id = self
            .drawing
            .polygons
            .iter()
            .find(|p| p.id == poly_id)
            .and_then(|p| p.layer_id.clone())
            .filter(|lid| {
                self.layers
                    .layer(lid)
                    .and_then(|l| l.area_poly_id.clone())
                    .is_some_and(|aid| aid == poly_id_s)
            });

        // A re-closed (reopened) area keeps its original layer when all of
        // its walls already sit on that dedicated layer and its polygon is
        // gone (its areaPolyId no longer names a live polygon).
        if layer_id.is_none() && source_ids.len() == 1 {
            let candidate = source_ids[0].clone();
            let adopt = self
                .layers
                .layer(&candidate)
                .and_then(|l| l.area_poly_id.clone())
                .is_some_and(|aid| {
                    !self
                        .drawing
                        .polygons
                        .iter()
                        .any(|p| p.id.to_string() == aid)
                });
            if adopt
                && !self
                    .drawing
                    .reopened_areas
                    .iter()
                    .any(|record| record.original.layer_id.as_deref() == Some(candidate.as_str()))
            {
                layer_id = Some(candidate);
            }
        }

        let layer_id = match layer_id {
            Some(id) => id,
            None => {
                let id = format!("layer-{}", self.drawing.gen_id());
                let name = self.layers.unique_layer_name(&base);
                self.layers.create_layer_raw(id.clone(), name, false);
                id
            }
        };

        if let Some(l) = self.layers.items.iter_mut().find(|l| l.id == layer_id) {
            l.area_poly_id = Some(poly_id_s);
        }
        if let Some(p) = self.drawing.polygons.iter_mut().find(|p| p.id == poly_id) {
            p.layer_id = Some(layer_id.clone());
        }
        for line in &mut self.drawing.lines {
            if edge_ids.contains(&line.id) {
                line.layer_id = Some(layer_id.clone());
            }
        }
        if activate {
            self.layers.set_active(layer_id.clone());
        }
        // Dedicated area layers left empty by the move go away with their
        // area (removeEmptyAreaLayer, sketch-app.js:1373).
        let drained: Vec<String> = source_ids
            .into_iter()
            .filter(|id| *id != layer_id)
            .collect();
        for id in drained {
            self.remove_empty_area_layer(&id);
        }
        Some(layer_id)
    }

    /// `removeEmptyAreaLayer(layerId)`: a dedicated area layer with nothing
    /// left on it disappears with its area. The original's guard order leaves
    /// a dangling parentId when a drained CHILD layer is removed; the port
    /// drops the whole descendant branch (`Layers::remove`) instead — a
    /// superset, and the tree never keeps an orphan row.
    fn remove_empty_area_layer(&mut self, layer_id: &str) {
        if self
            .drawing
            .reopened_areas
            .iter()
            .any(|record| record.original.layer_id.as_deref() == Some(layer_id))
        {
            return;
        }
        let dedicated = self
            .layers
            .layer(layer_id)
            .map(|l| l.area_poly_id.is_some())
            .unwrap_or(false);
        if !dedicated {
            return;
        }
        let default_id = self.layers.default_layer_id();
        let resolve = |lid: Option<&str>| -> String {
            lid.filter(|id| self.layers.layer(id).is_some())
                .map(str::to_string)
                .unwrap_or_else(|| default_id.clone())
        };
        let occupied = self
            .drawing
            .polygons
            .iter()
            .any(|p| resolve(p.layer_id.as_deref()) == layer_id)
            || self
                .drawing
                .lines
                .iter()
                .any(|l| resolve(l.layer_id.as_deref()) == layer_id);
        if occupied {
            return;
        }
        // clearLayerSelection([layerId]): drop the selection entries whose
        // entity sits on the branch.
        self.clear_branch_selection(layer_id);
        self.layers.remove(layer_id);
        // removeLayer re-points the default layer if the active one died.
        if !self.layers.items.iter().any(|l| l.id == self.layers.active) {
            self.layers.active = default_id;
        }
    }

    /// `bindLayerTreeEvents`' drop (ui-manager.js:3085): a same-row release
    /// is the tree's click (set-active), anything else is `moveLayer` with
    /// the can-move guards BEFORE the `Move Layer` undo push. A leaf row has
    /// no real inside drop — its middle zone resolves to `after`, which is
    /// moveLayer's next-sibling behavior anyway.
    fn commit_layer_drop(
        &mut self,
        drag: String,
        over: Option<(String, crate::layers::DragPos)>,
    ) -> Task<Message> {
        self.dock.layer_drag = None;
        self.dock.layer_over = None;
        match over {
            Some((target, pos)) if target != drag => {
                let is_group = self
                    .layers
                    .layer(&target)
                    .map(|l| l.is_group)
                    .unwrap_or(false);
                let pos = if is_group {
                    pos
                } else if pos == crate::layers::DragPos::Inside {
                    crate::layers::DragPos::After
                } else {
                    pos
                };
                let can_move = self.layers.layer(&drag).is_some()
                    && !self.layers.descendant_layer_ids(&drag).contains(&target);
                if !can_move {
                    return Task::none();
                }
                self.push_undo("Move Layer");
                self.layers.move_layer(&drag, &target, pos);
            }
            _ => {
                self.layers.set_active(drag);
            }
        }
        Task::none()
    }

    /// `hasSelection()` (sketch-app.js:1682).
    fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// `getAreaLayersForCurrentPage`'s filter (sketch-app.js:1211): a layer
    /// shows when its `areaPolyId` names a polygon on the live page AND that
    /// polygon still points back at the layer. `ensure_area_layers` keeps
    /// both directions true; a deleted area's orphaned layer stays in the
    /// model (as in the original) but drops out of the tab.
    fn area_layer_filter(&self) -> crate::layers::AreaLayerFilter {
        let mut poly_ids = std::collections::HashSet::new();
        let mut layer_ids = std::collections::HashSet::new();
        for poly in &self.drawing.polygons {
            poly_ids.insert(poly.id.to_string());
            if let Some(lid) = &poly.layer_id {
                layer_ids.insert(lid.clone());
            }
        }
        crate::layers::AreaLayerFilter {
            poly_ids,
            layer_ids,
        }
    }

    // ------------------------------------------------------------------
    // The library managers (the captured build's Edit / Organize buttons).
    // ------------------------------------------------------------------

    // ------------------------------------------------------------------
    // Recent Files (S11-049..053).
    // ------------------------------------------------------------------

    /// `recordRecentFile(name, JSON.stringify(model))` + the sidecar write
    /// (the browser's localStorage; main() opts the real app in).
    fn record_recent(&mut self, name: &str, data: &str) {
        match recent::RecentFile::new(name, data) {
            Some(mut entry) => {
                entry.path = self.current_file.as_ref().map(|file| file.path.clone());
                recent::record(&mut self.recent, entry);
            }
            None => return,
        }
        self.persist_recent();
    }

    /// The `%APPDATA%\SketchRust\recent-files.json` mirror — the
    /// localStorage stand-in. Skipped unless the real app opted in.
    fn persist_recent(&self) {
        if std::env::var_os("SKETCH_PERSIST").is_none() {
            return;
        }
        let Ok(base) = std::env::var("APPDATA") else {
            return;
        };
        let dir = std::path::Path::new(&base).join("SketchRust");
        if std::fs::create_dir_all(&dir).is_ok() {
            let body = serde_json::to_string(&self.recent).unwrap_or_default();
            let _ = std::fs::write(dir.join("recent-files.json"), body);
        }
    }

    /// `%APPDATA%\SketchRust\interface-theme.txt` - the stand-in for
    /// `localStorage sketch.interfaceTheme`.
    fn interface_theme_path() -> Option<std::path::PathBuf> {
        let base = std::env::var("APPDATA").ok()?;
        Some(std::path::Path::new(&base).join("SketchRust/interface-theme.txt"))
    }

    /// `applyInterfaceTheme()` at start: the stored theme, else dark.
    fn load_interface_theme(&mut self) {
        let stored = Self::interface_theme_path().and_then(|p| std::fs::read_to_string(p).ok());
        theme::set_interface_theme(theme::InterfaceTheme::from_key(
            stored.as_deref().unwrap_or(""),
        ));
    }

    /// The three popup menus' choices (ui-manager.js:1428, 2507, 2531).
    fn popup_msg(&mut self, m: popup_menu::PopupMsg) -> Task<Message> {
        use popup_menu::PopupMsg as P;
        match m {
            P::Open(kind, x, y) => {
                self.popup_menu.open_at(kind, x, y);
                Task::none()
            }
            P::Close => {
                self.popup_menu.close();
                Task::none()
            }
            P::Flip(axis) => {
                // `flipSketch`: nothing drawn returns before the undo step.
                self.popup_menu.close();
                if self.drawing.lines.is_empty() && self.drawing.polygons.is_empty() {
                    return Task::none();
                }
                self.push_undo("Flip Sketch");
                let mut north = self.pages.page().north_angle;
                transform::flip_sketch(&mut self.drawing, axis, &mut north);
                let current = self.pages.current;
                self.pages.pages[current].north_angle = north;
                self.selection.clear();
                self.sync_edit_tools();
                Task::none()
            }
            P::Quick(command) => {
                use popup_menu::Quick;
                self.popup_menu.close();
                self.update(match command {
                    Quick::New => Message::New,
                    Quick::Open => Message::OpenFile,
                    Quick::SaveAs => Message::SaveFileAs,
                    Quick::Options => Message::Options(options_dialog::OptMsg::Open),
                })
            }
            P::Canvas(t) => {
                self.popup_menu.close();
                canvas_theme::set(t);
                self.toasts
                    .show(format!("Drawing background: {}", t.label()))
            }
            P::Interface(t) => {
                self.popup_menu.close();
                theme::set_interface_theme(t);
                if std::env::var_os("SKETCH_PERSIST").is_some() {
                    if let Some(path) = Self::interface_theme_path() {
                        if let Some(dir) = path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(path, t.key());
                    }
                }
                let name = match t {
                    theme::InterfaceTheme::Light => "Light",
                    theme::InterfaceTheme::Dark => "Dark",
                    theme::InterfaceTheme::Blue => "Blue",
                };
                self.toasts.show(format!("Interface: {name}"))
            }
        }
    }

    /// `%APPDATA%\SketchRust\default-subject-comments.txt` - the stand-in for
    /// `localStorage sketch.defaultSubjectComments`.
    fn default_comments_path() -> Option<std::path::PathBuf> {
        let base = std::env::var("APPDATA").ok()?;
        Some(std::path::Path::new(&base).join("SketchRust/default-subject-comments.txt"))
    }

    fn load_default_comments(&mut self) {
        if let Some(text) =
            Self::default_comments_path().and_then(|p| std::fs::read_to_string(p).ok())
        {
            self.default_comments = text;
        }
    }

    /// The Subject Info dialog (`editSubjectInfo`).
    fn subject_msg(&mut self, m: subject_info::SubjectMsg) -> Task<Message> {
        use subject_info::SubjectMsg as M;
        match m {
            M::Open => self.subject_dialog.open_from(&self.subject_info),
            M::Close => self.subject_dialog.close(),
            M::Tab(t) => self.subject_dialog.tab = t,
            M::Field(i, v) => {
                if let Some(slot) = self.subject_dialog.fields.get_mut(i) {
                    *slot = v;
                }
            }
            M::ExtraName(i, v) => {
                if let Some(slot) = self.subject_dialog.extra.get_mut(i) {
                    slot.0 = v;
                }
            }
            M::ExtraValue(i, v) => {
                if let Some(slot) = self.subject_dialog.extra.get_mut(i) {
                    slot.1 = v;
                }
            }
            M::ToggleExtras => self.subject_dialog.extras_open = !self.subject_dialog.extras_open,
            M::Comments(a) => self.subject_dialog.comments.perform(a),
            M::Table(i, a) => {
                if let Some(t) = self.subject_dialog.tables.get_mut(i) {
                    t.perform(a);
                }
            }
            M::AutoFill => {
                // The suite hook (`window.parent.__collectSketchSubjectInfo`)
                // does not exist in the standalone.
                return self.toasts.show(
                    "Auto-Fill is available when the sketch is open in the appraisal suite."
                        .to_string(),
                );
            }
            M::SaveDefault => {
                self.default_comments = self.subject_dialog.comments_text();
                if std::env::var_os("SKETCH_PERSIST").is_some() {
                    if let Some(path) = Self::default_comments_path() {
                        if let Some(dir) = path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(path, &self.default_comments);
                    }
                }
                return self.toasts.show("Default comments saved.".to_string());
            }
            M::ApplyDefault => {
                let text = self.default_comments.clone();
                self.subject_dialog.set_comments(&text);
            }
            M::Ok => match self.subject_dialog.commit(&self.subject_info) {
                Ok(next) => {
                    self.subject_info = next;
                    self.subject_dialog.close();
                    return self.toasts.show("Subject information updated.".to_string());
                }
                Err(msg) => return self.toasts.show(msg.to_string()),
            },
        }
        Task::none()
    }

    /// `sketch.areaCodes`: the table after a definition edit or a new code.
    fn persist_area_codes(&self) {
        if let Ok(text) = sketch_core::geometry::area_codes::to_json(self.area_codes.clone()) {
            let _ = persist::write("area-codes.json", &text);
        }
    }

    /// `persistLibraryState`: the text library and the favourite symbols.
    fn persist_library(&self) {
        let _ = persist::write_json("text-library-labels.json", &self.dock.text_labels);
        let _ = persist::write_json("symbol-favorites.json", &self.favorite_types());
    }

    /// The boot reads of the stored settings (the `UIManager` constructor):
    /// saved default options (applied at once and kept for Restore Defaults),
    /// the area-code table, the text library and the favourite symbols. Any
    /// missing or invalid file leaves the bundled value.
    fn load_stored_settings(&mut self) {
        if let Some(stored) = persist::read_json::<crate::options::Options>("default-options.json")
        {
            self.default_options = stored.clone();
            self.apply_options(stored);
        }
        if let Some(codes) = persist::read("area-codes.json")
            .and_then(|text| sketch_core::geometry::area_codes::from_json(&text).ok())
        {
            self.dock.area_codes = codes.clone();
            self.area_codes = codes;
        }
        if let Some(labels) =
            persist::read_json::<Vec<String>>("text-library-labels.json").filter(|l| !l.is_empty())
        {
            self.dock.text_labels = labels;
        }
        if let Some(stored) = persist::read_json::<Vec<String>>("symbol-favorites.json") {
            // Unknown symbol types drop out; an empty result means "defaults".
            let known: Vec<String> = stored
                .into_iter()
                .filter(|t| matches!(&*self.symbols, Ok(ix) if ix.by_id(t).is_some()))
                .collect();
            if !known.is_empty() {
                self.symbol_favorites = Some(known);
            }
        }
    }

    /// The boot sidecar read (main() calls this; the e2e never does).
    fn load_recent(&mut self) {
        let Ok(base) = std::env::var("APPDATA") else {
            return;
        };
        let path = std::path::Path::new(&base).join("SketchRust/recent-files.json");
        if let Ok(text) = std::fs::read_to_string(path) {
            self.recent = recent::parse(&text);
        }
    }

    /// The effective favorites: the saved list when set, else the library
    /// defaults (`loadLibraryState`).
    fn favorite_types(&self) -> Vec<String> {
        match &self.symbol_favorites {
            Some(list) => list.clone(),
            None => match &*self.symbols {
                Ok(ix) => ix.favorites.iter().map(|i| i.id.clone()).collect(),
                Err(_) => Vec::new(),
            },
        }
    }

    fn symbol_organizer_msg(&mut self, m: symbol_organizer::SymbolOrganizerMsg) -> Task<Message> {
        use symbol_organizer::SymbolOrganizerMsg as M;
        match m {
            M::Open => {
                let current = self.favorite_types();
                self.symbol_organizer.open(&current);
                return Task::none();
            }
            M::Close => {
                self.symbol_organizer.close();
                return Task::none();
            }
            M::Save => {
                if self.symbol_organizer.update(M::Save, &[]) {
                    self.symbol_favorites = Some(self.symbol_organizer.checked.clone());
                    self.persist_library();
                    return self.toasts.show("Favorite symbols updated.".to_string());
                }
                return Task::none();
            }
            M::Defaults => {
                let defaults: Vec<String> = match &*self.symbols {
                    Ok(ix) => ix.favorites.iter().map(|i| i.id.clone()).collect(),
                    Err(_) => Vec::new(),
                };
                self.symbol_organizer.update(M::Defaults, &defaults);
                return Task::none();
            }
            _ => {}
        }
        self.symbol_organizer.update(m, &[]);
        Task::none()
    }

    /// `clearLayerSelection(descendantLayerIds)`: the selection entries
    /// whose entity sits on the branch (texts/symbols keep the legacy numeric
    /// layer ids and can never select onto a string layer).
    fn clear_branch_selection(&mut self, layer_id: &str) {
        let branch: std::collections::HashSet<String> = self
            .layers
            .descendant_layer_ids(layer_id)
            .into_iter()
            .collect();
        let default_id = self.layers.default_layer_id();
        let resolve = |lid: Option<&str>| -> String {
            lid.filter(|id| self.layers.layer(id).is_some())
                .map(str::to_string)
                .unwrap_or_else(|| default_id.clone())
        };
        let dead_lines: std::collections::HashSet<i64> = self
            .drawing
            .lines
            .iter()
            .filter(|l| branch.contains(&resolve(l.layer_id.as_deref())))
            .map(|l| l.id)
            .collect();
        let dead_polys: std::collections::HashSet<i64> = self
            .drawing
            .polygons
            .iter()
            .filter(|p| branch.contains(&resolve(p.layer_id.as_deref())))
            .map(|p| p.id)
            .collect();
        self.selection.lines.retain(|id| !dead_lines.contains(id));
        self.selection.polys.retain(|id| !dead_polys.contains(id));
    }

    /// `autoAssignLayer(name, toBack)` (sketch-app.js:1688): the selection
    /// moves onto a named top-level layer (created on demand) that is pushed
    /// to the back or front of the stack, then the entities re-sort within
    /// their layer segments (`reorderSelectionWithinLayers(!toBack)`).
    ///
    /// Labels and symbols keep the legacy numeric `layerId` (0) this port's
    /// PlacedText/PlacedSymbol carry; in the original they sit on the phantom
    /// "0" layer — one bottom segment the reorder can't visibly split — so
    /// assigning/reordering them is a no-op here too.
    fn auto_assign_layer(&mut self, name: &str, to_back: bool) -> Task<Message> {
        if !self.has_selection() {
            return Task::none();
        }
        self.push_undo("Auto-Assign Layer");
        let known = self
            .layers
            .items
            .iter()
            .any(|l| l.name == name && !l.is_group && l.parent_id.is_none());
        let layer_id = if known {
            self.layers
                .items
                .iter()
                .find(|l| l.name == name)
                .map(|l| l.id.clone())
                .expect("name found")
        } else {
            let id = format!("layer-{}", self.drawing.gen_id());
            self.layers
                .create_layer_raw(id.clone(), name.to_string(), false);
            id
        };
        if let Some(l) = self.layers.items.iter_mut().find(|l| l.id == layer_id) {
            l.visible = true;
            l.locked = false;
        }
        // Restack: [layer, ...others] or [...others, layer].
        if let Some(i) = self.layers.items.iter().position(|l| l.id == layer_id) {
            let layer = self.layers.items.remove(i);
            if to_back {
                self.layers.items.insert(0, layer);
            } else {
                self.layers.items.push(layer);
            }
        }
        // Assign the selected entities onto the layer.
        let sel = self.selection.clone();
        for line in &mut self.drawing.lines {
            if sel.lines.contains(&line.id) {
                line.layer_id = Some(layer_id.clone());
            }
        }
        for poly in &mut self.drawing.polygons {
            if sel.polys.contains(&poly.id) {
                poly.layer_id = Some(layer_id.clone());
            }
        }
        self.reorder_selection_within_layers(!to_back);
        let toast = format!("Moved selection to layer {name}.");
        self.toasts.show(toast)
    }

    /// `reorderSelectionWithinLayers(toFront)` (sketch-app.js:1727): within
    /// each layer's segment of the paint order the selected entities slide to
    /// the segment's front (or back). Array order only — no layerId changes.
    /// Lines and polygons are the entities this port's layer model reaches;
    /// see auto_assign_layer for the labels/symbols equivalence.
    fn reorder_selection_within_layers(&mut self, to_front: bool) {
        // The paint order at the moment of the call; every entity resolves
        // into a live layer segment (`getEntityLayerId` falls back to the
        // default layer), so nothing can drop out of the arrays.
        let order: Vec<String> = self
            .layers
            .flatten_layer_order()
            .iter()
            .map(|l| l.id.clone())
            .collect();
        if order.is_empty() {
            return;
        }
        let default_rank = order
            .iter()
            .position(|o| *o == self.layers.default_layer_id())
            .unwrap_or(0);
        let rank = |lid: Option<&str>| -> usize {
            let resolved = self.entity_layer_id(lid);
            order
                .iter()
                .position(|o| *o == resolved)
                .unwrap_or(default_rank)
        };
        let line_ranks: Vec<usize> = self
            .drawing
            .lines
            .iter()
            .map(|l| rank(l.layer_id.as_deref()))
            .collect();
        let poly_ranks: Vec<usize> = self
            .drawing
            .polygons
            .iter()
            .map(|p| rank(p.layer_id.as_deref()))
            .collect();
        // The original's polygon predicate folds the area label/size ids in;
        // this port selects areas through `selectedPolyIds` alone.
        let sel_lines = self.selection.lines.clone();
        let sel_polys = self.selection.polys.clone();
        let lines = std::mem::take(&mut self.drawing.lines);
        self.drawing.lines = reorder_within_layers(
            lines,
            line_ranks,
            &|l: &sketch_core::model::SketchLine| sel_lines.contains(&l.id),
            to_front,
        );
        let polys = std::mem::take(&mut self.drawing.polygons);
        self.drawing.polygons = reorder_within_layers(
            polys,
            poly_ranks,
            &|p: &sketch_core::model::Polygon| sel_polys.contains(&p.id),
            to_front,
        );
    }

    /// `exportLayer(layerId)` (sketch-app.js:1567, S10-033): the layer and,
    /// for groups, its children together with every entity on them, as a
    /// pretty-printed JSON file (importLayerFile's input shape).
    fn export_layer(&mut self, id: &str) -> Task<Message> {
        let Some(layer) = self.layers.layer(id).cloned() else {
            return Task::none();
        };
        let data = self.export_layer_data(id);
        // `${safeName}-layer.json` — [^\w\- ] stripped, blank falls back.
        let safe: String = layer
            .name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
            .collect::<String>()
            .trim()
            .to_string();
        let safe = if safe.is_empty() {
            "layer".to_string()
        } else {
            safe
        };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Sketch layer", &["json"])
            .set_file_name(format!("{safe}-layer.json"))
            .save_file()
        {
            if let Ok(body) = serde_json::to_string_pretty(&data) {
                let _ = std::fs::write(&path, body);
                let note = format!("Exported layer \"{}\".", layer.name);
                return self.toasts.show(note);
            }
        }
        Task::none()
    }

    /// The Options dialog's messages. OK / Cancel / Restore / Save-as-Default
    /// are App-level; everything else is the dialog's own state.
    fn options_msg(&mut self, m: options_dialog::OptMsg) -> Task<Message> {
        use options_dialog::OptMsg;
        match m {
            OptMsg::Open => {
                self.options_dialog =
                    options_dialog::OptionsDialog::open_with(self.drawing.options.clone());
                Task::none()
            }
            OptMsg::Ok => {
                let next = self.options_dialog.draft.clone();
                self.options_dialog.close();
                self.apply_options(next);
                self.toasts.show("Options applied.".to_string())
            }
            OptMsg::Cancel => {
                self.options_dialog.close();
                Task::none()
            }
            OptMsg::RestoreDefaults => {
                // `data-defaults`: applyOptions(this.defaultOptions) — the
                // code defaults; the paper choice is Page Layout's, not an
                // options key, so it is left untouched.
                self.options_dialog.update(OptMsg::RestoreDefaults);
                let next = crate::options::Options {
                    paper_size: self.drawing.options.paper_size,
                    ..self.default_options.clone()
                };
                self.apply_options(next);
                self.toasts
                    .show("Options restored to defaults.".to_string())
            }
            OptMsg::SaveDefaults => {
                // `data-save-defaults`: the draft is applied, becomes what
                // Restore Defaults restores, and is stored for the next start
                // (`localStorage sketch.defaultOptions`; the sidecar here).
                let next = self.options_dialog.draft.clone();
                self.apply_options(next.clone());
                self.default_options = next.clone();
                let text = match persist::write_json("default-options.json", &next) {
                    Some(true) => "Current options saved as defaults.",
                    Some(false) => "Current options applied, but could not be saved as defaults.",
                    // No store (the test harness): kept for this run only.
                    None => "Current options saved as defaults for this session.",
                };
                self.toasts.show(text.to_string())
            }
            other => {
                self.options_dialog.update(other);
                Task::none()
            }
        }
    }

    /// The Page Layout dialog arm (`UIManager.showPageLayout`, S11-033..037):
    /// Apply stores the draft with `setPageLayout`'s normalize and toasts
    /// `Page layout applied: Legal.`; Restore Defaults resets the form; the
    /// other ticks edit the draft.
    fn page_layout_msg(&mut self, m: page_layout::LayoutMsg) -> Task<Message> {
        use page_layout::LayoutMsg;
        match m {
            LayoutMsg::Open => {
                self.page_layout_dialog =
                    page_layout::PageLayoutDialog::open_with(self.page_layout.clone());
                Task::none()
            }
            LayoutMsg::Apply => {
                let next = self.page_layout_dialog.draft.clone();
                self.page_layout_dialog.close();
                // `this.app.setPageLayout(layout)`: the draft's pickers can
                // only produce allowed papers/forms, so it is already
                // normalized.
                self.page_layout = next;
                self.toasts.show(format!(
                    "Page layout applied: {}.",
                    page_layout::paper_label(self.page_layout.paper_size)
                ))
            }
            LayoutMsg::Cancel => {
                self.page_layout_dialog.close();
                Task::none()
            }
            other => {
                self.page_layout_dialog.update(other);
                Task::none()
            }
        }
    }

    /// `SketchApp.applyOptions` (refs/sketch-app.js:524): the clamp pass and
    /// the state mirrors (`gridEnabled`, `areaLabelSuffix`→`autoPost.suffix`
    /// reads it from `Units`), then the canvas re-derives its colours.
    fn apply_options(&mut self, mut next: crate::options::Options) {
        // Widths: `Math.max(1, Number(iw) || 2)` / `Math.max(2, ... || iw*2)`.
        let interior_raw = next.interior_width;
        next.interior_width = if interior_raw.is_finite() && interior_raw > 0.0 {
            interior_raw.max(1.0)
        } else {
            2.0
        };
        let exterior_raw = next.exterior_width;
        next.exterior_width = if exterior_raw.is_finite() && exterior_raw > 0.0 {
            exterior_raw.max(2.0)
        } else {
            next.interior_width * 2.0
        };
        // Thicknesses in inches; zero (or a broken number) disables solids.
        next.exterior_thickness = if next.exterior_thickness.is_finite() {
            next.exterior_thickness.max(0.0)
        } else {
            0.0
        };
        next.interior_thickness = if next.interior_thickness.is_finite() {
            next.interior_thickness.max(0.0)
        } else {
            0.0
        };
        // `state.gridEnabled = options.gridEnabled !== false` (the status
        // bar's toggle keeps writing back — see Message::Status arms).
        self.view.grid = next.grid_enabled;
        self.drawing.options = next;
        self.sync_edit_tools();
    }

    /// `importFromFile`'s payload merge (refs/sketch-app.js:5435): an Import
    /// undo step, then the walls (Exterior, fresh ids), text labels, and UND
    /// areas land offset in DXF-space rows. Toasts live in the handler.
    /// `runImport(file)` (CAD IMPORT UI LAYER): a `.dwg` gets the hint; a
    /// `.dxf` is parsed, inserted (one `Import` undo step, offset (2, 2))
    /// and described in an alert; a parse failure is
    /// `Could not import <name>: <reason>`.
    fn import_cad_file(&mut self, path: &std::path::Path) -> Task<Message> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        if name.to_ascii_lowercase().ends_with(".dwg") {
            return self.open_dialog(dialog::Dialog::alert(
                sketch_core::cad_import::cad_import_dwg_hint(),
            ));
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return self.open_dialog(dialog::Dialog::alert(&format!(
                "Could not import {name}: the file could not be read."
            )));
        };
        match sketch_core::cad_import::parse_dxf(&text, None, (2.0, 2.0)) {
            Ok(result) => {
                self.import_cad_sketch(&sketch_core::cad_import::ConvertedSketch {
                    lines: result.lines.clone(),
                    polygons: result.polygons.clone(),
                    labels: result.labels.clone(),
                    skipped: result.skipped.clone(),
                });
                self.open_dialog(dialog::Dialog::alert(
                    &sketch_core::cad_import::describe_import(&result),
                ))
            }
            Err(reason) => self.open_dialog(dialog::Dialog::alert(&format!(
                "Could not import {name}: {reason}"
            ))),
        }
    }

    fn import_cad_sketch(&mut self, sketch: &sketch_core::cad_import::ConvertedSketch) {
        self.push_undo("Import");
        let start = self.drawing.next_id();
        for (at, value) in &sketch.labels {
            let id = self.drawing.gen_id();
            let text = crate::placed_text::PlacedText::new(id, value.clone());
            self.drawing.texts.push(text);
            let placed = self.drawing.texts.last_mut().expect("just pushed");
            placed.x = at.x;
            placed.y = at.y;
        }
        let wall_count = sketch.lines.len() as i64;
        for (index, (a, b)) in sketch.lines.iter().enumerate() {
            self.drawing.lines.push(sketch_core::model::SketchLine {
                id: start + index as i64,
                start: *a,
                end: *b,
                kind: sketch_core::model::WallType::Exterior,
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
            });
        }
        let polygon_start = start + wall_count;
        for (index, points) in sketch.polygons.iter().enumerate() {
            self.drawing.polygons.push(sketch_core::model::Polygon {
                id: polygon_start + index as i64,
                points: points.clone(),
                edges: vec![],
                kind: "UND".into(),
                name: String::new(),
                gross: 0.0,
                area: 0.0,
                factor_override: None,
                subtract_mode: "auto".into(),
                subtract_target_id: None,
                factor: 1.0,
                measure_to: "center".into(),
                auto_post: self.drawing.auto_post.clone(),
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
            });
        }
        // Edge lists join the imported areas to the imported walls (the
        // browser's importer rebuilds edges; ids renumber).
        let lines = self.drawing.lines.clone();
        for polygon in &mut self.drawing.polygons {
            if polygon.edges.len() != polygon.points.len() {
                polygon.edges = sketch_core::model::Polygon::build_edges(&polygon.points, &lines);
            }
        }
        self.selection.clear();
        self.sync_edit_tools();
    }

    /// `undo()` / `redo()`: restore a snapshot and drop the selection, as the
    /// original does (`clearSelection` after every step).
    fn undo_step(&mut self) -> Task<Message> {
        let current = self.snapshot_model("Edit");
        if let Some(snap) = self.undo.undo(current) {
            self.restore_snapshot(&snap);
            self.selection.clear();
            self.sync_edit_tools();
        }
        Task::none()
    }

    fn redo_step(&mut self) -> Task<Message> {
        let current = self.snapshot_model("Edit");
        if let Some(snap) = self.undo.redo(current) {
            self.restore_snapshot(&snap);
            self.selection.clear();
            self.sync_edit_tools();
        }
        Task::none()
    }

    /// The text group's values (`syncContextualControls`): a selected label
    /// wins; otherwise the area whose label / size block is selected, with
    /// the auto-post font and size standing in for what it has not set.
    fn text_tools(&self) -> Option<ribbon::TextTools> {
        let color_of = |hex: &str, fallback: u32| {
            crate::render::parse_hex(hex).unwrap_or(theme::hex(fallback))
        };
        let size_text = |n: f64| {
            if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                n.to_string()
            }
        };
        if let Some(t) = self
            .drawing
            .texts
            .iter()
            .find(|t| self.selection.texts.contains(&t.id))
        {
            return Some(ribbon::TextTools {
                font: if t.font_family.is_empty() {
                    "Arial".into()
                } else {
                    t.font_family.clone()
                },
                size: size_text(if t.font_size != 0.0 {
                    t.font_size
                } else {
                    12.0
                }),
                color: color_of(
                    if t.color.is_empty() {
                        "#e2e8f0"
                    } else {
                        &t.color
                    },
                    0xE2E8F0,
                ),
                value: t.text.clone(),
            });
        }
        let poly = self.drawing.polygons.iter().find(|p| {
            self.selection.area_labels.contains(&p.id) || self.selection.area_sizes.contains(&p.id)
        })?;
        let opts = &self.drawing.options;
        let size_only = self.selection.area_sizes.contains(&poly.id)
            && !self.selection.area_labels.contains(&poly.id);
        let value = if size_only {
            let net = self.computed.get(&poly.id).map_or(poly.area, |c| c.net);
            format!("{net:.1} {}", poly.auto_post.suffix)
        } else if poly.name.is_empty() {
            poly.kind.clone()
        } else {
            poly.name.clone()
        };
        Some(ribbon::TextTools {
            font: poly
                .label_font_family
                .clone()
                .filter(|f| !f.is_empty())
                .or_else(|| Some(opts.auto_post_font.clone()).filter(|f| !f.is_empty()))
                .unwrap_or_else(|| "Arial".into()),
            size: size_text(
                poly.label_font_size
                    .filter(|s| *s != 0.0)
                    .or(Some(opts.auto_post_font_size).filter(|s| *s != 0.0))
                    .unwrap_or(16.0),
            ),
            color: color_of(
                poly.label_color
                    .as_deref()
                    .filter(|c| !c.is_empty())
                    .unwrap_or("#475569"),
                0x475569,
            ),
            value,
        })
    }

    /// The first selected area's Edit Tools values.
    fn area_tools(&self) -> Option<ribbon::AreaTools> {
        let poly = self
            .drawing
            .polygons
            .iter()
            .find(|p| self.selection.polys.contains(&p.id))?;
        let color = |value: &Option<String>, fallback: u32| {
            value
                .as_deref()
                .and_then(crate::render::parse_hex)
                .unwrap_or(theme::hex(fallback))
        };
        Some(ribbon::AreaTools {
            pattern: match area_props::shown_pattern(poly) {
                "none" => "None",
                "diagonal" => "Diagonal",
                "crosshatch" => "Crosshatch",
                "dots" => "Dots",
                _ => "Solid",
            },
            fill: color(&poly.fill_color, 0x4299e1),
            line_style: match area_props::shown_line_style(poly) {
                "dashed" => "Dashed",
                "dotted" => "Dotted",
                _ => "Solid",
            },
            thickness: match area_props::shown_thickness(poly) as i32 {
                2 => "2 px",
                3 => "3 px",
                4 => "4 px",
                6 => "6 px",
                _ => "1 px",
            },
            line_color: color(&poly.line_color, 0x3182ce),
        })
    }

    /// The Edit Tools panel's live values (`updateAreaActionStates`): the
    /// selected wall's length to 2dp, its style and colour, and which handle
    /// a length edit moves.
    fn edit_tools_data(&self) -> Option<ribbon::EditToolsData> {
        if self.selection.is_empty() {
            return None;
        }
        let area = self.area_tools();
        // Text-only selection (labels or an area's text blocks).
        if self.selection.lines.is_empty()
            && self.selection.symbols.is_empty()
            && self.has_text_selection()
        {
            return Some(ribbon::EditToolsData {
                length_text: String::new(),
                endpoint_label: "Text",
                style_text: "",
                color: theme::hex(0x2B6CB0),
                has_symbols: false,
                hosted: false,
                has_text: true,
                has_line: false,
                text: self.text_tools().map(|mut t| {
                    // A box being typed in shows what is typed.
                    match &self.text_edit {
                        Some((ribbon::TextField::Size, v)) => t.size = v.clone(),
                        Some((ribbon::TextField::Value, v)) => t.value = v.clone(),
                        None => {}
                    }
                    t
                }),
                area,
            });
        }
        // Symbol-only selection: a minimal data block so the Edit Tools tab
        // can host the symbol flip/opening-width buttons.
        if self.selection.lines.is_empty() && !self.selection.symbols.is_empty() {
            return Some(ribbon::EditToolsData {
                length_text: String::new(),
                endpoint_label: "Symbol",
                style_text: "",
                color: theme::hex(0x2B6CB0),
                has_symbols: true,
                hosted: self
                    .drawing
                    .symbols
                    .iter()
                    .any(|s| self.selection.symbols.contains(&s.id) && s.wall_id.is_some()),
                has_text: false,
                has_line: false,
                text: None,
                area,
            });
        }
        let Some(line) = self.drawing.first_selected_line(&self.selection) else {
            // An area-only selection: just the area group.
            return area.map(|area| ribbon::EditToolsData {
                length_text: String::new(),
                endpoint_label: "",
                style_text: "",
                color: theme::hex(0x2B6CB0),
                has_symbols: false,
                hosted: false,
                has_text: false,
                has_line: false,
                text: None,
                area: Some(area),
            });
        };
        let at_start = self
            .selection
            .active_endpoint
            .filter(|(id, _)| *id == line.id)
            .map(|(_, start)| start)
            .unwrap_or(false);
        let length = sketch_core::geometry::distance(line.start, line.end);
        Some(ribbon::EditToolsData {
            length_text: format!("{length:.2}"),
            endpoint_label: if at_start {
                "Length · Start handle"
            } else {
                "Length · End handle"
            },
            style_text: match line.line_style.as_deref() {
                Some("dashed") => "Dashed",
                Some("dotted") => "Dotted",
                _ => "Solid",
            },
            color: line
                .color
                .as_deref()
                .and_then(crate::render::parse_hex)
                .unwrap_or(theme::hex(0x2B6CB0)),
            has_symbols: false,
            hosted: false,
            has_text: false,
            has_line: true,
            text: None,
            area,
        })
    }

    /// The Edit Tools tab exists only with a selection; selecting switches to
    /// it and clearing the selection returns to the tab that was showing.
    fn sync_edit_tools(&mut self) {
        self.text_edit = None;
        let has = !self.selection.is_empty();
        if has && self.active_tab != ribbon::TabId::EditTools {
            self.tab_before_edit_tools = Some((self.active_tab, self.body));
            self.active_tab = ribbon::TabId::EditTools;
            self.body = ribbon::Body::EditTools;
        } else if !has && self.active_tab == ribbon::TabId::EditTools {
            let (tab, body) = self
                .tab_before_edit_tools
                .take()
                .unwrap_or((ribbon::TabId::Home, ribbon::Body::Home));
            self.active_tab = tab;
            self.body = body;
        }
    }

    /// The Color dialog: OK hands back the hex, which becomes
    /// `state.drawColor` with the original's toast (ui-manager.js:1865).
    fn color_msg(&mut self, m: color_dialog::ColorMsg) -> Task<Message> {
        if let Some(hex) = self.color.update(m) {
            match self.color_target {
                color_dialog::ColorTarget::DrawColor => {
                    self.drawing.draw_color = hex;
                    return self.toasts.show("Drawing color updated.".to_string());
                }
                color_dialog::ColorTarget::SelectedLines => {
                    self.push_undo("Wall Appearance");
                    self.drawing
                        .set_selected_line_appearance(&self.selection, None, Some(&hex));
                    self.color_target = color_dialog::ColorTarget::DrawColor;
                }
                color_dialog::ColorTarget::SelectedText => {
                    self.set_selected_text_style("color", hex);
                    self.color_target = color_dialog::ColorTarget::DrawColor;
                }
                color_dialog::ColorTarget::AreaFill | color_dialog::ColorTarget::AreaLine => {
                    let style = if self.color_target == color_dialog::ColorTarget::AreaFill {
                        area_props::AreaStyle::FillColor(hex)
                    } else {
                        area_props::AreaStyle::LineColor(hex)
                    };
                    self.push_undo("Area Style");
                    area_props::set_selected_area_style(&mut self.drawing, &self.selection, &style);
                    self.color_target = color_dialog::ColorTarget::DrawColor;
                    self.sync_canvas();
                }
            }
        }
        Task::none()
    }

    /// `makeNegative`: flips the sign of the selected areas' factor (a
    /// negative `factorOverride` subtracts from the totals). Any area still
    /// positive makes them all negative; otherwise they turn positive.
    fn make_negative(&mut self) -> Task<Message> {
        let ids: Vec<i64> = self
            .drawing
            .polygons
            .iter()
            .filter(|p| self.selection.polys.contains(&p.id))
            .map(|p| p.id)
            .collect();
        if ids.is_empty() {
            return self
                .toasts
                .show("Select an area to make negative.".to_string());
        }
        self.push_undo("Make Negative");
        let value = |p: &sketch_core::model::Polygon| p.factor_override.unwrap_or(p.factor);
        let negative = self
            .drawing
            .polygons
            .iter()
            .filter(|p| ids.contains(&p.id))
            .any(|p| value(p) >= 0.0);
        for poly in self
            .drawing
            .polygons
            .iter_mut()
            .filter(|p| ids.contains(&p.id))
        {
            let magnitude = match value(poly).abs() {
                m if m == 0.0 || !m.is_finite() => 1.0,
                m => m,
            };
            let signed = if negative { -magnitude } else { magnitude };
            poly.factor_override = Some(signed);
            poly.factor = signed;
        }
        self.toasts.show(format!(
            "Area{} {}.",
            if ids.len() == 1 { "" } else { "s" },
            if negative {
                "made negative"
            } else {
                "made positive"
            }
        ))
    }

    /// `cloneSelectedAreas`: each selected area is copied 2 ft down and to
    /// the right with its own copies of the boundary walls; the clones become
    /// the selection.
    fn clone_selected_areas(&mut self) -> Task<Message> {
        let sources: Vec<sketch_core::model::Polygon> = self
            .drawing
            .polygons
            .iter()
            .filter(|p| self.selection.polys.contains(&p.id))
            .cloned()
            .collect();
        if sources.is_empty() {
            return self
                .toasts
                .show("Select one or more areas to clone.".to_string());
        }
        self.push_undo("Clone Area");
        self.selection.clear();
        let id_map: std::collections::HashMap<i64, i64> = sources
            .iter()
            .map(|p| (p.id, self.drawing.gen_id()))
            .collect();
        for poly in &sources {
            let points: Vec<sketch_core::geometry::Pt> = poly
                .points
                .iter()
                .map(|p| sketch_core::geometry::Pt::new(p.x + 2.0, p.y + 2.0))
                .collect();
            let mut edges = Vec::with_capacity(points.len());
            for i in 0..points.len() {
                let source_edge = poly.edges.get(i).cloned().unwrap_or_default();
                let source_line = source_edge
                    .line_id
                    .and_then(|id| self.drawing.lines.iter().find(|l| l.id == id))
                    .cloned();
                let id = self.drawing.gen_id();
                let (start, end) = (points[i], points[(i + 1) % points.len()]);
                let mut line = source_line.unwrap_or(sketch_core::model::SketchLine {
                    id,
                    start,
                    end,
                    kind: sketch_core::model::WallType::Exterior,
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
                });
                line.id = id;
                line.start = start;
                line.end = end;
                line.bow_angle = source_edge.bow_angle;
                line.arc_height = if line.bow_angle != 0.0 {
                    let chord = sketch_core::geometry::distance(start, end);
                    chord / 2.0 * (line.bow_angle * std::f64::consts::PI / 720.0).tan()
                } else {
                    0.0
                };
                // A moved wall's pinned dimension no longer applies.
                line.dimension_position = None;
                line.dimension_anchor = None;
                self.drawing.lines.push(line);
                edges.push(sketch_core::geometry::Edge {
                    line_id: Some(id),
                    bow_angle: source_edge.bow_angle,
                });
            }
            let clone_id = id_map[&poly.id];
            let mut clone = poly.clone();
            clone.id = clone_id;
            clone.points = points;
            clone.edges = edges;
            // The blocks that sit where they were put move with the copy.
            let shift =
                |p: sketch_core::geometry::Pt| sketch_core::geometry::Pt::new(p.x + 2.0, p.y + 2.0);
            clone.label_position = clone.label_position.map(shift);
            clone.area_size_position = clone.area_size_position.map(shift);
            clone.subtract_target_id = poly
                .subtract_target_id
                .map(|t| id_map.get(&t).copied().unwrap_or(t));
            // Its own dedicated layer is assigned by the per-update sync.
            clone.layer_id = None;
            self.drawing.polygons.push(clone);
            self.selection.polys.insert(clone_id);
        }
        self.sync_edit_tools();
        Task::none()
    }

    /// The body of `deleteSelected` after its prompt: banked before the
    /// removal, as `pushUndo('Delete')` is.
    fn delete_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.push_undo("Delete");
        clipboard::delete(&mut self.drawing, &self.selection);
        // Free labels and symbols go with it.
        let selection = &self.selection;
        self.drawing
            .texts
            .retain(|t| !selection.texts.contains(&t.id));
        self.drawing
            .symbols
            .retain(|s| !selection.symbols.contains(&s.id));
        // Deleting a measurement label only hides it; the wall stays. Space
        // on the wall brings it back.
        for line in &mut self.drawing.lines {
            if self.selection.dimensions.contains(&line.id) {
                line.dimension_hidden = true;
                line.dimension_position = None;
                line.dimension_anchor = None;
            }
        }
        self.selection.clear();
        self.sync_edit_tools();
    }

    fn dialog_msg(&mut self, m: dialog::DialogMsg) -> Task<Message> {
        use dialog::{Action, DialogMsg};
        let Some(d) = self.dialog.as_mut() else {
            return Task::none();
        };
        let ok = match m {
            DialogMsg::Input(s) => {
                d.input = Some(s);
                return Task::none();
            }
            DialogMsg::Ok => true,
            DialogMsg::Discard => {
                if d.on_ok == Action::SaveAndExit {
                    // "Don't save": the work is being thrown away on
                    // purpose, so its recovery snapshot goes too.
                    self.dialog = None;
                    if let Some(path) = self.autosave.path.take() {
                        let _ = std::fs::remove_file(path);
                    }
                    return self.exit();
                }
                if d.on_ok != Action::ReturnToReport || self.apex_returning {
                    return Task::none();
                }
                self.dialog = None;
                return self.discard_host_changes();
            }
            DialogMsg::Cancel => false,
            DialogMsg::Dismiss => !d.has_cancel,
        };
        let d = self.dialog.take().expect("dialog is open");
        if !ok {
            return Task::none();
        }
        match d.on_ok {
            Action::None => {}
            Action::DeleteAreaType(code) => {
                return self.workflow_msg(area_workflow::Msg::ConfirmDelete(code))
            }
            Action::ShapeValue => {
                return self.shape_value(d.input.as_deref().unwrap_or(""));
            }
            Action::SetNorth => {
                self.pages.set_north(d.input.as_deref().unwrap_or(""));
            }
            Action::EditText(id) => {
                // `promptDialog('Text:')`: a blank or unchanged answer does
                // nothing; otherwise one `Edit Text` undo step.
                let text = d.input.clone().unwrap_or_default();
                let current = self
                    .drawing
                    .texts
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.text.clone());
                if let Some(current) = current {
                    if !text.trim().is_empty() && text != current {
                        self.push_undo("Edit Text");
                        if let Some(t) = self.drawing.texts.iter_mut().find(|t| t.id == id) {
                            t.text = text;
                        }
                        self.sync_canvas();
                    }
                }
            }
            Action::AddPage => self.add_page(),
            Action::New => {
                // `newSketch`: resetToBlank + the File menu/submenu hide.
                self.reset_to_blank();
                self.close_file_menu();
            }
            Action::DeleteSelected => self.delete_selected(),
            Action::DeletePage => {
                if self.pages.delete_current() {
                    self.load_page_data();
                    self.page_loaded();
                }
            }
            // `exitApp`: window.close(); the browser's fallback toast does
            // not apply to a desktop window.
            Action::OpenDropped(path) => return self.open_requested(path),
            // Save, then close only if the save really happened (the file
            // picker can be cancelled, the write can fail).
            Action::SaveAndExit => {
                let task = self.save_to_file(false);
                self.refresh_dirty();
                if !self.dirty {
                    return self.exit();
                }
                return task;
            }
            Action::ReturnToReport => return self.return_to_host(),
            Action::PhotoRemove => {
                self.bg_image = reference_image::BgImage::default();
            }
            Action::PhotoScale => {
                // handleScaleClick's tail: an invalid measure cancels the
                // mode silently; a valid one rescales and clears.
                let feet = d
                    .input
                    .as_deref()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|v| *v > 0.0);
                match (feet, self.bg_image.scale_p1, self.bg_image.scale_p2) {
                    (Some(known), Some(p1), Some(p2)) => {
                        self.bg_image.apply_known_distance(p1, p2, known);
                        self.bg_image.scale_mode = false;
                        self.bg_image.scale_p1 = None;
                        self.bg_image.scale_p2 = None;
                    }
                    _ => {
                        self.bg_image.scale_mode = false;
                        self.bg_image.scale_p1 = None;
                        self.bg_image.scale_p2 = None;
                    }
                }
            }
            Action::PhotoOrient => {
                let value = d.input.unwrap_or_default();
                let vertical = value.trim().to_lowercase().starts_with('v');
                let (Some(p1), Some(p2)) = (self.bg_image.orient_p1, self.bg_image.orient_p2)
                else {
                    return Task::none();
                };
                self.bg_image.apply_orient(p1, p2, vertical);
                self.bg_image.orient_mode = false;
                self.bg_image.orient_p1 = None;
                self.bg_image.orient_p2 = None;
                return self.toasts.show("Image orientation applied.".to_string());
            }
            Action::GeoCoordinate => {
                let value = d.input.unwrap_or_default();
                return self.update(Message::GeoCoordinateValue(value));
            }
            Action::GeoAddress => {
                let value = d.input.unwrap_or_default();
                return self.update(Message::GeoAddressValue(value));
            }
        }
        Task::none()
    }

    /// loadPageData's UI refresh: updateCalcsPanel leaves the empty areas
    /// select disabled; the selection and transient drawing state reset.
    fn page_loaded(&mut self) {
        self.workflow_close();
        self.workflow.hover = None;
        self.areas_disabled = true;
    }

    fn add_page(&mut self) {
        // addPage(): saveCurrentPage, insert after current, load the new
        // (blank) page — the live drawing empties like the original's
        // page switch.
        self.save_current_page();
        self.pages.add_page();
        self.load_page_data();
        self.page_loaded();
    }

    fn go_to_page(&mut self, index: usize) {
        if self
            .apex_geo_pending
            .as_ref()
            .is_some_and(|(page, _)| *page != index)
        {
            self.apex_geo_pending = None;
        }
        self.save_current_page();
        if self.pages.go_to(index) {
            self.load_page_data();
            self.page_loaded();
        }
    }

    /// `importFromFile(file)` (sketch-app.js:5385, S11-044): merge another
    /// sketch's FIRST page into the current one at the (2,2) offset — walls
    /// and areas with fresh ids (edges' lineIds and subtractTargetId remap;
    /// containerId/subtractedFromId drop), each imported area gets its
    /// dedicated layer, labels and symbols follow with new ids. Banked as
    /// `Import`; `Imported {name}.` toasts.
    fn import_state(&mut self, model: &serde_json::Value, file_path: &str) -> Task<Message> {
        // `Array.isArray(data.pages) ? data.pages[0] : data` — a legacy
        // single-page JSON IS the page.
        let page = if let Some(pages) = model.get("pages").and_then(|v| v.as_array()) {
            pages.first().cloned()
        } else {
            Some(model.clone())
        };
        let Some(page) = page else {
            return self
                .toasts
                .show("Error importing file: no sketch geometry found.".to_string());
        };
        let Ok(imported_lines) = serde_json::from_value::<Vec<sketch_core::model::SketchLine>>(
            page.get("lines")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        ) else {
            return self
                .toasts
                .show("Error importing file: no sketch geometry found.".to_string());
        };
        let imported_polys: Vec<sketch_core::model::Polygon> = serde_json::from_value(
            page.get("polygons")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .unwrap_or_default();
        let imported_texts: Vec<crate::placed_text::PlacedText> = serde_json::from_value(
            page.get("labels")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .unwrap_or_default();
        let imported_symbols: Vec<crate::placed_symbol::PlacedSymbol> = serde_json::from_value(
            page.get("symbols")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .unwrap_or_default();

        self.push_undo("Import");
        const OFFSET: f64 = 2.0;
        let mut line_ids: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        for line in imported_lines {
            let new_id = self.drawing.gen_id();
            line_ids.insert(line.id, new_id);
            let mut copy = line;
            copy.start =
                sketch_core::geometry::Pt::new(copy.start.x + OFFSET, copy.start.y + OFFSET);
            copy.end = sketch_core::geometry::Pt::new(copy.end.x + OFFSET, copy.end.y + OFFSET);
            copy.id = new_id;
            self.drawing.lines.push(copy);
        }
        let mut poly_ids: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        for poly in &imported_polys {
            poly_ids.insert(poly.id, self.drawing.gen_id());
        }
        for poly in imported_polys {
            let new_subtract_target = poly
                .subtract_target_id
                .and_then(|t| poly_ids.get(&t).copied());
            let mut copy = poly;
            copy.points = copy
                .points
                .iter()
                .map(|p| sketch_core::geometry::Pt::new(p.x + OFFSET, p.y + OFFSET))
                .collect();
            copy.edges = copy
                .edges
                .iter()
                .map(|e| sketch_core::geometry::Edge {
                    line_id: e.line_id.and_then(|l| line_ids.get(&l).copied()),
                    ..*e
                })
                .collect();
            // The original also nulls `containerId`/`subtractedFromId`; this
            // port's Polygon carries neither (its subtraction is the
            // subtractTargetId alone).
            copy.subtract_target_id = new_subtract_target;
            copy.id = poly_ids[&copy.id];
            self.drawing.polygons.push(copy.clone());
            // `assignAreaToDedicatedLayer(importedPoly)` — the updatePanel
            // default (true) does not matter here; the sync runs per update.
            self.assign_area_to_dedicated_layer(copy.id, false);
        }
        for mut text in imported_texts {
            text.x += OFFSET;
            text.y += OFFSET;
            text.id = self.drawing.gen_id();
            self.drawing.texts.push(text);
        }
        for mut symbol in imported_symbols {
            symbol.x += OFFSET;
            symbol.y += OFFSET;
            symbol.id = self.drawing.gen_id();
            self.drawing.symbols.push(symbol);
        }
        self.sync_canvas();
        let name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sketch")
            .to_string();
        self.toasts.show(format!("Imported {name}"))
    }

    // ------------------------------------------------------------------
    // The reference image (screen 10).
    // ------------------------------------------------------------------

    /// `loadBgImage(file)`: the picker reads the image; the fresh state is
    /// visible/centered/auto-fit to ~60 feet, flips reset, the 50
    /// intensity.
    fn photo_load(&mut self) -> Task<Message> {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(&path) {
                let size = reference_image::image_size(&bytes);
                let handle = iced::widget::image::Handle::from_bytes(bytes);
                self.bg_image = match size {
                    Some((w, h)) => reference_image::BgImage::load(handle, w, h),
                    // A format the header scan can't size: load at the
                    // natural 1x1 scale assumption (the user rescales).
                    None => reference_image::BgImage::load(handle, 1, 1),
                };
            }
        }
        Task::none()
    }

    /// `resetToBlank()` (sketch-app.js:5306): drop every page and return to
    /// a blank drawing, no prompt (File > New / Ctrl+N confirm first; the
    /// suite workfiles use the bare reset). The OPTIONS survive — the
    /// original clears the entity arrays, not `state.options`.
    fn reset_to_blank(&mut self) {
        self.invalidate_canvas();
        self.workflow_close();
        self.workflow = Default::default();
        self.drawing.lines.clear();
        self.drawing.polygons.clear();
        self.drawing.reopened_areas.clear();
        self.drawing.texts.clear();
        self.drawing.symbols.clear();
        self.drawing.dimension_chains.clear();
        self.drawing.points.clear();
        self.drawing.line_ids.clear();
        self.drawing.pen_up();
        self.drawing.cursor = sketch_core::geometry::Pt::new(0.0, 0.0);
        self.drawing.set_next_id(1);
        self.undo = undo::UndoStack::default();
        self.selection.clear();
        self.subject_auto_filled = false;
        self.layers = layers::Layers::default();
        self.pages = pages::Pages::default();
        self.file_name = "Untitled".to_string();
        self.current_file = None;
        let _ = self.set_mode(view_state::Mode::Select);
        self.sync_edit_tools();
        self.autosave.path = None;
        self.autosave.recovered = None;
        self.autosave_baseline();
    }

    /// The `.tab` click handler of ui-manager.js: the clicked tab becomes
    /// `.active`; File toggles the backstage (leaving the body as is), any
    /// other tab closes it and shows its own body.
    fn click_tab(&mut self, id: TabId) {
        self.active_tab = id;
        match id.body() {
            None => {
                self.file_menu = !self.file_menu;
            }
            Some(body) => {
                self.file_menu = false;
                self.body = body;
            }
        }
    }

    /// `closeFileMenu()` — hides the menu; the File tab stays `.active`.
    fn close_file_menu(&mut self) {
        self.file_menu = false;
        // The Recent Files submenu hides with the menu (ui-manager.js:694).
        self.recent_open = false;
    }

    /// `saveCurrentPage()` — copy the live drawing into the page record
    /// (refs/sketch-app.js:4981). The page's `northAngle` lives on the
    /// record already, so only the arrays move.
    fn save_current_page(&mut self) {
        let page = &mut self.pages.pages[self.pages.current];
        page.lines = self.drawing.lines.clone();
        page.polygons = self.drawing.polygons.clone();
        page.reopened_areas = self.drawing.reopened_areas.clone();
        page.labels = self.drawing.texts.clone();
        page.symbols = self.drawing.symbols.clone();
        page.dimension_chains = self.drawing.dimension_chains.clone();
    }

    /// `loadPageData(page)` — replace the live drawing with the current
    /// page's arrays (`self.pages.current` must already point at it).
    fn load_page_data(&mut self) {
        self.invalidate_canvas();
        let page = self.pages.pages[self.pages.current].clone();
        self.drawing.lines = page.lines;
        self.drawing.polygons = page.polygons;
        self.drawing.reopened_areas = page.reopened_areas;
        self.drawing.texts = page.labels;
        self.drawing.symbols = page.symbols;
        self.drawing.dimension_chains = page.dimension_chains;
        // The measure runs are transient (screen 9), and an interrupted
        // chain's pen-down state would leave a hidden start point behind.
        self.drawing.measure_points.clear();
        self.drawing.measure_runs.clear();
        self.drawing.pen_down = false;
        self.drawing.points.clear();
        self.drawing.line_ids.clear();
        self.selection.clear();
        self.sync_edit_tools();
        self.sync_canvas();
    }

    /// The named layer id a saved entity's 1-based layer index maps to
    /// (`layerId` is a layer NAME in the original; this port's placed
    /// symbol/text records keep an index into `layers.items`). `None` =
    /// unassigned, and the key is omitted like the original's `null`.
    fn layer_save_name(&self, index: i64) -> Option<String> {
        if index <= 0 {
            return None;
        }
        self.layers
            .items
            .get((index - 1) as usize)
            .map(|l| l.id.clone())
    }

    /// A saved page's arrays, with numeric entity layer indices mapped to
    /// the named layer ids the original stores.
    fn page_to_save(&self, page: &pages::Page) -> serde_json::Value {
        let mut value = serde_json::to_value(page).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            for key in ["labels", "symbols"] {
                if let Some(list) = obj.get_mut(key).and_then(|v| v.as_array_mut()) {
                    for entity in list {
                        let index = entity.get("layerId").and_then(|v| v.as_i64()).unwrap_or(0);
                        match self.layer_save_name(index) {
                            Some(name) => {
                                entity["layerId"] = serde_json::Value::String(name);
                            }
                            None => {
                                if let Some(map) = entity.as_object_mut() {
                                    map.remove("layerId");
                                }
                            }
                        }
                    }
                }
            }
        }
        value
    }

    /// `state.textLibraryLabels`: `null` while the dock's list is the
    /// library default, the customized list otherwise (sketch-app.js:46).
    fn text_labels_save(&self) -> serde_json::Value {
        let defaults = dock::DEFAULT_TEXT_LABELS;
        let customized = self.dock.text_labels.len() != defaults.len()
            || self
                .dock
                .text_labels
                .iter()
                .zip(defaults.iter())
                .any(|(a, b)| a.as_str() != *b);
        if customized {
            serde_json::Value::Array(
                self.dock
                    .text_labels
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            )
        } else {
            serde_json::Value::Null
        }
    }

    /// Build a save model mirroring buildSaveModel (refs/sketch-app.js:5211).
    fn build_save_model(&mut self) -> serde_json::Value {
        self.apply_host_code_properties();
        // saveCurrentPage()
        self.save_current_page();
        let pages: Vec<serde_json::Value> = self
            .pages
            .pages
            .iter()
            .map(|p| self.page_to_save(p))
            .collect();
        let next_id = self.drawing.next_id();
        let options =
            serde_json::to_value(&self.drawing.options).unwrap_or_else(|_| serde_json::json!({}));
        let page_layout =
            serde_json::to_value(&self.page_layout).unwrap_or_else(|_| serde_json::json!({}));
        let settings = serde_json::json!({
            "defineFirst": self.drawing.options.define_first,
            "dimensionsEnabled": self.view.dimensions,
            "dimensionPlacement": if self.view.dimension_placement==view_state::DimensionPlacement::In {"in"}else{"out"},
            "drawColor": self.drawing.draw_color,
            "canvasTheme": canvas_theme::current().key(),
            "pageLayout": page_layout,
            "autoPost": self.drawing.auto_post,
            "sidebar": {
                "active": self.dock.active.key(),
                "open": self.dock.open,
                "pinned": self.dock.pinned,
            },
            "textLibraryLabels": self.text_labels_save(),
            "symbolFavorites": match &self.symbol_favorites {
                Some(list) => serde_json::json!(list),
                None => serde_json::Value::Null,
            },
            "options": options,
            "apexDocument": self.native_apex,
            "customSymbols": self.portable_custom_symbols(),
        });
        serde_json::json!({
            "version": "Sketch-Web",
            "areaModelVersion": 1,
            "fileName": self.file_name,
            "nextId": next_id,
            "areaCodes": area_codes_save(&self.area_codes),
            "layers": serde_json::to_value(&self.layers.items)
                .unwrap_or_else(|_| serde_json::json!([])),
            "activeLayerId": self.layers.active,
            "pages": pages,
            "subjectInfo": serde_json::to_value(&self.subject_info)
                .unwrap_or_else(|_| serde_json::json!({})),
            "subjectInfoAutoFilled": self.subject_auto_filled,
            "extendedData": self.extended_data,
            "settings": settings,
            "savedAt": iso_now(),
        })
    }

    /// The saved entity `layerId` is a layer NAME; the placed
    /// symbol/text records keep a 1-based index. Converted in place before
    /// the typed parse (lines/polygons carry the string natively). Unknown
    /// names leave the record unassigned.
    fn normalize_entity_layers(layers: &layers::Layers, data: &mut serde_json::Value) {
        let Some(pages) = data.get_mut("pages").and_then(|v| v.as_array_mut()) else {
            return;
        };
        for page in pages {
            let Some(items) = page.as_object_mut() else {
                continue;
            };
            for key in ["labels", "symbols"] {
                let Some(list) = items.get_mut(key).and_then(|v| v.as_array_mut()) else {
                    continue;
                };
                for entity in list {
                    let name = entity
                        .get("layerId")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or_else(|| {
                            entity
                                .get("layerId")
                                .and_then(|v| v.as_i64())
                                .map(|n| n.to_string())
                        });
                    let index = name
                        .and_then(|n| {
                            layers
                                .items
                                .iter()
                                .position(|l| l.id == n)
                                .map(|p| (p + 1) as i64)
                        })
                        .unwrap_or(0);
                    if let Some(map) = entity.as_object_mut() {
                        if index == 0 {
                            map.remove("layerId");
                        } else {
                            map.insert("layerId".into(), serde_json::json!(index));
                        }
                    }
                }
            }
        }
    }

    /// `ensurePageModel(page, _)`: one saved page with its one-time
    /// migrations — bow clamp + arc height, polygon edge rebuild with the
    /// factor/subtract defaults, and the hosted-opening swing migration.
    fn restore_page(&mut self, raw: &serde_json::Value) -> pages::Page {
        let mut page: pages::Page = serde_json::from_value(raw.clone()).unwrap_or_default();
        page.north_angle = pages::normalize_north(page.north_angle);
        let engine = sketch_core::geometry::GeometryEngine::new();
        for line in &mut page.lines {
            line.bow_angle = engine.clamp_bow_angle(line.bow_angle);
            if line.bow_angle != 0.0 {
                let chord = (line.end.x - line.start.x).hypot(line.end.y - line.start.y);
                line.arc_height =
                    chord / 2.0 * (line.bow_angle * std::f64::consts::PI / 720.0).tan();
            }
        }
        for poly in &mut page.polygons {
            if poly.edges.len() != poly.points.len() {
                poly.edges = sketch_core::model::Polygon::build_edges(&poly.points, &page.lines);
            }
            if poly.factor_override.is_none() && poly.factor != 1.0 {
                poly.factor_override = Some(poly.factor);
            }
            if !matches!(poly.subtract_mode.as_str(), "auto" | "always" | "never") {
                poly.subtract_mode = "auto".into();
            }
        }
        // The hosted-opening swing repair needs the finished edge lists.
        for symbol in &mut page.symbols {
            if let Some(wall_id) = symbol.wall_id {
                if let Some(host) = page.lines.iter().find(|l| l.id == wall_id) {
                    let host = host.clone();
                    symbol.migrate_saved_swing(&host, &page.polygons);
                }
            }
        }
        self.resolve_symbol_aliases(&mut page.symbols);
        page.symbols
            .retain(|s| !self.symbol_changes.deleted.contains(&s.symbol_type));
        page
    }

    /// Load a real file and bind Save to that exact path and container format.
    fn open_sketch_path(&mut self, path: std::path::PathBuf) -> Task<Message> {
        let path = match std::path::absolute(path) {
            Ok(path) => path,
            Err(e) => return self.toasts.show(format!("Could not open sketch: {e}")),
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => return self.toasts.show(format!("Could not open sketch: {e}")),
        };
        let DecodedSketch {
            model,
            format,
            notice,
        } = match decode_sketch_bytes(&bytes) {
            Ok(decoded) => decoded,
            Err(e) => {
                return self
                    .toasts
                    .show(toast_error(&format!("Could not open sketch: {e}")))
            }
        };
        if model.get("pages").and_then(|v| v.as_array()).is_none() {
            return self.toasts.show("Invalid sketch file format.".into());
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        let stem = stem.strip_suffix(".sketchy").unwrap_or(&stem).to_owned();
        let task = self.apply_model(&model, &stem);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&stem)
            .to_owned();
        self.current_file = Some(CurrentFile { path, format });
        self.record_recent(&name, &model.to_string());
        match notice {
            Some(notice) => Task::batch([task, self.toasts.show(notice)]),
            None => task,
        }
    }

    /// Save overwrites the bound file. First save and Save As choose a path.
    /// Commit the new target only after a successful atomic write.
    fn save_to_file(&mut self, save_as: bool) -> Task<Message> {
        let mut model = self.build_save_model();
        let mut fallback_toast = None;
        // A Sketchy JSON source is never overwritten: Save acts as Save As.
        let target = if !save_as {
            self.current_file
                .clone()
                .filter(|file| file.format != SketchFileFormat::SketchyJson)
        } else {
            None
        };
        let target = match target {
            Some(target) => target,
            None => {
                // Offer a JSON destination if a new SKX cannot be encoded;
                // never silently replace an existing SKX with JSON bytes.
                let ext = match sketch_core::skx::write(&model, &Default::default()) {
                    Ok(_) => "skx",
                    Err(e) => {
                        fallback_toast = Some(format!(
                            "Could not write .skx ({e}) - saved as JSON instead."
                        ));
                        "json"
                    }
                };
                let base = self.file_name.strip_suffix(".sketchy").unwrap_or(&self.file_name);
                let Some(path) = file_dialogs::save_sketch(&format!("{base}.{ext}")) else {
                    return Task::none();
                };
                let path = match std::path::absolute(path) {
                    Ok(path) => path,
                    Err(e) => return self.toasts.show(format!("Could not save sketch: {e}")),
                };
                // Writing save-model JSON over a Sketchy file would destroy it.
                let is_source = self.current_file.as_ref().is_some_and(|f| {
                    f.format == SketchFileFormat::SketchyJson && f.path == path
                });
                if is_source || is_sketchy_name(&path) {
                    return self.toasts.show(
                        "Saving as Sketchy isn't available yet - choose a .skx name.".into(),
                    );
                }
                let format = if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case("json"))
                {
                    SketchFileFormat::Json
                } else {
                    SketchFileFormat::Skx
                };
                CurrentFile { path, format }
            }
        };
        let name = target
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.file_name)
            .to_owned();
        model["fileName"] = name.clone().into();
        let bytes = match target.format {
            SketchFileFormat::Skx => {
                sketch_core::skx::write(&model, &Default::default()).map_err(|e| e.to_string())
            }
            SketchFileFormat::Json => serde_json::to_vec_pretty(&model).map_err(|e| e.to_string()),
            // Save and Save As never target a Sketchy file (see above).
            SketchFileFormat::SketchyJson => {
                Err("Saving as Sketchy isn't available yet.".to_string())
            }
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(e) => return self.toasts.show(format!("Could not save sketch: {e}")),
        };
        if let Err(e) = autosave::atomic_write(&target.path, &bytes) {
            return self.toasts.show(format!("Could not save sketch: {e}"));
        }
        let recent_name = target
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
            .to_owned();
        let json_fallback = target.format == SketchFileFormat::Json;
        self.current_file = Some(target);
        self.file_name = name;
        self.autosave_clean();
        self.record_recent(&recent_name, &model.to_string());
        self.close_file_menu();
        match fallback_toast.filter(|_| json_fallback) {
            Some(toast) => self.toasts.show(toast),
            None => Task::none(),
        }
    }

    /// applySaveModel (refs/sketch-app.js:5315): layers → page
    /// normalization → nextId/areaCodes → subject/settings restore → the
    /// first page loads. `fallback_name` is the file's stem (the saved
    /// `fileName` wins when it is not "Untitled").
    /// Divergence note: the undo stack clears here — the browser keeps
    /// history across an open, but its snapshots reference the replaced
    /// page's walls, which this port's `Snapshot`-based stack cannot honor.
    fn apply_model(&mut self, model: &serde_json::Value, fallback_name: &str) -> Task<Message> {
        self.invalidate_canvas();
        self.workflow_close();
        self.workflow = Default::default();
        self.native_apex = model
            .pointer("/settings/apexDocument")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        // One working copy: layer names convert before the typed parse.
        let mut data = model.clone();
        if !data.get("pages").and_then(|v| v.as_array()).is_some() {
            return self.toasts.show("Invalid sketch file format.".to_string());
        }
        // Recovery and old cache-only recent entries have no trusted file path.
        self.current_file = None;
        if let Some(custom) = data.pointer("/settings/customSymbols").and_then(|v| {
            serde_json::from_value::<
                    std::collections::BTreeMap<String, sketch_core::symbols::Symbol>,
                >(v.clone())
                .ok()
        }) {
            self.symbol_changes.custom.extend(custom);
            self.rebuild_symbols();
        }
        self.layers
            .ensure(data.get("layers"), data.get("activeLayerId"));
        Self::normalize_entity_layers(&self.layers, &mut data);
        let mut restored: Vec<pages::Page> = vec![];
        if let Some(arr) = data
            .get("pages")
            .and_then(|v| v.as_array())
            .filter(|a| !a.is_empty())
        {
            for raw in arr {
                restored.push(self.restore_page(raw));
            }
        }
        // `Math.max(Number(data.nextId) || 1, getMaxLiveId(pages) + 1)` —
        // over the saved pages, like the original (the live drawing has not
        // been replaced yet).
        let saved_next = data.get("nextId").and_then(|v| v.as_i64()).unwrap_or(1);
        let live_from_pages =
            restored
                .iter()
                .flat_map(|p| {
                    p.lines
                        .iter()
                        .map(|l| l.id)
                        .chain(p.polygons.iter().map(|x| x.id))
                        .chain(p.reopened_areas.iter().flat_map(|r| {
                            [r.original.id, r.first_replacement_id.saturating_sub(1)]
                        }))
                        .chain(p.labels.iter().map(|x| x.id))
                        .chain(p.symbols.iter().map(|x| x.id))
                        .chain(p.dimension_chains.iter().map(|c| c.id))
                })
                .max()
                .unwrap_or(0);
        self.drawing
            .set_next_id(saved_next.max(live_from_pages + 1));
        // areaCodes: the catalogue validates its own JSON (defaults on error),
        // and the dock's copy follows `populateAreaCodes`.
        if let Some(codes_raw) = data.get("areaCodes").filter(|v| !v.is_null()) {
            match serde_json::to_string(codes_raw)
                .ok()
                .and_then(|text| sketch_core::geometry::area_codes::from_json(&text).ok())
            {
                Some(codes) => self.dock.area_codes = codes.clone(),
                None => {
                    return self
                        .toasts
                        .show("Area code table is not valid; defaults kept.".to_string())
                }
            }
            let codes = self.dock.area_codes.clone();
            self.area_codes = codes;
        }
        // A file without a table keeps the one in use (`if (data.areaCodes ...)`).
        // currentPage → 0, underlay cleared.
        self.pages.pages = std::mem::take(&mut restored);
        self.pages.current = 0;
        self.pages.underlay = None;
        let saved_name = data
            .get("fileName")
            .and_then(|v| v.as_str())
            .filter(|n| !n.is_empty() && *n != "Untitled")
            .map(str::to_string)
            .unwrap_or_else(|| fallback_name.to_string());
        self.file_name = if saved_name.is_empty() {
            "Untitled".to_string()
        } else {
            saved_name
        };
        // subjectInfo is an object; legacy string becomes generalComments.
        self.subject_info = serde_json::from_value(
            data.get("subjectInfo")
                .cloned()
                .unwrap_or(serde_json::json!({})),
        )
        .unwrap_or_else(|_| subject_info::SubjectInfo {
            general_comments: data
                .get("subjectInfo")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            ..subject_info::SubjectInfo::default()
        });
        self.subject_auto_filled = data
            .get("subjectInfoAutoFilled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.extended_data = data
            .get("extendedData")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        self.apply_settings(data.get("settings"));
        // loadPageData(pages[0]) + centerAndFit; the recalculation runs on
        // the next paint (its signature guard sees the new polygon inputs).
        self.load_page_data();
        self.undo = undo::UndoStack::default();
        self.autosave.path = None;
        self.autosave.recovered = None;
        self.autosave_baseline();
        Task::none()
    }

    /// applySaveModel's settings block. Keys this port owns update their
    /// state owners; the others land in `settings_extra` verbatim.
    fn apply_settings(&mut self, settings: Option<&serde_json::Value>) {
        let Some(s) = settings else {
            // An older sketch with no saved settings must not inherit solids.
            self.drawing.options = crate::options::Options::default();
            self.drawing.auto_post = Default::default();
            self.view.dimensions = true;
            self.view.dimension_placement = view_state::DimensionPlacement::Out;
            self.page_layout = crate::page_layout::PageLayout::default();
            self.settings_extra.clear();
            return;
        };
        if let Some(map) = s.as_object() {
            for (key, value) in map {
                match key.as_str() {
                    "defineFirst" | "drawColor" | "options" | "sidebar" | "textLibraryLabels"
                    | "pageLayout" | "symbolFavorites" | "canvasTheme" | "autoPost"
                    | "dimensionsEnabled" | "dimensionPlacement" => {} // handled below/above verbatim
                    // Unmodeled keys stay untouched verbatim.
                    _ => {
                        self.settings_extra.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        self.view.dimensions = s
            .get("dimensionsEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        self.view.dimension_placement =
            if s.get("dimensionPlacement").and_then(|v| v.as_str()) == Some("in") {
                view_state::DimensionPlacement::In
            } else {
                view_state::DimensionPlacement::Out
            };
        self.drawing.auto_post = s
            .get("autoPost")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        // `state.canvasTheme`: dark and blue are kept, anything else is light.
        canvas_theme::set(canvas_theme::CanvasTheme::from_key(
            s.get("canvasTheme").and_then(|v| v.as_str()).unwrap_or(""),
        ));
        // `loadLibraryState`: the saved favorites replace the defaults (the
        // library's unknown types filter out).
        self.symbol_favorites = s.get("symbolFavorites").filter(|v| v.is_array()).map(|v| {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str())
                        .filter(|t| matches!(&*self.symbols, Ok(ix) if ix.by_id(t).is_some()))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        });
        // `state.defineFirst` merges behind `options.defineFirst` — the save
        // wrote both from the same value.
        self.drawing.options = s
            .get("options")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_else(crate::options::Options::default);
        if let Some(df) = s.get("defineFirst").and_then(|v| v.as_bool()).filter(|_| {
            !s.get("options")
                .map(|o| o.get("defineFirst").is_some())
                .unwrap_or(false)
        }) {
            self.drawing.options.define_first = df;
        }
        // Loaded options now carry `gridEnabled`; mirror it to the view state
        // (`applyOptions`'s `state.gridEnabled = options.gridEnabled !== false`).
        self.view.grid = self.drawing.options.grid_enabled;
        // `this.state.pageLayout = this.normalizePageLayout(
        // data.settings.pageLayout)` — a garbage or legacy-null layout lands
        // on the defaults through the deserialize clamp.
        self.page_layout = s
            .get("pageLayout")
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let color = s.get("drawColor").and_then(|v| v.as_str());
        if let Some(c) = color {
            self.drawing.draw_color = c.to_string();
        }
        // `state.gridEnabled` follows the options too (grid toggle is the
        // status bar's; its option key rides the passthrough until Screen 11
        // lands — keep the current grid state instead).
        if let Some(sb) = s.get("sidebar") {
            if let Some(active) = sb
                .get("active")
                .and_then(|v| v.as_str())
                .and_then(dock::DockTab::from_key)
            {
                self.dock.active = active;
            }
            if let Some(o) = sb.get("open").and_then(|v| v.as_bool()) {
                self.dock.open = o;
            }
            if let Some(p) = sb.get("pinned").and_then(|v| v.as_bool()) {
                self.dock.pinned = p;
            }
        }
        if let Some(labels) = s.get("textLibraryLabels") {
            self.dock.text_labels = labels
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(|| {
                    dock::DEFAULT_TEXT_LABELS
                        .iter()
                        .map(|s| s.to_string())
                        .collect()
                });
        }
    }

    /// The skill's measured metres feed the same numeric entry and commit path
    /// as keyboard input. Drawing coordinates and keyboard distances are feet.
    fn disto_measurement(&mut self, meters: f64) -> Task<Message> {
        use sketch_core::disto::{buffer_text, round_distance, DistoRounding};
        if !meters.is_finite()
            || meters <= 0.0
            || !self.view.mode.is_drawing()
            || !self.drawing.pen_down
            || (self.dialog.is_some()
                || self.options_dialog.open
                || self.area_dialog.open
                || self.attr_dialog.open
                || self.color.open
                || self.def_editor.open
                || self.new_code.open
                || self.codes_editor.open
                || self.page_layout_dialog.open
                || self.subject_dialog.open
                || self.text_manager.open
                || self.symbol_manager.open
                || self.autosave_prompt
                || self.symbol_organizer.open
                || self.shortcuts.open
                || self.guide.open
                || self.breakdown.open
                || self.command_palette_open
                || self.file_menu
                || self.print_preview.open
                || self.integration_setup.open
                || self.line_length_edit.is_some()
                || self.scale_edit.is_some()
                || self.dock.layer_rename.is_some()
                || self.workflow.chooser.is_some()
                || self.workflow.advanced)
        {
            return Task::none();
        }
        let metric = self.drawing.options.units.metric;
        let value = if metric { meters } else { meters / 0.3048 };
        let rounded = round_distance(
            value,
            DistoRounding::parse(&self.drawing.options.disto_rounding),
        );
        let feet = if metric { rounded / 0.3048 } else { rounded };
        let Some(buffer) = buffer_text(feet) else {
            return Task::none();
        };
        self.drawing.buffer.clear();
        for c in buffer.chars() {
            let key = match c {
                '0' => "0",
                '1' => "1",
                '2' => "2",
                '3' => "3",
                '4' => "4",
                '5' => "5",
                '6' => "6",
                '7' => "7",
                '8' => "8",
                '9' => "9",
                '.' => ".",
                _ => continue,
            };
            let _ = self.update(Message::Key(input::KeyCmd::Buffer(key)));
        }
        if self.drawing.options.disto_drawing_mode == "continuous" {
            // Enter with the cursor at the anchor uses the previous segment's
            // heading, independent of where the mouse happens to be hovering.
            if let Some(&anchor) = self.drawing.points.last() {
                self.drawing.cursor = anchor;
            }
            return self.update(Message::Key(input::KeyCmd::Enter));
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let trace = if std::env::var_os("SKETCH_TRACE").is_some() {
            event::listen_with(|event, status, _window| {
                match &event {
                    Event::Mouse(
                        mouse::Event::ButtonPressed(_)
                        | mouse::Event::ButtonReleased(_)
                        | mouse::Event::CursorEntered
                        | mouse::Event::CursorLeft,
                    ) => eprintln!("TRACE event={event:?} status={status:?}"),
                    Event::Mouse(mouse::Event::CursorMoved { position }) if position.y < 60.0 => {
                        eprintln!("TRACE moved={position:?}")
                    }
                    _ => {}
                }
                None
            })
        } else {
            Subscription::none()
        };
        // The rename input's Escape: a focused text box swallows keys before
        // the EventTap, so the cancel rides the raw event stream (the
        // original's rename input stops propagation and handles it itself).
        let rename_cancel = if self.dock.layer_rename.is_some() {
            event::listen_with(|event, _status, _window| match &event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::LayerRenameCancel),
                _ => None,
            })
        } else {
            Subscription::none()
        };
        // An HTML5 drag that leaves the dock never comes back through a
        // widget release; resolve the armed press against the last hovered
        // zone (or abort). No-op once a widget release committed.
        let drop_guard = if self.dock.layer_drag.is_some() {
            event::listen_with(|event, _status, _window| match &event {
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::LayerReleaseAnywhere)
                }
                _ => None,
            })
        } else {
            Subscription::none()
        };
        // The backstage close rules are routed through the root `EventTap`
        // (see view()), not a subscription.
        // The palette's input keydown handler: a focused text box swallows
        // keys before the EventTap, so Escape, the arrows and Ctrl+K ride
        // the raw event stream.
        let palette_keys = if self.command_palette_open {
            event::listen_with(|event, _status, _window| match &event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    use iced::keyboard::key::Named;
                    use iced::keyboard::Key;
                    match key.as_ref() {
                        Key::Named(Named::Escape) => Some(Message::CommandPaletteClose),
                        Key::Named(Named::ArrowDown) => Some(Message::CommandPaletteMove(1)),
                        Key::Named(Named::ArrowUp) => Some(Message::CommandPaletteMove(-1)),
                        Key::Character(c) if modifiers.command() && c.eq_ignore_ascii_case("k") => {
                            Some(Message::CommandPaletteClose)
                        }
                        _ => None,
                    }
                }
                _ => None,
            })
        } else {
            Subscription::none()
        };
        let resize = window::resize_events().map(|(_, size)| Message::WindowResized(size));
        let window_events = event::listen_with(|event, _status, _window| match event {
            Event::Window(window::Event::Moved(position)) => Some(Message::WindowMoved(position)),
            Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
            _ => None,
        });
        // While a file is open here, hold its "open in this window" marker
        // so a second launch of the same file brings this window forward.
        let activation = match &self.current_file {
            Some(file) => Subscription::run_with(file.path.clone(), platform::activation_stream)
                .map(|_| Message::ActivateWindow),
            None => Subscription::none(),
        };
        let apex = if self.apex_session.is_some() {
            Subscription::run(apex_compat::ticks).map(|_| Message::ApexPoll)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            apex,
            trace,
            autosave::subscription(self.drawing.options.auto_save_enabled),
            resize,
            window_events,
            activation,
            window::close_requests().map(|_| Message::Close),
            rename_cancel,
            drop_guard,
            palette_keys,
            Subscription::run(disto::event_stream).map(Message::Disto),
        ])
    }

    /// The window has no OS frame (`decorations: false`), so it has no OS
    /// resize borders either: these invisible strips along the edges and
    /// squares in the corners stand in for them. A maximized window is not
    /// resizable, as with a native frame.
    fn resize_layer(&self) -> Element<'_, Message> {
        use iced::widget::{mouse_area, Space};
        use mouse::Interaction as I;
        use window::Direction as D;
        if self.maximized {
            return Space::new().into();
        }
        const EDGE: f32 = 4.0;
        const CORNER: f32 = 10.0;
        let zone = |w: Length, h: Length, direction: D, cursor: I| -> Element<'_, Message> {
            mouse_area(Space::new().width(w).height(h))
                .interaction(cursor)
                .on_press(Message::ResizeDrag(direction))
                .into()
        };
        let fixed = Length::Fixed;
        column![
            row![
                zone(fixed(CORNER), fixed(EDGE), D::NorthWest, I::ResizingDiagonallyDown),
                zone(Length::Fill, fixed(EDGE), D::North, I::ResizingVertically),
                zone(fixed(CORNER), fixed(EDGE), D::NorthEast, I::ResizingDiagonallyUp),
            ],
            row![
                column![
                    zone(fixed(EDGE), fixed(CORNER - EDGE), D::NorthWest, I::ResizingDiagonallyDown),
                    zone(fixed(EDGE), Length::Fill, D::West, I::ResizingHorizontally),
                    zone(fixed(EDGE), fixed(CORNER - EDGE), D::SouthWest, I::ResizingDiagonallyUp),
                ],
                Space::new().width(Length::Fill),
                column![
                    zone(fixed(EDGE), fixed(CORNER - EDGE), D::NorthEast, I::ResizingDiagonallyUp),
                    zone(fixed(EDGE), Length::Fill, D::East, I::ResizingHorizontally),
                    zone(fixed(EDGE), fixed(CORNER - EDGE), D::SouthEast, I::ResizingDiagonallyDown),
                ],
            ]
            .height(Length::Fill),
            row![
                zone(fixed(CORNER), fixed(EDGE), D::SouthWest, I::ResizingDiagonallyUp),
                zone(Length::Fill, fixed(EDGE), D::South, I::ResizingVertically),
                zone(fixed(CORNER), fixed(EDGE), D::SouthEast, I::ResizingDiagonallyDown),
            ],
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn view(&self) -> Element<'_, Message> {
        // The Calcs panel reflects the polygons the canvas is drawing.
        let calcs_model = calcs::build(&self.drawing.polygons, &self.computed);
        let columns = row![
            rail::view(self.view.mode, self.pages_nav_open),
            // 1px gap between the rail's border and the canvas (body bg).
            container(theme::rect(1.0, 1.0, theme::bg_app()))
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(theme::bg_app().into()),
                    ..container::Style::default()
                }),
            if self.pages_nav_open {
                pages_nav::view(&self.pages, self.pages_nav_hover)
            } else {
                iced::widget::Space::new().into()
            },
            // The drawing canvas (CanvasEngine): background + grid.
            iced::widget::canvas(sketch_canvas::SketchCanvas {
                cache: Some(&self.canvas_cache),
                pointer: Some(&self.canvas_pointer),
                pan: iced::Vector::new(self.view.pan.0 as f32, self.view.pan.1 as f32),
                zoom: self.view.zoom as f32,
                grid: self.view.grid,
                mode: self.view.mode,
                panning: self.view.pan_from.is_some(),
                space_pan: self.view.space_pan,
                scene: render::Scene {
                    live_pointer: None,
                    area_codes: &self.area_codes,
                    computed: &self.computed,
                    drawing: &self.drawing,
                    selection: &self.selection,
                    gates: &self.gates_cache,
                    photo: &self.bg_image,
                    ctx: self.draw_ctx(),
                    pan: self.view.pan,
                    zoom: self.view.zoom,
                    filters: self.view.filters,
                    dimensions: self.view.dimensions,
                    placement: self.view.dimension_placement,
                    symbols: &self.drawing.symbols,
                    symbol_lib: Some(self.symbols_lib()),
                    texts: &self.drawing.texts,
                    measure_points: &self.drawing.measure_points,
                    measure_runs: &self.drawing.measure_runs,
                    curve_tool: self.view.mode == view_state::Mode::Curve,
                    select_mode: self.view.mode == view_state::Mode::Select,
                    drag: self.view.drag.as_ref(),
                    drag_moved: self.drag_banked,
                    text_zoom_floor: 0.0,
                    curve_drag: match &self.view.drag {
                        Some(view_state::Drag::Curve { line_id, .. }) => {
                            Some((*line_id, self.drag_banked))
                        }
                        _ => None,
                    },
                    underlay: self
                        .pages
                        .underlay
                        .filter(|&u| u != self.pages.current)
                        .and_then(|u| self.pages.pages.get(u)),
                },
                text_preview: self
                    .dock_drag
                    .as_ref()
                    .filter(|d| d.active)
                    .and_then(|d| match &d.item {
                        dock::DragItem::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .or(self.placing_text.as_deref()),
                symbol_preview: self
                    .dock_drag
                    .as_ref()
                    .filter(|d| d.active)
                    .and_then(|d| match &d.item {
                        dock::DragItem::Symbol(id) => Some(id.as_str()),
                        _ => None,
                    })
                    .or(self.placing_symbol.as_deref()),
                symbol_drag: self
                    .dock_drag
                    .as_ref()
                    .is_some_and(|d| d.active && matches!(d.item, dock::DragItem::Symbol(_))),
                workflow: Some(&self.workflow),
                drop_hint: self
                    .dock_drag
                    .as_ref()
                    .and_then(|d| d.hover.as_ref())
                    .map(|(ring, color)| (ring.as_slice(), *color)),
                dragging: self.view.drag.is_some(),
                fence: self.view.fence.map(|(a, b)| {
                    let (ax, ay) = self.view.to_screen(a);
                    let (bx, by) = self.view.to_screen(b);
                    (
                        Point::new(ax as f32, ay as f32),
                        Point::new(bx as f32, by as f32),
                    )
                }),
            })
            .width(Length::Fill)
            .height(Length::Fill),
            dock::view(
                &self.dock,
                &self.symbols,
                &calcs_model,
                self.calcs_totals,
                &self.layers,
                &self.area_layer_filter(),
                dock::PanelContext {
                    dimensions: self.view.dimensions,
                    placement: self.view.dimension_placement,
                    favorites: self.symbol_favorites.as_deref(),
                    workflow: &self.workflow,
                    auto_post: &self.drawing.auto_post
                },
            ),
        ]
        .height(Length::Fill);
        // The resize handle straddles the canvas/dock boundary, above both.
        let workspace = stack![columns, dock::resize_handle(&self.dock)].height(Length::Fill);

        let chrome = column![
            theme::nudged(0.5, || titlebar::view(
                self.close_hover,
                self.maximized,
                self.save_state(),
                self.undo.can_undo(),
                self.undo.can_redo(),
                format!(
                    "Appraisal Sketch - {} (page {})",
                    self.file_name,
                    self.pages.current + 1
                ),
            )),
            theme::nudged(0.33, || ribbon::view(
                self.active_tab,
                self.body,
                self.pages.can_go_prev(),
                self.edit_tools_data().as_ref(),
                self.undo.can_undo(),
                self.undo.can_redo(),
                ribbon::PhotoRibbon {
                    edit_active: self.bg_image.edit_mode,
                    intensity: self.bg_image.opacity * 100.0,
                },
                self.disto.chip_label(),
            )),
            workspace,
            theme::nudged(0.5, || {
                let scale_text = self
                    .scale_edit
                    .clone()
                    .unwrap_or_else(|| view_state::js_number(self.view.drawing_scale));
                let (areas, area_selected) = self.status_areas();
                statusbar::view(
                    &self.view,
                    scale_text,
                    &statusbar::PageView {
                        pages: &self.pages,
                        totals: self.calcs_totals.unwrap_or_default(),
                        area_count: areas.len(),
                        areas: &areas,
                        area_selected,
                        areas_disabled: self.areas_disabled,
                        hud: &self.drawing.hud,
                        width: self.window_size.width,
                        scroll: self.status_scroll,
                    },
                )
            }),
        ]
        .width(Length::Fill)
        .height(Length::Fill);

        // The chrome stays the stack's first child in both states so its
        // widget state survives the backstage closing mid-click: the press
        // that closes the menu and the release that clicks a tab belong to
        // the same button (ui-manager.js lets that press through).
        let overlay: Element<'_, Message> = if self.file_menu {
            backstage::layer(
                self.recent_open,
                &self.recent,
                self.apex_session
                    .as_ref()
                    .map(|_| self.apex_target.as_str()),
            )
        } else {
            iced::widget::Space::new().into()
        };
        let toast_layer: Element<'_, Message> = match self.toasts.text() {
            Some(t) => toast::layer(t),
            None => iced::widget::Space::new().into(),
        };
        let filter_layer: Element<'_, Message> = if self.view.filter_open {
            filter::layer(&self.view.filters)
        } else {
            iced::widget::Space::new().into()
        };
        let shapes_layer: Element<'_, Message> = shapes::layer(&shapes::ShapesView {
            shapes: &self.shapes,
        })
        .unwrap_or_else(|| iced::widget::Space::new().into());
        let dialog_layer: Element<'_, Message> = match &self.dialog {
            Some(d) => dialog::layer(d),
            None => iced::widget::Space::new().into(),
        };
        let breakdown_layer: Element<'_, Message> = breakdown_dialog::layer(
            &self.breakdown,
            self.breakdown_report(),
            breakdown_dialog::preview(&self.drawing.polygons),
        )
        .unwrap_or_else(|| iced::widget::Space::new().into());
        let def_layer: Element<'_, Message> = def_editor::layer(&self.def_editor)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let new_code_layer: Element<'_, Message> = new_code_dialog::layer(&self.new_code)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let codes_editor_layer: Element<'_, Message> = codes_editor::layer(&self.codes_editor)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let attr_dialog_layer: Element<'_, Message> = attr_dialog::layer(&self.attr_dialog)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let area_dialog_layer: Element<'_, Message> =
            match area_dialog::layer(&self.area_dialog, &self.area_dialog.codes) {
                Some(l) => l,
                None => iced::widget::Space::new().into(),
            };
        let context_layer: Element<'_, Message> =
            context_menu::layer(&self.context).unwrap_or_else(|| iced::widget::Space::new().into());
        let select_layer: Element<'_, Message> = select_menu::layer(&self.select_menu)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let popup_layer: Element<'_, Message> = popup_menu::layer(
            &self.popup_menu,
            canvas_theme::current(),
            theme::interface_theme(),
            (self.window_size.width, self.window_size.height),
        )
        .unwrap_or_else(|| iced::widget::Space::new().into());
        let disto_layer: Element<'_, Message> =
            disto::layer(&self.disto).unwrap_or_else(|| iced::widget::Space::new().into());
        let history_layer: Element<'_, Message> =
            undo_history::layer(&self.history, &self.undo.labels())
                .unwrap_or_else(|| iced::widget::Space::new().into());
        let color_layer: Element<'_, Message> = if self.color.open {
            self.color.view()
        } else {
            iced::widget::Space::new().into()
        };
        let cp_layer: Element<'_, Message> = command_palette::layer(
            self.command_palette_open,
            &self.command_palette_query,
            self.command_palette_selected,
            (self.window_size.width, self.window_size.height),
        )
        .unwrap_or_else(|| iced::widget::Space::new().into());
        let touch_layer: Element<'_, Message> =
            touch_panel::layer(self.touch_panel_open, self.touch_distance)
                .unwrap_or_else(|| iced::widget::Space::new().into());
        let room_shapes_layer: Element<'_, Message> = room_shapes::layer(&self.room_shapes)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let subject_info_layer: Element<'_, Message> =
            subject_info::layer(&self.subject_dialog, self.subject_auto_filled)
                .unwrap_or_else(|| iced::widget::Space::new().into());
        let options_layer: Element<'_, Message> = options_dialog::layer(&self.options_dialog)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let page_layout_layer: Element<'_, Message> = page_layout::layer(&self.page_layout_dialog)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let text_manager_layer: Element<'_, Message> =
            text_manager::layer(&self.text_manager, &self.dock.text_labels)
                .unwrap_or_else(|| iced::widget::Space::new().into());
        let organizer_layer: Element<'_, Message> = match &*self.symbols {
            Ok(ix) => symbol_organizer::layer(&self.symbol_organizer, ix)
                .unwrap_or_else(|| iced::widget::Space::new().into()),
            Err(_) => iced::widget::Space::new().into(),
        };
        let geo_layer: Element<'_, Message> = reference_image::geo_layer(self.geo_menu.open)
            .unwrap_or_else(|| iced::widget::Space::new().into());
        let shortcuts_layer: Element<'_, Message> =
            shortcuts::layer(&self.shortcuts).unwrap_or_else(|| iced::widget::Space::new().into());
        let mini_map_layer: Element<'_, Message> =
            minimap::layer(self.mini_map.as_ref(), self.dock.width())
                .unwrap_or_else(|| iced::widget::Space::new().into());
        let print_layer: Element<'_, Message> =
            print::layer(&self.print_preview).unwrap_or_else(|| iced::widget::Space::new().into());
        let root = stack![
            chrome,
            overlay,
            filter_layer,
            shapes_layer,
            area_dialog_layer,
            attr_dialog_layer,
            codes_editor_layer,
            new_code_layer,
            def_layer,
            breakdown_layer,
            context_layer,
            select_layer,
            history_layer,
            popup_layer,
            disto_layer,
            dialog_layer,
            color_layer,
            cp_layer,
            touch_layer,
            room_shapes_layer,
            subject_info_layer,
            options_layer,
            page_layout_layer,
            text_manager_layer,
            organizer_layer,
            self.manager_layer(),
            self.recovery_layer(),
            geo_layer,
            mini_map_layer,
            shortcuts_layer,
            guide::layer(&self.guide, self.window_size)
                .unwrap_or_else(|| iced::widget::Space::new().into()),
            print_layer,
            area_workflow::drag_ghost(self.dock_drag.as_ref(), &self.area_codes, self.window_size)
                .unwrap_or_else(|| iced::widget::Space::new().into()),
            area_workflow::chooser_layer(&self.workflow, &self.area_codes, self.window_size)
                .unwrap_or_else(|| iced::widget::Space::new().into()),
            area_workflow::menu_layer(
                &self.workflow,
                &self.area_codes,
                self.selection.polys.len(),
                self.window_size
            )
            .unwrap_or_else(|| iced::widget::Space::new().into()),
            integration_setup::layer(&self.integration_setup, self.window_size)
                .unwrap_or_else(|| iced::widget::Space::new().into()),
            toast_layer,
            self.resize_layer()
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        // The tap is always the root (stable widget tree); it only reports
        // what someone is listening for: backstage close rules while the
        // File menu is open, pointer capture while the dock is resized.
        let mut tap = event_tap::EventTap::new(root).on_right_press(Message::RightPress);
        let workflow_dialog = self.workflow.chooser.is_some() && !self.integration_setup.open;
        let workflow_escape = self.dialog.is_none()
            && !self.def_editor.open
            && !self.new_code.open
            && (workflow_dialog
                || self.workflow.advanced
                || self.workflow.menu.is_some()
                || self.view.mode == view_state::Mode::DefineArea);
        let management_modal =
            self.symbol_manager.open || self.autosave_prompt || self.integration_setup.open;
        tap = tap.capture_key(move |key, mods| match key.as_ref() {
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) if workflow_dialog => Some(
                Message::AreaWorkflow(area_workflow::Msg::Focus(mods.shift())),
            ),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                if workflow_escape || management_modal =>
            {
                Some(Message::Key(input::KeyCmd::Escape))
            }
            _ => None,
        });
        if self.file_menu {
            tap = tap
                .on_press(Message::PointerDown)
                .on_escape(Message::Escape);
        }
        if self.dock.resizing {
            tap = tap
                .on_move(Message::PointerMoved)
                .on_release(Message::PointerUp);
        } else if self.dock_drag.is_some() {
            tap = tap
                .on_move(Message::DockDragMove)
                .on_release(Message::DockDragRelease);
        }
        // The dialog's capture-phase key listener: Escape dismisses (and
        // stops propagation), Enter = OK.
        let dialog_open = self.dialog.is_some();
        if dialog_open {
            tap = tap.on_escape(Message::Dialog(dialog::DialogMsg::Dismiss));
        }
        // The cheat sheet's own key listener (ui-manager.js: the overlay
        // stops Escape/F1 in the capture phase): Escape closes it. F1 is
        // handled by the key map's toggle.
        if self.shortcuts.open {
            tap = tap.on_escape(Message::Shortcuts(shortcuts::ShortcutsMsg::Close));
        }
        if self.guide.open {
            tap = tap.on_escape(Message::Guide(guide::GuideMsg::Close));
        }
        // InputController.handleKeyDown / handleKeyUp (see input.rs).
        let areas_keyboard =
            self.dock.active == dock::DockTab::Areas && !self.integration_setup.open;
        let ctx = input::KeyContext {
            drawing: self.view.mode.is_drawing(),
            dialog_open: dialog_open || workflow_dialog,
            buffer: !self.drawing.buffer.is_empty(),
            has_selection: !self.selection.is_empty(),
        };
        tap = tap
            .on_key(move |key, mods| match key.as_ref() {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab)
                    if areas_keyboard && !dialog_open =>
                {
                    Some(Message::AreaWorkflow(area_workflow::Msg::Focus(
                        mods.shift(),
                    )))
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) if workflow_dialog => {
                    Some(Message::AreaWorkflow(area_workflow::Msg::Focus(
                        mods.shift(),
                    )))
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
                    if workflow_dialog =>
                {
                    Some(Message::AreaWorkflow(area_workflow::Msg::Submit))
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) if dialog_open => {
                    Some(Message::Dialog(dialog::DialogMsg::Ok))
                }
                _ => input::key_down(key, mods, ctx).map(Message::Key),
            })
            .on_key_up(move |key, _| input::key_up(key, ctx).map(Message::Key));
        tap.into()
    }
}

/// `reorderWithinLayers(items, isSelected)` (sketch-app.js:1727): entities
/// group into consecutive layer segments by rank; within each segment the
/// selected ones slide to its front (or back), the rest keeping their order.
fn reorder_within_layers<T>(
    items: Vec<T>,
    ranks: Vec<usize>,
    is_selected: &dyn Fn(&T) -> bool,
    to_front: bool,
) -> Vec<T> {
    debug_assert_eq!(items.len(), ranks.len());
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    let mut seg: Vec<T> = Vec::new();
    let mut current: Option<usize> = None;
    for (r, item) in ranks.into_iter().zip(items) {
        match current {
            Some(c) if c != r => {
                flush_segment(&mut seg, &mut out, is_selected, to_front);
                current = Some(r);
            }
            None => current = Some(r),
            _ => {}
        }
        seg.push(item);
    }
    flush_segment(&mut seg, &mut out, is_selected, to_front);
    out
}

/// One segment's stable partition (the original's spread expression).
fn flush_segment<T>(
    seg: &mut Vec<T>,
    out: &mut Vec<T>,
    is_selected: &dyn Fn(&T) -> bool,
    to_front: bool,
) {
    let (sel, unsel): (Vec<T>, Vec<T>) = seg.drain(..).partition(|item| is_selected(item));
    if to_front {
        out.extend(unsel);
        out.extend(sel);
    } else {
        out.extend(sel);
        out.extend(unsel);
    }
}

/// The primary monitor's DPI scale (1.0 = 96 dpi).
#[cfg(windows)]
fn system_scale() -> f32 {
    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        fn GetDpiForSystem() -> u32;
    }
    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 (-4) — what winit sets
    // anyway; required before GetDpiForSystem reports the real DPI.
    // SAFETY: plain Win32 calls with no pointers; failure is harmless.
    let dpi = unsafe {
        SetProcessDpiAwarenessContext(-4);
        GetDpiForSystem()
    };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

#[cfg(not(windows))]
fn system_scale() -> f32 {
    1.0
}

/// The application exactly as `main()` runs it — shared with the headless
/// end-to-end tests (`iced_test::Emulator`) so they exercise the real program
/// (view, update, subscriptions, window settings), not a test double.
fn app_icon() -> window::Icon {
    let image = image::load_from_memory(include_bytes!("../../../assets/app-icon.png"))
        .expect("embedded app icon must be a valid PNG")
        .to_rgba8();
    let (width, height) = image.dimensions();
    window::icon::from_rgba(image.into_raw(), width, height)
        .expect("app icon dimensions must match its pixels")
}

fn program(
    ui_scale: f32,
) -> iced::Application<impl iced::Program<State = App, Message = Message, Theme = iced::Theme>> {
    // Reopen where the window was left; `App::boot` reads the same file.
    let placement = window_state::load(ui_scale, system_scale());
    iced::application(move || App::boot(ui_scale), App::update, App::view)
        // Present frames without waiting for the display refresh: with
        // vsync the wall being drawn trails the mouse cursor by a frame or
        // two. Frames are only drawn in response to input, so this does not
        // spin the GPU. `ICED_PRESENT_MODE=vsync` in the environment turns
        // it back on. Must come before the calls below, which edit settings.
        .settings(iced::Settings {
            vsync: false,
            ..iced::Settings::default()
        })
        .title("Appraisal Sketch - Untitled")
        .subscription(App::subscription)
        .scale_factor(|app: &App| app.ui_scale)
        .default_font(theme::SEGOE)
        .theme(|_: &App| iced::Theme::Dark)
        .window(window::Settings {
            size: placement.map_or(Size::new(WINDOW_W, WINDOW_H), |p| p.size()),
            maximized: placement.is_some_and(|p| p.maximized),
            // `min_size` is in OS-logical px: undo the app's own scale.
            min_size: Some(Size::new(
                window_state::MIN_W * ui_scale,
                window_state::MIN_H * ui_scale,
            )),
            decorations: false,
            exit_on_close_request: false,
            position: placement
                .and_then(|p| p.restore_position(ui_scale))
                .map_or(window::Position::Centered, window::Position::Specific),
            icon: Some(app_icon()),
            ..window::Settings::default()
        })
}

fn main() -> iced::Result {
    #[cfg(all(windows, debug_assertions))]
    if let Some(index) = std::env::args().position(|arg| arg == "--check-native-draft") {
        let args: Vec<_> = std::env::args().collect();
        let checked = (|| -> Result<(), String> {
            let session = apex_compat::Session::start_legacy(
                "file",
                std::path::Path::new(args.get(index + 1).ok_or("Missing native draft fixture")?),
            )?;
            let (mut app, _) = App::boot(1.);
            app.load_apex_context(&session.root)?;
            let original = app
                .drawing
                .reopened_areas
                .first()
                .ok_or("Missing native draft metadata")?
                .original
                .clone();
            let missing = original
                .edges
                .iter()
                .position(|edge| {
                    !app.drawing
                        .lines
                        .iter()
                        .any(|line| Some(line.id) == edge.line_id)
                })
                .ok_or("No missing boundary")?;
            let _ = app.set_mode(view_state::Mode::DrawExterior);
            let context = drawing::DrawCtx {
                snap: false,
                ..app.draw_ctx()
            };
            app.drawing.pen_down_at(original.points[missing], context);
            let before = app.drawing.lines.last().map(|line| line.id);
            app.drawing.click(
                original.points[(missing + 1) % original.points.len()],
                context,
            );
            app.finish_wall_commit(before);
            let repaired = app
                .drawing
                .polygons
                .iter()
                .find(|area| area.id == original.id)
                .ok_or("Native boundary did not restore its original area")?;
            if repaired.name != original.name
                || repaired.kind != original.kind
                || repaired.factor_override != original.factor_override
                || !app.drawing.reopened_areas.is_empty()
            {
                return Err("Native draft lost original area properties".into());
            }
            app.write_apex_return(&session.root)?;
            let receipt =
                session.wait_for("return-receipt.json", std::time::Duration::from_secs(30))?;
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(receipt).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if value["ok"] != true {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("Native repair save failed")
                    .into());
            }
            session.cancel();
            drop(app);
            drop(session);
            println!("PASS: native draft closed through real Rust drawing and saved with its name, code, factor and identity");
            Ok(())
        })();
        if let Err(error) = checked {
            eprintln!("{error}");
            apex_compat::finish_process(1);
        }
        apex_compat::finish_process(0);
    }

    #[cfg(all(windows, debug_assertions))]
    if std::env::args().any(|arg| arg == "--check-native-batch") {
        let args: Vec<_> = std::env::args().collect();
        let checked = (|| -> Result<(), String> {
            let options = apex_startup::parse(&args)?;
            if options.files.len() < 2 {
                return Err("Two native fixtures are required".into());
            }
            let mut children = apex_startup::launch_files(&options, 0, true)?;
            let mut pids = Vec::new();
            for child in &mut children {
                pids.push(child.id());
                let started = std::time::Instant::now();
                loop {
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        if !status.success() {
                            return Err(format!("Native file fixture failed: {status}"));
                        }
                        break;
                    }
                    if started.elapsed() > std::time::Duration::from_secs(30) {
                        let _ = child.kill();
                        return Err("Native file fixture timed out".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            if pids[0] == pids[1] {
                return Err("Native files shared one process".into());
            }
            println!("PASS: independent native file sessions launched by sketch EXE: {pids:?}");
            Ok(())
        })();
        if let Err(error) = checked {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }

    #[cfg(all(windows, debug_assertions))]
    if let Some(index) = std::env::args().position(|arg| arg == "--check-host-codes") {
        let args: Vec<_> = std::env::args().collect();
        let checked = (|| -> Result<(), String> {
            let path =
                apex_compat::load_host_codes(args.get(index + 1).ok_or("Missing code table URL")?)?;
            std::fs::copy(path, args.get(index + 2).ok_or("Missing output path")?)
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = checked {
            eprintln!("{error}");
            apex_compat::finish_process(1);
        }
        apex_compat::finish_process(0);
    }

    #[cfg(all(windows, debug_assertions))]
    if let Some(index) = std::env::args().position(|arg| arg == "--check-apex-legacy") {
        let args: Vec<_> = std::env::args().collect();
        let result = (|| -> Result<(), String> {
            let path = std::path::Path::new(args.get(index + 1).ok_or("Missing native fixture")?);
            let session = apex_compat::Session::start_legacy("file", path)?;
            let (mut app, _) = App::boot(1.);
            app.load_apex_context(&session.root)?;
            app.write_apex_return(&session.root)?;
            let receipt =
                session.wait_for("return-receipt.json", std::time::Duration::from_secs(30))?;
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(receipt).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if value["ok"] != true {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("Native return failed")
                    .to_owned());
            }
            println!(
                "{}",
                std::fs::read_to_string(session.root.join("legacy-runtime.json"))
                    .map_err(|e| e.to_string())?
            );
            session.cancel();
            println!("PASS: native file launch/edit/return ran in the sketch EXE.");
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    #[cfg(all(windows, debug_assertions))]
    if let Some(index) = std::env::args().position(|arg| arg == "--check-apex-live") {
        let args: Vec<_> = std::env::args().collect();
        let result = (|| -> Result<(), String> {
            let config = args.get(index + 1).ok_or("Missing configuration")?;
            let root = std::path::PathBuf::from(args.get(index + 2).ok_or("Missing test session")?);
            let fixture = args.get(index + 3).ok_or("Missing editor fixture")?;
            let session =
                apex_compat::Session::start_mode(std::path::Path::new(config), root.clone(), true)?;
            session.wait_for("context.json", std::time::Duration::from_secs(30))?;
            let (mut app, _) = App::boot(1.);
            app.load_apex_context(&root)?;
            let model: serde_json::Value =
                serde_json::from_slice(&std::fs::read(fixture).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let _ = app.apply_model(&model, "Live SDK fixture");
            let started = std::time::Instant::now();
            while !root.join("host-closed").exists() && !root.join("return-receipt.json").exists() {
                if started.elapsed() > std::time::Duration::from_secs(240) {
                    session.cancel();
                    return Err("Timed out in live SDK fixture".into());
                }
                if root.join("ui-save").exists() {
                    let _ = std::fs::remove_file(root.join("ui-save"));
                    let _ = app.save_to_host();
                }
                if app.apex_returning && root.join("save-receipt.json").exists() {
                    let _ = app.update(Message::ApexSaved(Ok(())));
                }
                let _ = std::fs::write(root.join("check-progress"), "polling");
                let _ = app.apex_poll();
                let _ = std::fs::write(root.join("check-progress"), "poll complete");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let _ = std::fs::write(root.join("check-progress"), "cancelling");
            session.cancel();
            let _ = std::fs::write(root.join("check-progress"), "cancelled");
            drop(app);
            let _ = std::fs::write(root.join("check-progress"), "app dropped");
            drop(session);
            let _ = std::fs::write(root.join("check-progress"), "session dropped");
            let _ = std::fs::write(
                root.join("check-progress"),
                "PASS: live editor and host session closed",
            );
            apex_compat::finish_process(0);
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    #[cfg(windows)]
    if let Some(enable) = std::env::args().find_map(|arg| match arg.as_str() {
        "--enable-apex-compat" => Some(true),
        "--restore-apex-launch" => Some(false),
        _ => None,
    }) {
        let interactive = !std::env::args().any(|arg| {
            matches!(
                arg.as_str(),
                "--apex-registration-elevated" | "--apex-registration-quiet"
            )
        });
        match apex_registration::configure(enable) {
            Ok(text) => {
                println!("{text}");
                if interactive {
                    rfd::MessageDialog::new()
                        .set_title("Sketch Rust - host launch setup")
                        .set_description(text)
                        .set_level(rfd::MessageLevel::Info)
                        .show();
                }
            }
            Err(error) => {
                eprintln!("{error}");
                if interactive {
                    rfd::MessageDialog::new()
                        .set_title("Sketch Rust - host launch setup")
                        .set_description(error)
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(all(windows, debug_assertions))]
    if let Some(config) = std::env::args()
        .skip_while(|a| a != "--check-apex-server")
        .nth(1)
    {
        let root = std::env::args().next_back().expect("session directory");
        let result = (|| -> Result<(), String> {
            let session = apex_compat::Session::start_in(
                std::path::Path::new(&config),
                std::path::PathBuf::from(root),
            )?;
            session.wait_for("context.json", std::time::Duration::from_secs(45))?;
            println!(
                "Embedded sketch server accepted a document in process {}",
                std::process::id()
            );
            let result =
                session.wait_for("return-receipt.json", std::time::Duration::from_secs(45));
            session.cancel();
            result.map(|_| ())
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--check-apex-inprocess") {
        match apex_compat::probe() {
            Ok(pid) => println!("Apex compatibility runs inside sketch-app.exe; process ID {pid}."),
            Err(error) => {
                eprintln!("Apex compatibility check failed: {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--check-installation") {
        let check = || -> Result<(), String> {
            let _ = app_icon();
            let library = symlib::library();
            let library = library.as_ref().as_ref().map_err(Clone::clone)?;
            if library.library.symbols.is_empty() {
                return Err("The external symbol library is empty.".into());
            }
            let (mut app, _) = App::boot(1.0);
            let _ = app.view();
            let model = app.build_save_model();
            let bytes = sketch_core::skx::write(&model, &Default::default())
                .map_err(|error| error.to_string())?;
            sketch_core::skx::read(&bytes).map_err(|error| error.to_string())?;
            println!(
                "Sketch Rust {}: OK; {} external symbols; native view; SKX round trip.",
                env!("CARGO_PKG_VERSION"),
                library.library.symbols.len()
            );
            Ok(())
        };
        if let Err(error) = check() {
            eprintln!("Installation check failed: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    // The Recent Files sidecar mirror (localStorage's stand-in) is only
    // active in the real app.
    std::env::set_var("SKETCH_PERSIST", "1");
    let system = system_scale();
    let dpr = std::env::var("SKETCH_DPR")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(system);
    let ui_scale = dpr / system;

    #[cfg(windows)]
    let integration = {
        let args: Vec<_> = std::env::args().collect();
        let launch = (|| -> Result<Option<apex_compat::Session>, String> {
            let options = apex_startup::parse(&args)?;
            if let Some(address) = &options.code_table {
                let codes = apex_compat::load_host_codes(address)?;
                std::env::set_var("SKETCH_APEX_CODES", codes);
            }
            if let Some(config) = &options.integration {
                return apex_compat::start(config).map(Some);
            }
            if let Some(mode) = &options.legacy {
                return apex_compat::Session::start_legacy(mode, std::path::Path::new(""))
                    .map(Some);
            }
            if let Some(file) = options.files.first() {
                for input in &options.files {
                    if input.exists() && !input.is_file() {
                        return Err(format!("Native sketch does not exist: {}", input.display()));
                    }
                }
                apex_startup::open_additional(&options)?;
                return apex_compat::Session::start_legacy("file", file).map(Some);
            }
            Ok(None)
        })();
        match launch {
            Ok(session) => session,
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title("Sketch Rust - native sketch")
                    .set_description(error)
                    .set_level(rfd::MessageLevel::Error)
                    .show();
                return Ok(());
            }
        }
    };
    // A sketch file on the command line (a double-click on an associated
    // file, "Open with"): each file gets its own window, as the native files
    // above do; a file some window already has open just comes forward.
    #[cfg(windows)]
    let host_session = integration.is_some();
    #[cfg(not(windows))]
    let host_session = false;
    if !host_session {
        let args: Vec<_> = std::env::args().collect();
        let mut files = apex_startup::sketch_files(&args).into_iter();
        let first = files.next();
        if let Ok(executable) = std::env::current_exe() {
            for extra in files {
                let _ = std::process::Command::new(&executable).arg(extra).spawn();
            }
        }
        if let Some(file) = first {
            if platform::activate_existing(&file) {
                return Ok(());
            }
            if let Ok(mut startup) = STARTUP_FILE.lock() {
                *startup = Some(file);
            }
        }
    }
    let result = program(ui_scale).run();
    #[cfg(windows)]
    if let Some(session) = integration {
        session.cancel();
        drop(session);
        // The hosted CLR and Windows PDF engine retain native threads. After
        // the editor and host session are closed, end the process explicitly
        // rather than returning through the mixed CRT/CLR shutdown path.
        if let Err(error) = result {
            eprintln!("{error}");
            apex_compat::finish_process(1);
        }
        apex_compat::finish_process(0);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::geometry::Pt;
    use sketch_core::model::WallType;
    use sketch_core::symbols::WallSnap;

    fn direct_save_fixture() -> (App, std::path::PathBuf) {
        let (mut app, _) = App::boot(1.);
        app.drawing.add_wall_shape(
            drawing::WallShape::Rectangle,
            WallType::Exterior,
            10.,
            Some(10.),
            None,
            Pt::new(0., 0.),
        );
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/direct-save-017");
        std::fs::create_dir_all(&dir).unwrap();
        (app, std::path::absolute(dir).unwrap())
    }

    #[test]
    #[ignore = "development timing probe; records update cost without changing the user's desktop"]
    fn pointer_motion_profile() {
        let mut rows = Vec::new();
        for rooms in [0, 10, 40] {
            let (mut app, _) = App::boot(1.);
            for i in 0..rooms {
                app.drawing.add_wall_shape(
                    drawing::WallShape::Rectangle,
                    WallType::Exterior,
                    10.,
                    Some(10.),
                    None,
                    Pt::new(i as f64 * 20., 0.),
                );
            }
            let _ = app.update(Message::Key(input::KeyCmd::DrawExterior));
            let start = std::time::Instant::now();
            for i in 0..500 {
                let _ = app.update(Message::CanvasHover((i % 900) as f32, 200.));
            }
            let row = serde_json::json!({"rooms": rooms, "moves": 500,
                "elapsedMs": start.elapsed().as_secs_f64() * 1000.});
            println!("POINTER_PROFILE {row}");
            rows.push(row);
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/pointer-performance");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("last.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn direct_save_recent_reopens_latest_disk_file_and_keeps_destination() {
        let (mut app, dir) = direct_save_fixture();
        let path = dir.join("recent-source.skx");
        let (_, requests) = file_dialogs::with_save_path(Some(path.clone()), || {
            let _ = app.update(Message::SaveFile);
        });
        assert_eq!(requests.len(), 1);
        let recent = recent::parse(&serde_json::to_string(&app.recent).unwrap());
        assert_eq!(recent[0].path.as_ref(), Some(&path));
        let mut disk = app.build_save_model();
        assert!(
            disk.get("current_file").is_none(),
            "session path stays out of portable data"
        );
        disk["pages"][0]["northAngle"] = 180.0.into();
        std::fs::write(
            &path,
            sketch_core::skx::write(&disk, &Default::default()).unwrap(),
        )
        .unwrap();
        let (mut reopened, _) = App::boot(1.);
        reopened.recent = recent;
        let _ = reopened.update(Message::RecentOpen(0));
        assert_eq!(
            reopened.pages.page().north_angle,
            180.,
            "disk is newer than cached snapshot"
        );
        assert_eq!(reopened.current_file.as_ref().unwrap().path, path);
        reopened.pages.pages[0].north_angle = 270.;
        let (_, requests) = file_dialogs::with_save_path(None, || {
            let _ = reopened.update(Message::Key(input::KeyCmd::Save));
        });
        assert!(requests.is_empty(), "Ctrl+S route must not open a picker");
        let saved = sketch_core::skx::read(&std::fs::read(path).unwrap())
            .unwrap()
            .model;
        assert_eq!(saved["pages"][0]["northAngle"], 270.0);
    }

    #[test]
    fn direct_save_recovery_forgets_previous_file_and_cancel_keeps_recovery() {
        let (mut app, dir) = direct_save_fixture();
        let original = dir.join("previous-document.skx");
        std::fs::write(&original, b"original document").unwrap();
        app.current_file = Some(CurrentFile {
            path: original.clone(),
            format: SketchFileFormat::Skx,
        });
        let model = app.build_save_model();
        let recovery = dir.join("retained-recovery.skx");
        std::fs::write(&recovery, b"recovery data").unwrap();
        app.autosave.recovery = Some((recovery.clone(), model));
        let _ = app.recover_autosave(true);
        assert!(app.current_file.is_none());
        let (_, requests) = file_dialogs::with_save_path(None, || {
            let _ = app.update(Message::SaveFile);
        });
        assert_eq!(requests, ["Recovered sketch.skx"]);
        assert_eq!(app.drawing.lines.len(), 4);
        assert_eq!(std::fs::read(original).unwrap(), b"original document");
        assert_eq!(std::fs::read(recovery).unwrap(), b"recovery data");
    }

    #[test]
    fn direct_save_io_errors_keep_target_drawing_and_recovery() {
        let (mut app, dir) = direct_save_fixture();
        let blocked = dir.join("unit-blocked.skx");
        std::fs::create_dir_all(&blocked).unwrap();
        let target = CurrentFile {
            path: blocked,
            format: SketchFileFormat::Skx,
        };
        app.current_file = Some(target.clone());
        app.file_name = "Unchanged".into();
        let recovery = dir.join("failed-save-recovery.skx");
        std::fs::write(&recovery, b"recovery").unwrap();
        app.autosave.path = Some(recovery.clone());
        let (_, requests) = file_dialogs::with_save_path(None, || {
            let _ = app.update(Message::SaveFile);
        });
        assert!(requests.is_empty());
        assert_eq!(app.current_file.as_ref(), Some(&target));
        assert_eq!(app.file_name, "Unchanged");
        assert!(app
            .toasts
            .text()
            .unwrap()
            .starts_with("Could not save sketch:"));
        assert_eq!(app.autosave.path.as_ref(), Some(&recovery));
        assert_eq!(std::fs::read(recovery).unwrap(), b"recovery");
        let invalid = dir.join("invalid.json");
        std::fs::write(&invalid, b"{}").unwrap();
        let _ = app.open_sketch_path(invalid);
        assert_eq!(app.current_file.as_ref(), Some(&target));
        assert_eq!(app.drawing.lines.len(), 4);
    }

    #[test]
    fn view_controls_center_preserves_zoom_and_geometry() {
        let (mut app, _) = App::boot(1.);
        app.drawing.add_wall_shape(
            drawing::WallShape::Rectangle,
            WallType::Exterior,
            20.,
            Some(10.),
            None,
            Pt::new(100., -50.),
        );
        app.ensure_area_layers();
        let lines = app.drawing.lines.clone();
        app.view.pan = (10000., -5000.);
        app.view.zoom = 2.;
        let _ = app.update(Message::Status(statusbar::StatusMsg::CenterSketch));
        let at = app.view.to_screen(view_state::World { x: 100., y: -50. });
        assert!((at.0 - app.view.canvas.0 / 2.).abs() < 1e-7);
        assert!((at.1 - app.view.canvas.1 / 2.).abs() < 1e-7);
        assert_eq!(app.view.zoom, 2.);
        assert_eq!(app.drawing.lines, lines);
    }

    #[test]
    fn view_controls_fit_and_shortcuts_use_live_bounds_and_canvas() {
        let (mut app, _) = App::boot(1.);
        app.view.pan = (-1000., 2000.);
        app.view.zoom = 3.;
        let before = (app.view.pan, app.view.zoom);
        let _ = app.update(Message::Status(statusbar::StatusMsg::CenterSketch));
        let _ = app.update(Message::Status(statusbar::StatusMsg::CenterFit));
        assert_eq!(
            (app.view.pan, app.view.zoom),
            before,
            "empty pages stay put"
        );
        app.drawing.add_wall_shape(
            drawing::WallShape::Rectangle,
            WallType::Exterior,
            200.,
            Some(100.),
            None,
            Pt::new(500., -250.),
        );
        for window in [Size::new(1300., 700.), Size::new(900., 1100.)] {
            let _ = app.update(Message::WindowResized(window));
            let canvas = app.view.canvas;
            let _ = app.update(Message::Key(input::KeyCmd::CenterFit));
            let at = app.view.to_screen(view_state::World { x: 500., y: -250. });
            assert!((at.0 - canvas.0 / 2.).abs() < 1e-7 && (at.1 - canvas.1 / 2.).abs() < 1e-7);
            for p in [
                view_state::World { x: 400., y: -300. },
                view_state::World { x: 600., y: -200. },
            ] {
                let (x, y) = app.view.to_screen(p);
                assert!(
                    x >= 29.999 && x <= canvas.0 - 29.999 && y >= 29.999 && y <= canvas.1 - 29.999
                );
            }
            let zoom = app.view.zoom;
            app.view.pan = (123., 456.);
            let _ = app.update(Message::Key(input::KeyCmd::CenterSketch));
            assert_eq!(app.view.zoom, zoom);
            assert_eq!(
                app.view.to_screen(view_state::World { x: 500., y: -250. }),
                at
            );
        }
    }

    #[test]
    fn view_controls_include_curved_walls_and_ignore_other_pages() {
        let (mut app, _) = App::boot(1.);
        app.drawing.add_wall_shape(
            drawing::WallShape::Rectangle,
            WallType::Exterior,
            20.,
            Some(1.),
            None,
            Pt::new(10., 0.5),
        );
        app.drawing.lines[0].bow_angle = 180.;
        let (min, max) = app.sketch_view_bounds().unwrap();
        assert!(
            max.y - min.y > 9.9,
            "arc sweep must extend beyond its endpoints"
        );
        app.add_page();
        app.pages.underlay = Some(0);
        assert!(
            app.sketch_view_bounds().is_none(),
            "ghost page is not the live sketch"
        );
        app.drawing.add_wall_shape(
            drawing::WallShape::Rectangle,
            WallType::Exterior,
            10.,
            Some(10.),
            None,
            Pt::new(-100., 80.),
        );
        let _ = app.update(Message::Status(statusbar::StatusMsg::CenterFit));
        let (x, y) = app.view.to_screen(view_state::World { x: -100., y: 80. });
        assert!(
            (x - app.view.canvas.0 / 2.).abs() < 1e-7 && (y - app.view.canvas.1 / 2.).abs() < 1e-7
        );
    }

    #[test]
    fn iso_now_looks_like_an_iso_stamp() {
        // `new Date().toISOString()` shape from the Unix epoch civil math.
        let stamp = iso_now();
        assert_eq!(stamp.len(), "2026-10-26T21:57:00.000Z".len());
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
        assert!(stamp.ends_with('Z'));
        assert!(stamp.starts_with("20"));
    }

    #[test]
    fn default_view_builds_without_missing_icons() {
        // Every icon is resolved while the view is built; a name missing
        // from the embedded table panics here instead of at launch.
        let (mut app, _) = App::boot(1.0);
        let _ = app.view();
        // ...and every ribbon body plus the backstage.
        for (id, _, _) in ribbon::TABS {
            let _ = app.update(Message::TabClicked(*id));
            let _ = app.view();
        }
    }

    #[test]
    fn tab_clicks_follow_ui_manager() {
        let (mut app, _) = App::boot(1.0);
        assert_eq!(
            (app.active_tab, app.body, app.file_menu),
            (TabId::Home, Body::Home, false)
        );
        let _ = app.update(Message::TabClicked(TabId::View));
        assert_eq!((app.active_tab, app.body), (TabId::View, Body::View));
        // File: becomes active, opens the menu, keeps the View body.
        let _ = app.update(Message::TabClicked(TabId::File));
        assert_eq!(
            (app.active_tab, app.body, app.file_menu),
            (TabId::File, Body::View, true)
        );
        // File again: closes it; File stays active.
        let _ = app.update(Message::TabClicked(TabId::File));
        assert_eq!((app.active_tab, app.file_menu), (TabId::File, false));
        // Reopen, then a tab click closes it and switches.
        let _ = app.update(Message::TabClicked(TabId::File));
        let _ = app.update(Message::TabClicked(TabId::Help));
        assert_eq!(
            (app.active_tab, app.body, app.file_menu),
            (TabId::Help, Body::Help, false)
        );
    }

    #[test]
    fn backstage_closes_on_outside_press_and_escape_only() {
        let (mut app, _) = App::boot(1.0);
        let _ = app.update(Message::TabClicked(TabId::File));
        // Inside the menu and on the File tab: stays open.
        for p in [Point::new(100.0, 300.0), Point::new(20.0, 40.0)] {
            let _ = app.update(Message::PointerDown(p));
            assert!(app.file_menu);
        }
        // On the canvas: closes; File stays active, body unchanged.
        let _ = app.update(Message::PointerDown(Point::new(600.0, 400.0)));
        assert_eq!(
            (app.file_menu, app.active_tab, app.body),
            (false, TabId::File, Body::Home)
        );
        let _ = app.update(Message::TabClicked(TabId::File));
        let _ = app.update(Message::Escape);
        assert!(!app.file_menu);
    }

    #[test]
    fn opacity_composite_matches_css() {
        // `.qat-btn:disabled` (opacity .38) #cccccc over #1e1e1e.
        let c = theme::over(theme::text_main(), 0.38, theme::bg_app());
        let to8 = |v: f32| (v * 255.0).round() as u8;
        assert_eq!((to8(c.r), to8(c.g), to8(c.b)), (96, 96, 96));
    }

    fn sketchy_example(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sketch-core/tests/data/sketchy")
            .join(format!("{name}.sketchy.json"))
    }

    #[test]
    fn opens_sketchy_json_examples() {
        for name in ["single-family", "condo-unit", "commercial-building"] {
            let (mut app, _) = App::boot(1.);
            let _ = app.open_sketch_path(sketchy_example(name));
            if let Some(text) = app.toasts.text() {
                assert!(text.starts_with("Opened. Not shown"), "{name}: {text}");
            }
            assert!(!app.pages.pages.is_empty(), "{name}: no pages");
            let file = app.current_file.as_ref().expect("bound file");
            assert_eq!(file.format, SketchFileFormat::SketchyJson);
            if name == "single-family" {
                assert!(!app.drawing.polygons.is_empty(), "house spaces on page 1");
            }
            if name == "condo-unit" {
                assert!(!app.drawing.lines.is_empty(), "condo walls");
            }
        }
    }

    /// A fresh per-test scratch directory (pid + test name), removed on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(test: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("sketch-rust-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn open_sketchy_copy(dir: &TempDir) -> (App, std::path::PathBuf, Vec<u8>) {
        let copy = dir.0.join("copy.sketchy.json");
        std::fs::copy(sketchy_example("condo-unit"), &copy).unwrap();
        let original = std::fs::read(&copy).unwrap();
        let (mut app, _) = App::boot(1.);
        let _ = app.open_sketch_path(copy.clone());
        assert!(app.current_file.is_some());
        (app, copy, original)
    }

    #[test]
    fn sketchy_json_save_does_not_overwrite() {
        let dir = TempDir::new("sketchy_save_no_overwrite");
        let (mut app, copy, original) = open_sketchy_copy(&dir);
        assert_eq!(app.save_state(), "Unsaved");
        app.file_name = "copy.sketchy".into();
        let dest = dir.0.join("saved-as.skx");
        let (_, requests) = file_dialogs::with_save_path(Some(dest.clone()), || {
            let _ = app.update(Message::Key(input::KeyCmd::Save));
        });
        assert_eq!(requests.len(), 1, "Save falls back to the Save As dialog");
        assert_eq!(requests[0], "copy.skx", "default name drops `.sketchy`");
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        assert!(dest.is_file());
        assert_eq!(app.current_file.as_ref().unwrap().format, SketchFileFormat::Skx);
        // Save now writes the bound .skx without prompting.
        let (_, requests) = file_dialogs::with_save_path(None, || {
            let _ = app.update(Message::Key(input::KeyCmd::Save));
        });
        assert!(requests.is_empty());
        assert_eq!(std::fs::read(&copy).unwrap(), original);
    }

    #[test]
    fn save_as_to_the_sketchy_source_is_refused() {
        let dir = TempDir::new("sketchy_save_as_source");
        let (mut app, copy, original) = open_sketchy_copy(&dir);
        let (_, _) = file_dialogs::with_save_path(Some(copy.clone()), || {
            let _ = app.save_to_file(true);
        });
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        assert!(app.toasts.text().unwrap().contains("Saving as Sketchy"));
        assert_eq!(
            app.current_file.as_ref().unwrap().format,
            SketchFileFormat::SketchyJson
        );
        // Any `.sketchy.json` name is refused, even a new file.
        let other = dir.0.join("other.sketchy.json");
        let (_, _) = file_dialogs::with_save_path(Some(other.clone()), || {
            let _ = app.save_to_file(true);
        });
        assert!(!other.exists());
    }

    #[test]
    fn cancelled_save_as_keeps_the_sketchy_binding() {
        let dir = TempDir::new("sketchy_save_as_cancel");
        let (mut app, copy, original) = open_sketchy_copy(&dir);
        let (_, requests) = file_dialogs::with_save_path(None, || {
            let _ = app.save_to_file(true);
        });
        assert_eq!(requests.len(), 1);
        assert_eq!(
            app.current_file.as_ref().unwrap().format,
            SketchFileFormat::SketchyJson
        );
        assert_eq!(std::fs::read(&copy).unwrap(), original);
    }

    #[test]
    fn garbage_sketchy_json_shows_error() {
        let dir = TempDir::new("sketchy_garbage");
        let (mut app, _) = App::boot(1.);
        let before = app.current_file.clone();
        let bad = dir.0.join("garbage.sketchy.json");
        std::fs::write(&bad, br#"{"format":"sketchy","format_version":"9.0"}"#).unwrap();
        let _ = app.open_sketch_path(bad);
        assert!(app
            .toasts
            .text()
            .unwrap()
            .starts_with("Could not open sketch:"));
        assert_eq!(app.current_file, before);
    }

    #[test]
    fn toast_errors_are_truncated() {
        assert_eq!(toast_error("short"), "short");
        assert!(toast_error(&"x".repeat(500)).chars().count() <= 203);
    }

    #[test]
    fn commercial_sketchy_open_reports_unshown_content() {
        let (mut app, _) = App::boot(1.);
        let _ = app.open_sketch_path(sketchy_example("commercial-building"));
        let text = app.toasts.text().expect("unshown-content toast");
        assert!(text.starts_with("Opened. Not shown in Sketch Rust: "), "{text}");
    }

    #[test]
    fn import_decodes_sketchy_files() {
        let bytes = std::fs::read(sketchy_example("condo-unit")).unwrap();
        let decoded = decode_sketch_bytes(&bytes).expect("decodes");
        assert_eq!(decoded.format, SketchFileFormat::SketchyJson);
        assert!(decoded.model["pages"].as_array().is_some());
    }

    #[test]
    fn rich_sketch_round_trips_through_sketchy() {
        let mut app = rich_app();
        let model = app.build_save_model();
        let doc = sketch_core::sketchy::convert::to_sketchy(&model, Some("doc-1"))
            .expect("to_sketchy");
        let back = sketch_core::sketchy::convert::from_sketchy(&doc).expect("from_sketchy");
        let (mut fresh, _) = App::boot(1.0);
        let _ = fresh.apply_model(&back, "x");
        // The fresh app loads page 1 of the model; compare against it.
        let page = &model["pages"][0];
        let count = |key: &str| page[key].as_array().map_or(0, Vec::len);
        assert_eq!(fresh.drawing.lines.len(), count("lines"));
        assert_eq!(fresh.drawing.polygons.len(), count("polygons"));
        assert_eq!(fresh.drawing.texts.len(), count("labels"));
        assert_eq!(fresh.drawing.symbols.len(), count("symbols"));
        assert_eq!(fresh.drawing.lines.len(), 1);
        assert_eq!(fresh.drawing.polygons.len(), 1);
        assert_eq!(fresh.drawing.texts.len(), 1);
        assert_eq!(fresh.drawing.symbols.len(), 1);
        assert_eq!(fresh.drawing.lines[0].id, 5);
        assert_eq!(fresh.drawing.polygons[0].id, 6);
        assert_eq!(fresh.drawing.symbols[0].id, 8);
        assert_eq!(fresh.drawing.texts[0].id, 9);
    }

    /// An app holding a rich two-page sketch (walls, area, symbol, label,
    /// subject info, layout and a dimension chain).
    fn rich_app() -> App {
        let (mut app, _) = App::boot(1.0);
        app.drawing.lines.push(sketch_core::model::SketchLine {
            id: 5,
            start: Pt::new(0.0, 0.0),
            end: Pt::new(10.0, 0.0),
            kind: WallType::Exterior,
            color: Some("#123456".into()),
            line_thickness: None,
            thickness: Some(4.0),
            dimension_hidden: false,
            layer_id: Some("layer-1".into()),
            bow_angle: 0.0,
            arc_height: 0.0,
            line_style: Some("dashed".into()),
            dimension_position: None,
            dimension_anchor: None,
        });
        app.drawing.polygons.push(sketch_core::model::Polygon {
            id: 6,
            points: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 8.0)],
            edges: vec![],
            kind: "LIV".into(),
            name: "Living".into(),
            gross: 40.0,
            area: 40.0,
            factor_override: Some(1.5),
            subtract_mode: "auto".into(),
            subtract_target_id: None,
            factor: 1.5,
            measure_to: "interior".into(),
            auto_post: Default::default(),
            attributes: [("ref".to_string(), "P-1".to_string())].into(),
            layer_id: Some("layer-1".into()),
            fill_color: None,
            fill_pattern: Some("crosshatch".into()),
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
        });
        app.drawing.symbols.push(placed_symbol::PlacedSymbol {
            id: 8,
            symbol_type: "door".into(),
            x: 5.0,
            y: 0.0,
            rotation: 90.0,
            scale: 1.0,
            flip_h: true,
            wall_id: Some(5),
            wall_snap: WallSnap::Side,
            wall_gap_width: 3.0 / 12.0,
            wall_fraction: Some(0.5),
            wall_side_flip: false,
            wall_side_explicit: true,
            layer_id: 1,
        });
        app.drawing
            .texts
            .push(placed_text::PlacedText::new(9, "Label"));
        app.file_name = "Rodney".into();
        app.subject_info.property_address = "123 Main St".into();
        app.subject_info.city = "Springfield".into();
        app.drawing.options.wall_solids = true;
        // A non-default page layout (Letter + Full Page + a caption edit).
        app.page_layout = crate::page_layout::PageLayout {
            paper_size: sketch_core::print_layout::PaperSize::Letter,
            form: crate::page_layout::Form::FullPage,
            show_page_numbers: false,
            captions: crate::page_layout::Captions {
                sketch_label: "PLAN VIEW".to_string(),
                ..crate::page_layout::Captions::default()
            },
            ..crate::page_layout::PageLayout::default()
        };
        // A dimension chain on page 0 (screen 9's per-page `dimensionChains`).
        app.drawing
            .dimension_chains
            .push(crate::chains::ChainRecord {
                id: 3,
                seed_line_id: 5,
                side: -1,
                offset: 3.0,
                show_overall: false,
                hidden_spans: vec![1],
            });
        // A second page carries its own wall.
        app.add_page();
        app.drawing.lines.push(sketch_core::model::SketchLine {
            id: 7,
            start: Pt::new(0.0, 0.0),
            end: Pt::new(0.0, -10.0),
            kind: WallType::Interior,
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
        });

        app
    }

    #[test]
    fn save_model_round_trips_a_rich_sketch() {
        // buildSaveModel ↔ applySaveModel: walls, areas, symbols, labels,
        // pages, options, the id cursor, layers and subject info survive a
        // save into a fresh app — in the original's exact JSON keys.
        let mut app = rich_app();
        let model = app.build_save_model();

        // The original's keys, off the model itself.
        assert_eq!(model["version"], "Sketch-Web");
        assert_eq!(model["areaModelVersion"], 1);
        assert_eq!(model["fileName"], "Rodney");
        assert_eq!(model["pages"].as_array().unwrap().len(), 2);
        // The named layer ids ride the placed records; the wall keeps its
        // string layer natively.
        assert_eq!(model["pages"][0]["symbols"][0]["wallSnap"], "SIDE");
        assert_eq!(model["pages"][0]["symbols"][0]["layerId"], "layer-1");
        // The chain record rides the page verbatim in the original's keys.
        assert_eq!(model["pages"][0]["dimensionChains"][0]["id"], 3);
        assert_eq!(model["pages"][0]["dimensionChains"][0]["seedLineId"], 5);
        assert_eq!(model["pages"][0]["dimensionChains"][0]["side"], -1);
        assert_eq!(
            model["pages"][0]["dimensionChains"][0]["hiddenSpans"],
            serde_json::json!([1])
        );
        assert_eq!(
            model["pages"][1]["lines"][0]["layerId"],
            serde_json::Value::Null
        );
        // textLibraryLabels stays null while the dock lists the defaults.
        assert_eq!(
            model["settings"]["textLibraryLabels"],
            serde_json::Value::Null
        );
        // The page layout rides `settings.pageLayout` in the original's keys.
        assert_eq!(model["settings"]["pageLayout"]["paperSize"], "letter");
        assert_eq!(
            model["settings"]["pageLayout"]["currentForm"],
            "full-page-letter"
        );
        assert_eq!(model["settings"]["pageLayout"]["showPageNumbers"], false);
        assert_eq!(
            model["settings"]["pageLayout"]["captions"]["sketchLabel"],
            "PLAN VIEW"
        );
        assert!(model["savedAt"].as_str().unwrap().ends_with('Z'));

        let (mut target, _) = App::boot(1.0);
        let _ = target.apply_model(&model, "FromDisk");

        // Saved name wins over the file's stem.
        assert_eq!(target.file_name, "Rodney");
        // Page 0 loaded (applySaveModel restores pages[0]).
        assert_eq!(target.drawing.lines.len(), 1);
        assert_eq!(target.drawing.lines[0].id, 5);
        assert_eq!(target.drawing.lines[0].kind, WallType::Exterior);
        assert_eq!(target.drawing.polygons[0].kind, "LIV");
        assert_eq!(target.drawing.polygons[0].factor_override, Some(1.5));
        assert_eq!(target.drawing.polygons[0].attributes["ref"], "P-1");
        assert_eq!(target.drawing.symbols[0].symbol_type, "door");
        assert!(target.drawing.symbols[0].wall_side_explicit);
        assert_eq!(target.drawing.symbols[0].layer_id, 1);
        assert_eq!(target.drawing.symbols[0].wall_snap, WallSnap::Side);
        assert_eq!(target.drawing.texts.len(), 1);
        // The page-0 chain restored with it.
        assert_eq!(target.drawing.dimension_chains.len(), 1);
        assert_eq!(target.drawing.dimension_chains[0].id, 3);
        assert_eq!(target.drawing.dimension_chains[0].seed_line_id, 5);
        assert_eq!(target.drawing.dimension_chains[0].side, -1);
        assert_eq!(target.drawing.dimension_chains[0].hidden_spans, vec![1]);
        assert!(!target.drawing.dimension_chains[0].show_overall);
        // nextId: max(1, max live id + 1) guards the id cursor on open.
        assert_eq!(target.drawing.next_id(), 10);
        // Layers, subject settings, solids option, the second page record.
        assert_eq!(target.layers.items[0].id, "layer-1");
        // The page layout restored through `normalizePageLayout`.
        assert_eq!(
            target.page_layout.paper_size,
            sketch_core::print_layout::PaperSize::Letter
        );
        assert_eq!(target.page_layout.form, crate::page_layout::Form::FullPage);
        assert!(!target.page_layout.show_page_numbers);
        // An untouched caption keeps the default.
        assert_eq!(
            target
                .page_layout
                .get_caption(crate::page_layout::Key::SummaryLabel),
            "AREA CALCULATIONS SUMMARY"
        );
        assert_eq!(target.subject_info.property_address, "123 Main St");
        assert_eq!(target.subject_info.city, "Springfield");
        assert!(target.drawing.options.wall_solids);
        assert_eq!(target.pages.pages.len(), 2);
        assert_eq!(target.pages.pages[1].lines.len(), 1);
        assert_eq!(target.pages.pages[1].lines[0].id, 7);

        // A page switch moves the content like the original's
        // saveCurrentPage/loadPageData.
        target.go_to_page(1);
        assert_eq!(target.drawing.lines.len(), 1);
        assert_eq!(target.drawing.lines[0].id, 7);
        assert!(target.drawing.texts.is_empty());
        assert!(target.drawing.dimension_chains.is_empty());
        target.go_to_page(0);
        assert_eq!(target.drawing.lines[0].id, 5);
        assert_eq!(target.drawing.texts.len(), 1);
        assert_eq!(target.drawing.dimension_chains.len(), 1);
    }

    #[test]
    fn import_state_merges_a_legacy_page_at_the_offset() {
        // S11-044's importFromFile: fresh ids, the (2,2) offset, the edge
        // remap, the dedicated layer per imported area, the Import undo.
        let (mut app, _) = App::boot(1.0);
        let model = serde_json::json!({
            "lines": [
                {"id": 1, "start": {"x": 0.0, "y": 0.0}, "end": {"x": 10.0, "y": 0.0},
                 "type": "exterior"}
            ],
            "polygons": [
                {"id": 2, "points": [{"x": 0.0, "y": 0.0}, {"x": 10.0, "y": 0.0},
                                     {"x": 10.0, "y": 8.0}],
                 "edges": [{"lineId": 1}], "type": "LIV", "name": "Living"}
            ],
            "labels": [],
            "symbols": []
        });
        let _ = app.import_state(&model, "C:\\x\\Merge.skx");
        assert_eq!(app.drawing.lines.len(), 1);
        let wall = &app.drawing.lines[0];
        assert_eq!(wall.id, 1, "a fresh gen id");
        assert_eq!((wall.start.x, wall.start.y), (2.0, 2.0), "the offset");
        // The imported area got its own dedicated layer and its wall moved
        // onto it.
        assert_eq!(app.drawing.polygons.len(), 1);
        let poly = &app.drawing.polygons[0];
        assert_eq!(poly.id, 2, "a fresh gen id after the wall");
        let lid = poly.layer_id.clone().expect("dedicated layer");
        assert!(app.layers.layer(&lid).is_some());
        assert_eq!(wall.layer_id.as_deref(), Some(lid.as_str()));
        // pushUndo('Import').
        assert_eq!(
            app.undo.labels().first().map(String::as_str),
            Some("Import")
        );
    }

    #[test]
    fn import_state_refuses_geometryless_files() {
        let (mut app, _) = App::boot(1.0);
        // A model with no pages and no lines throws 'No sketch geometry
        // found' - the port toasts instead.
        let _ = app.import_state(&serde_json::json!({"subjectInfo": ""}), "x.json");
        assert!(app.drawing.lines.is_empty());
    }
}
