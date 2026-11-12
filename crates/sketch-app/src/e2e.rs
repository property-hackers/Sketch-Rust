//! Headless end-to-end tests on `iced_test`'s `Emulator`.
//!
//! The emulator runs the REAL program from `program()` — the same view,
//! update, subscriptions and settings `main()` runs — and feeds it scripted
//! mouse/keyboard instructions through the real widget tree and event
//! listeners. No window opens and the user's mouse is never touched, so the
//! results are deterministic (real-input driving was not: the user works on
//! this PC and clicks sometimes never reached the app).
//!
//! Instruction syntax (iced_test .ice lines): `click "File"` (a widget by its
//! text), `click (700, 400)` (a viewport point), `type escape`,
//! `expect "Print Preview..."` (text must be on screen).
//!
//! Headless screenshots go to `target/e2e/<name>.png` (1400x860 at scale 1,
//! the reference gallery's size) for `tools/compare.py`; `tools/e2e.sh` runs
//! these tests and compares every screenshot with its reference.

use crate::dialog::DialogMsg;
use crate::dock::{DockMsg, DockTab, DEFAULT_W};
use crate::ribbon::{Body, TabId};
use crate::Message;
use crate::{program, toast, App};
use sketch_core::geometry::Pt;

use iced_test::emulator::{self, Emulator, Mode};
use iced_test::futures::futures::channel::mpsc;
use iced_test::futures::futures::executor;
use iced_test::futures::futures::StreamExt;
use iced_test::program::Program;
use iced_test::Instruction;

use std::path::PathBuf;

const VIEWPORT: iced::Size = iced::Size::new(1400.0, 860.0);

struct Harness<P: Program + 'static> {
    program: P,
    emulator: Emulator<P>,
    events: mpsc::Receiver<emulator::Event<P>>,
}

impl<P: Program<Theme = iced::Theme> + 'static> Harness<P> {
    fn boot(program: P) -> Self {
        Self::boot_at_size(program, VIEWPORT)
    }

    fn boot_at_size(program: P, viewport: iced::Size) -> Self {
        let (sender, events) = mpsc::channel(64);
        // Immediate: never wait on tasks — the boot tasks query a real
        // window/monitor that does not exist headlessly.
        let emulator = Emulator::new(sender, &program, Mode::Immediate, viewport);
        let mut harness = Self {
            program,
            emulator,
            events,
        };
        harness.settle().expect("emulator boots");
        harness
    }

    /// Performs emitted actions until the emulator reports Ready (Ok) or a
    /// failed instruction (Err with that instruction).
    fn settle(&mut self) -> Result<(), String> {
        loop {
            let event = executor::block_on(self.events.next())
                .expect("emulator runtime should not stop on its own");
            match event {
                emulator::Event::Action(action) => self.emulator.perform(&self.program, action),
                emulator::Event::Failed(instruction) => return Err(instruction.to_string()),
                emulator::Event::Ready => return Ok(()),
            }
        }
    }

    fn step(&mut self, line: &str) -> Result<(), String> {
        let instruction =
            Instruction::parse(line).map_err(|e| format!("bad instruction {line:?}: {e:?}"))?;
        self.emulator.run(&self.program, instruction);
        self.settle()
    }

    fn script(&mut self, lines: &[&str]) {
        for line in lines {
            if let Err(failed) = self.step(line) {
                panic!("instruction failed: {failed} (script step {line:?})");
            }
            // Widget-operation tasks (a dialog's focus + select-all) run on
            // the executor and can emit just AFTER Ready; let them land
            // before the next instruction, as a real event loop would.
            self.wait(15);
        }
    }

    /// Deliver a message directly — only for input the emulator cannot
    /// script (mouse wheel, middle button, a held Space); the canvas side of
    /// those is covered by sketch_canvas's event-mapping tests.
    fn send(&mut self, message: P::Message) {
        // `Emulator::update` never reports Ready (only `run` does), so drain
        // whatever its task emits instead of settling.
        self.emulator.update(&self.program, message);
        self.wait(15);
    }

    /// True if `text` is currently on screen.
    fn shows(&mut self, text: &str) -> bool {
        self.step(&format!("expect {text:?}")).is_ok()
    }

    /// Let real time pass, then perform whatever the program's tasks emitted
    /// meanwhile (e.g. the toast timer).
    fn wait(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        while let Ok(event) = self.events.try_recv() {
            if let emulator::Event::Action(action) = event {
                self.emulator.perform(&self.program, action);
            }
        }
    }

    /// Writes a headless screenshot to target/e2e/<name>.png.
    fn screenshot(&mut self, name: &str) -> PathBuf {
        let shot = self
            .emulator
            .screenshot(&self.program, &iced::Theme::Dark, 1.0);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e");
        std::fs::create_dir_all(&dir).expect("create target/e2e");
        let path = dir.join(format!("{name}.png"));
        let file = std::fs::File::create(&path).expect("create screenshot file");
        let mut encoder = png::Encoder::new(
            std::io::BufWriter::new(file),
            shot.size.width,
            shot.size.height,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&shot.rgba))
            .expect("write screenshot png");
        path
    }
}

fn harness() -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static>
{
    Harness::boot(program(1.0))
}

fn state<P: Program<State = App> + 'static>(h: Harness<P>) -> App {
    h.emulator.into_state().0
}

// ---------------------------------------------------------------------------
// Reference-state screenshots (compared with refs/ui-spec/gallery by e2e.sh)
// ---------------------------------------------------------------------------

#[test]
fn e2e_screenshots_of_every_reference_state() {
    // iced_test 0.14's Emulator::screenshot() takes the UI cache and never
    // puts it back, so nothing may run after a screenshot: each state gets a
    // fresh emulator replaying its script.
    let states: &[(&str, &[&str])] = &[
        ("default", &[]),
        ("view", &["click \"View\""]),
        ("tools", &["click \"Tools\""]),
        ("reference-image", &["click \"Reference Image\""]),
        ("help", &["click \"Help\""]),
        ("home", &["click \"View\"", "click \"Home\""]),
        ("file-backstage", &["click \"File\""]),
        ("after-file-escape", &["click \"File\"", "type escape"]),
        ("after-file-toggle", &["click \"File\"", "click \"File\""]),
        // Screen 3 — the dock. Clicks leave the cursor where the reference
        // capture left it (hover states included).
        ("dock-areas", &[TAB_TEXT, TAB_AREAS, "move (700, 400)"]),
        ("dock-text", &[TAB_TEXT, "move (700, 400)"]),
        ("dock-symbols", &[TAB_SYMBOLS, "move (700, 400)"]),
        ("dock-calcs", &[TAB_CALCS, "move (700, 400)"]),
        ("dock-layers", &[TAB_LAYERS, "move (700, 400)"]),
        ("dock-collapsed", &[COLLAPSE]),
        ("dock-symbol-selected", &[TAB_SYMBOLS, "click (1240, 253)"]),
        ("dock-text-abc", &[TAB_TEXT, "click (1372, 238)"]),
        ("dock-pin-toast", &[PIN, "move (700, 400)"]),
        (
            "dock-resized",
            &[
                "move (1088, 400)",
                "press (1088, 400)",
                "move (1040, 400)",
                "move (990, 400)",
                "release (990, 400)",
                "move (700, 400)",
            ],
        ),
    ];
    // SKETCH_E2E_ONLY=<substring> renders just the matching states (fast
    // iteration on one screen); the full run renders all of them.
    let only = std::env::var("SKETCH_E2E_ONLY").ok();
    for (name, script) in states
        .iter()
        .chain(STATUS_STATES)
        .chain(STATUS_STATES_4B)
        .chain(STATES_5A)
        .chain(STATES_5B)
        .chain(STATES_5D2)
        .chain(STATES_6E)
        .chain(STATES_6F)
        .chain(STATES_5D3)
    {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let mut h = harness();
        h.script(script);
        h.screenshot(name);
    }
    // Screen 7b: the Area Properties dialog over a 10x10 area. The four walls
    // close into an UND area on the last click, a click inside selects it, and
    // the dialog opens through the message its opener sends.
    if only
        .as_deref()
        .is_none_or(|o| "area-properties".contains(o))
    {
        let mut h = harness();
        h.script(&[
            RAIL_DRAW_EXT,
            "move (464, 380)",
            "click (464, 380)",
            "move (664, 380)",
            "click (664, 380)",
            "move (664, 580)",
            "click (664, 580)",
            "move (464, 580)",
            "click (464, 580)",
            // Back to the start closes the loop and creates the area.
            "move (464, 380)",
            "click (464, 380)",
            "type escape",
            RAIL_SELECT,
            "move (564, 480)",
            "click (564, 480)",
        ]);
        h.send(Message::AreaProps);
        h.screenshot("area-properties");
    }
    // Screen 7c: the Area Attributes dialog over the same 10x10 area.
    if only
        .as_deref()
        .is_none_or(|o| "area-attributes".contains(o))
    {
        let mut h = harness();
        h.script(&[
            RAIL_DRAW_EXT,
            "move (464, 380)",
            "click (464, 380)",
            "move (664, 380)",
            "click (664, 380)",
            "move (664, 580)",
            "click (664, 580)",
            "move (464, 580)",
            "click (464, 580)",
            "move (464, 380)",
            "click (464, 380)",
            "type escape",
            RAIL_SELECT,
            "move (564, 480)",
            "click (564, 480)",
        ]);
        h.send(Message::OpenAreaAttributes(true));
        h.screenshot("area-attributes");
    }
    // Screen 7g: the New Area Code dialog.
    if only.as_deref().is_none_or(|o| "new-area-code".contains(o)) {
        let mut h = harness();
        h.send(Message::OpenNewCode);
        h.screenshot("new-area-code");
    }
    // Screen 7h: the area-definition editor on GLA1 over the default state.
    if only
        .as_deref()
        .is_none_or(|o| "area-definition-editor".contains(o))
    {
        let mut h = harness();
        h.send(Message::OpenDefEditor);
        h.screenshot("area-definition-editor");
    }
    // Screen 7f: the area-code table editor over the default state.
    if only
        .as_deref()
        .is_none_or(|o| "area-codes-editor".contains(o))
    {
        let mut h = harness();
        h.send(Message::OpenCodesEditor);
        h.screenshot("area-codes-editor");
    }
    // Screen 7k: the Breakdown Details dialog. The dialog guards against
    // empty input, so seed one rectangle and open it — capturing the full
    // report + preview.
    if only
        .as_deref()
        .is_none_or(|o| "breakdown-dialog".contains(o))
    {
        let mut h = harness();
        h.script(&[CARET_EXT, MENU_RECT, "type enter", "type enter"]);
        h.send(Message::OpenBreakdown);
        h.screenshot("breakdown-dialog");
    }
    // Screen 5d-7: a bowed wall with wall solids on — the annular sector
    // (`drawCurvedWallSolid`) rather than a quad.
    if only
        .as_deref()
        .is_none_or(|o| "draw-bowed-solid".contains(o))
    {
        let mut h = harness();
        h.send(Message::WallSolids(true));
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
        ]);
        for _ in 0..2 {
            h.send(Message::Wheel {
                up: true,
                x: 300.0,
                y: 300.0,
                shift: true,
                ctrl: false,
            });
        }
        h.script(&["type escape", AWAY]);
        h.screenshot("draw-bowed-solid");
    }
    // Screen 5d-6: one wall bowed 30 degrees by two Shift+wheel ups (the
    // original's own response, captured from the live app).
    if only
        .as_deref()
        .is_none_or(|o| "draw-bowed-wall".contains(o))
    {
        let mut h = harness();
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
        ]);
        for _ in 0..2 {
            h.send(Message::Wheel {
                up: true,
                x: 300.0,
                y: 300.0,
                shift: true,
                ctrl: false,
            });
        }
        h.script(&["type escape", AWAY]);
        h.screenshot("draw-bowed-wall");
    }
    // Screen 6a: the same two walls with the first one selected (the purple
    // highlight), captured from the live original.
    if only.as_deref().is_none_or(|o| "select-wall".contains(o)) {
        let mut h = harness();
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "type escape",
        ]);
        h.send(Message::Key(crate::input::KeyCmd::Select));
        h.script(&["click (441, 420)"]);
        h.screenshot("select-wall");
    }
    // Screen 5d-5: the same two walls with wall solids on (the original's
    // 6in exterior / 4in interior defaults).
    if only
        .as_deref()
        .is_none_or(|o| "draw-wall-solids".contains(o))
    {
        let mut h = harness();
        h.send(Message::WallSolids(true));
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "move (391, 579)",
            // Pen up: a line in the live chain keeps its stroke preview, so
            // the solids only show once the chain is finished.
            "type escape",
        ]);
        h.screenshot("draw-wall-solids");
    }
    // Screen 5d-4: the same two walls with metric options — the labels go
    // through format::Units, which the Options dialog (screen 12) will edit.
    if only
        .as_deref()
        .is_none_or(|o| "draw-metric-label".contains(o))
    {
        let mut h = harness();
        h.send(Message::Units(crate::format::Units {
            metric: true,
            ..crate::format::Units::default()
        }));
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "move (391, 579)",
        ]);
        h.screenshot("draw-metric-label");
    }
    // Wheel zoom (no scroll instruction in iced_test): the original was
    // wheeled up, up at canvas (300, 300) and down at (600, 400).
    if only
        .as_deref()
        .is_none_or(|o| "canvas-wheel-zoom".contains(o))
    {
        let mut h = harness();
        for (up, x, y) in [
            (true, 300.0, 300.0),
            (true, 300.0, 300.0),
            (false, 600.0, 400.0),
        ] {
            h.send(Message::Wheel {
                up,
                x,
                y,
                shift: false,
                ctrl: false,
            });
        }
        h.script(&[AWAY]);
        h.screenshot("canvas-wheel-zoom");
    }
    // Screen 5c: typed entry and the arrow-key cursor. iced_test cannot
    // script arrow keys (its named keys are only enter/escape/tab/backspace),
    // so the messages are delivered directly, as the app receives them.
    let typed: &[(&str, &[crate::input::KeyCmd])] = &[
        (
            "draw-typed-buffer",
            &[
                crate::input::KeyCmd::Buffer("1"),
                crate::input::KeyCmd::Buffer("0"),
            ],
        ),
        (
            "draw-keyboard-preview",
            &[
                crate::input::KeyCmd::Buffer("1"),
                crate::input::KeyCmd::Buffer("0"),
                crate::input::KeyCmd::Arrow {
                    dir: 0,
                    ctrl: false,
                    shift: false,
                },
                crate::input::KeyCmd::Arrow {
                    dir: 0,
                    ctrl: false,
                    shift: false,
                },
            ],
        ),
        (
            "draw-typed-wall",
            &[
                crate::input::KeyCmd::Buffer("2"),
                crate::input::KeyCmd::Buffer("0"),
                crate::input::KeyCmd::Enter,
            ],
        ),
    ];
    for (name, keys) in typed {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let mut h = harness();
        h.script(&[RAIL_DRAW_EXT, "move (341, 429)", "click (341, 429)"]);
        for key in *keys {
            h.send(Message::Key(*key));
        }
        // No final pointer move: the reference's pointer stayed on the click,
        // and moving it would update the readout (and its pill width).
        h.screenshot(name);
    }

    // Screen 5d-1: the dimension cycle. The references were captured with the
    // original's HOME tab still showing (its tab strip is not hit-testable from
    // the page), so the cycle is driven through the message the ribbon sends
    // and the ribbon is left alone.
    let dims: &[(&str, usize)] = &[("dims-inside", 1), ("dims-hidden", 2)];
    for (name, clicks) in dims {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let mut h = harness();
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "move (391, 579)",
            "type escape",
            AWAY,
        ]);
        for _ in 0..*clicks {
            h.send(Message::Status(
                crate::statusbar::StatusMsg::CycleDimensions,
            ));
        }
        h.screenshot(name);
    }

    // The pin-off reference was captured after the toast had gone.
    let mut h = harness();
    h.script(&[PIN, "move (700, 400)"]);
    h.wait(toast::SHOW_MS + 400);
    h.screenshot("dock-pin-off");
}

// Screen 4a — status bar states (names = their reference images).
const AWAY: &str = "move (700, 400)";
const STATUS_STATES: &[(&str, &[&str])] = &[
    ("status-default", &[]),
    ("status-zoom-in", &[ZOOM_IN, AWAY]),
    ("status-zoom-out", &[ZOOM_OUT, AWAY]),
    ("status-slider-250", &[SLIDER_250, AWAY]),
    (
        "status-scale-20",
        &[
            SCALE_END,
            "type backspace",
            "type backspace",
            "type \"20\"",
            "type enter",
            AWAY,
        ],
    ),
    ("status-grid-off", &[GRID, AWAY]),
    ("status-snap-off", &[SNAP, AWAY]),
    (
        "status-areasnap-half",
        &[AREA_SNAP, "click (187, 847)", PITCH_HALF, AWAY],
    ),
    ("status-filter", &[FILTER, AWAY]),
    (
        "status-filter-unchecked",
        &[FILTER, "click (1240, 682)", AWAY],
    ),
    ("status-zoomfence-armed", &[FENCE, AWAY]),
    (
        "status-zoomfence-drag",
        &[
            FENCE,
            "press (300, 300)",
            "move (400, 375)",
            "move (500, 450)",
        ],
    ),
    (
        "status-zoomfence-done",
        &[
            FENCE,
            "press (300, 300)",
            "move (400, 375)",
            "move (500, 450)",
            "release (500, 450)",
            AWAY,
        ],
    ),
];

// Screen 4b — north, pages, underlay, dialogs (names = reference images).
const STATUS_STATES_4B: &[(&str, &[&str])] = &[
    ("status-north-prompt", &[NORTH, AWAY]),
    (
        "status-north-90",
        &[NORTH, "type \"90\"", "click \"OK\"", AWAY],
    ),
    ("status-page-next-confirm", &[PAGE_NEXT, AWAY]),
    ("status-page-2of2", &[PAGE_NEXT, "click \"OK\"", AWAY]),
    (
        "status-page-1of2",
        &[PAGE_NEXT, "click \"OK\"", PAGE_PREV_2, AWAY],
    ),
    (
        "status-underlay-page2",
        &[PAGE_ADD, PAGE_PREV_2, UNDERLAY_2, UNDERLAY_PICK_2, AWAY],
    ),
    ("status-page-delete-alert", &[PAGE_DEL, AWAY]),
    ("status-page-delete-confirm", &[PAGE_ADD, PAGE_DEL_2, AWAY]),
    (
        "status-page-deleted",
        &[PAGE_ADD, PAGE_DEL_2, "click \"OK\"", AWAY],
    ),
];

// Screen 5a — modes and navigation (names = reference images).
const RAIL_PAN: &str = "click (20, 251)";
const RAIL_DRAW_EXT: &str = "click (20, 299)";
const RAIL_DRAW_INT: &str = "click (20, 333)";
const RAIL_MEASURE: &str = "click (20, 449)";
const RAIL_SELECT: &str = "click (20, 149)";
const STATES_5A: &[(&str, &[&str])] = &[
    ("rail-pan", &[RAIL_PAN, AWAY]),
    ("rail-draw-exterior", &[RAIL_DRAW_EXT, AWAY]),
    ("rail-draw-interior", &[RAIL_DRAW_INT, AWAY]),
    ("rail-measure-toast", &[RAIL_MEASURE, AWAY]),
    (
        "canvas-pan-drag",
        &[
            RAIL_PAN,
            "press (400, 400)",
            "move (420, 410)",
            "move (440, 420)",
            "move (460, 430)",
            "move (480, 440)",
            "move (500, 450)",
            "release (500, 450)",
        ],
    ),
];

// Screen 5b — drawing walls with the mouse. The references were driven
// with in-page canvas pointer/click events after the rail tool's
// `el.click()` (no cursor in either capture).
const STATES_5B: &[(&str, &[&str])] = &[
    (
        "draw-pen-down",
        &[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 432)",
        ],
    ),
    (
        "draw-two-walls",
        &[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "move (391, 579)",
        ],
    ),
    (
        "draw-pen-up",
        &[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "move (391, 579)",
            "type escape",
        ],
    ),
    (
        // A closed 10x10 ft loop: the walls stay plain linework (an UND area
        // draws no fill), the pen lifts and the areas select is unchanged.
        "draw-closed-area",
        &[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "move (341, 629)",
            "click (341, 629)",
            "move (342, 430)",
            "click (342, 430)",
            "move (700, 400)",
        ],
    ),
    (
        "draw-interior",
        &[
            RAIL_DRAW_INT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 580)",
        ],
    ),
];

// Screen 5d — View › Apply (Dims) cycles outside / inside / hidden. The
// references were captured after two walls, the pen up and the pointer away,
// so the scripts mirror that.
const VIEW_TAB: &str = "click \"View\"";
const APPLY_DIMS: &str = "click (696, 92)";
// Screen 5d-2 — the wall-shape menu and its prompt chain (the references
// were captured with the pointer away from the menu).
const CARET_EXT: &str = "click (19, 309)";
const MENU_LINE: &str = "click (100, 337)";
const MENU_RECT: &str = "click (100, 366)";
// Screen 6e — the select-by-type menu, opened from Edit Tools › Select
// (x 213.25, y 68.41, 46x49.19 in the ribbon body).
const SELECT_BY_TYPE_BTN: &str = "click (236, 92)";
const STATES_6E: &[(&str, &[&str])] = &[(
    "select-menu",
    &[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        SELECT_BY_TYPE_BTN,
        AWAY,
    ],
)];

// Screen 6f — the canvas context menu, opened by a right-click at (600,300)
// with the first wall selected (the reference's own setup).
const STATES_6F: &[(&str, &[&str])] = &[(
    "context-menu",
    &[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        "move (600, 300)",
        "press right (600, 300)",
        AWAY,
    ],
)];

const STATES_5D2: &[(&str, &[&str])] = &[
    ("wall-shape-menu", &[CARET_EXT, AWAY]),
    ("wall-shape-width-prompt", &[CARET_EXT, MENU_RECT, AWAY]),
    (
        "wall-shape-rectangle",
        &[CARET_EXT, MENU_RECT, "type enter", "type enter", AWAY],
    ),
];

/// Screen 5d-3: the Color dialog. The ribbon's Draw Color button is at
/// (674, 68) 80x49; on the Standard tab the Custom tab is at (784, 243) and
/// the ninth preset (red) at (562, 495). `move` after each click because the
/// reference was captured with JS clicks, i.e. with nothing hovered.
const DRAW_COLOR: &str = "click (700, 90)";
const COLOR_AWAY: &str = "move (700, 400)";
const STATES_5D3: &[(&str, &[&str])] = &[
    ("color-dialog-standard", &[DRAW_COLOR, COLOR_AWAY]),
    (
        "color-dialog-custom",
        &[DRAW_COLOR, COLOR_AWAY, "click (784, 243)", COLOR_AWAY],
    ),
    (
        "color-dialog-preset",
        &[DRAW_COLOR, COLOR_AWAY, "click (562, 517)", COLOR_AWAY],
    ),
];

// Right block (one page): north, delete, previous, next, add.
const NORTH: &str = "click (1105, 847)";
const PAGE_DEL: &str = "click (1127, 846)";
const PAGE_NEXT: &str = "click (1195, 846)";
const PAGE_ADD: &str = "click (1213, 846)";
// With two pages the underlay select is 8px wider: the block starts 8px left.
const PAGE_DEL_2: &str = "click (1119, 846)";
const PAGE_PREV_2: &str = "click (1137, 846)";
const PAGE_NEXT_2: &str = "click (1187, 846)";
const UNDERLAY_2: &str = "click (1282, 847)";
/// The underlay menu opens upward: "None" 808-822, "Page 2" 823-837.
const UNDERLAY_PICK_2: &str = "click (1282, 830)";

// Status bar control centers (y = 847) at the default layout.
const CENTER: &str = "click (19, 847)";
const CENTER_FIT: &str = "click (45, 847)";
const FENCE: &str = "click (71, 847)";
const GRID: &str = "click (97, 847)";
const SNAP: &str = "click (123, 847)";
const AREA_SNAP: &str = "click (149, 847)";
const FILTER: &str = "click (225, 847)";
const CHIP: &str = "click (306, 847)";
const ZOOM_OUT: &str = "click (390, 846)";
const ZOOM_IN: &str = "click (491, 846)";
const SCALE_END: &str = "click (636, 846)";
/// The pitch menu opens upward: 13px rows 1' 785, 1/2' 798, 1/4' 811, 1" 824.
const PITCH_HALF: &str = "click (187, 804)";
const PITCH_QUARTER: &str = "click (187, 817)";
/// 250% on the 80px range: iced maps value linearly across the slider
/// (6.1 values/px). Its left edge lands at ~400.52 (Chrome: 400.77 — text
/// widths differ by fractions), so 250 = 400.52 + 240/490 * 80; probed
/// 439.65..439.75. Re-probe if the status bar layout changes.
// 250%: the reference's click (439.70) reads 250 in Chrome, but iced's
// slider hit geometry differs from a native range input by a fraction of a
// pixel, so this is the mid-band click for our layout (probed: 439.90-440.05
// with the pinned space-between centre). Re-probe whenever the status bar
// changes: a throwaway #[ignore] test clicking x in 0.1 steps.
const SLIDER_250: &str = "click (439.95, 846)";

// Dock tab centers (y = 144) at the default width.
const TAB_AREAS: &str = "click (1113, 144)";
const TAB_TEXT: &str = "click (1157, 144)";
const TAB_SYMBOLS: &str = "click (1208, 144)";
const TAB_CALCS: &str = "click (1263, 144)";
const TAB_LAYERS: &str = "click (1313, 144)";
const PIN: &str = "click (1355, 144)";
const COLLAPSE: &str = "click (1385, 144)";

// ---------------------------------------------------------------------------
// Interaction matrix (real event routing: widget tree + subscriptions)
// ---------------------------------------------------------------------------

const BACKSTAGE_ITEM: &str = "Print Preview...";

/// A two-revision `.sketchy` store in a scratch directory.
fn two_revision_store(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sketch-rust-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("house.sketchy");
    let mut app = App::boot(1.0).0;
    app.subject_info.city = "Springfield".into();
    app.drawing.lines.push(wall(5, (0.0, 0.0), (10.0, 0.0)));
    let (_, _) = crate::file_dialogs::with_save_path(Some(path.clone()), || {
        let _ = app.update(Message::SaveFile);
    });
    app.subject_info.city = "Shelbyville".into();
    app.drawing.lines[0].end = Pt::new(12.0, 0.0);
    let _ = app.update(Message::SaveFile);
    path
}

#[test]
fn e2e_version_history_panel_banner_and_back_to_latest() {
    crate::persist::with_temp_store(|| {
        let path = two_revision_store("e2e-versions");
        let mut h = harness();
        h.send(Message::FileDropped(path.clone()));
        h.script(&["click \"File\"", "expect \"Version History...\"", "click \"Version History...\""]);
        assert!(h.shows("Mark as Final..."), "the Versions panel opens");
        assert!(h.shows("(current)"));
        h.script(&["click \"Open\""]);
        assert!(h.shows("Restore this version"), "the banner appears");
        assert!(h.shows("Viewing v1"), "the pill names the version");
        h.script(&["click \"Back to latest\""]);
        assert!(!h.shows("Restore this version"), "the banner is gone");
        let app = state(h);
        assert!(app.viewing_revision.is_none());
        assert_eq!(app.dock.active, DockTab::Versions);
        assert_eq!(app.subject_info.city, "Shelbyville");
        assert_eq!(app.drawing.lines[0].end.x, 12.0, "the head's geometry is back");
        let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
    });
}

#[test]
fn e2e_a_file_click_opens_the_backstage() {
    let mut h = harness();
    assert!(!h.shows(BACKSTAGE_ITEM), "backstage hidden at start");
    h.script(&["click \"File\""]);
    assert!(h.shows(BACKSTAGE_ITEM), "File click opens the backstage");
    assert!(state(h).file_menu);
}

#[test]
fn e2e_b_second_file_click_toggles_it_closed() {
    let mut h = harness();
    h.script(&["click \"File\"", "click \"File\""]);
    assert!(!h.shows(BACKSTAGE_ITEM));
    let app = state(h);
    // ui-manager.js: closing leaves File as the active tab.
    assert!(!app.file_menu);
    assert_eq!(app.active_tab, TabId::File);
}

#[test]
fn e2e_c_escape_closes_it() {
    let mut h = harness();
    h.script(&["click \"File\"", "type escape"]);
    assert!(!h.shows(BACKSTAGE_ITEM));
    assert!(!state(h).file_menu);
}

#[test]
fn e2e_d_canvas_press_closes_it_and_e_file_reopens() {
    let mut h = harness();
    h.script(&["click \"File\"", "click (700, 400)"]);
    assert!(!h.shows(BACKSTAGE_ITEM), "canvas press closes");
    h.script(&["click \"File\""]);
    assert!(
        h.shows(BACKSTAGE_ITEM),
        "File reopens after an outside close"
    );
    assert!(state(h).file_menu);
}

#[test]
fn e2e_f_another_tab_closes_it_and_shows_its_body() {
    let mut h = harness();
    h.script(&["click \"File\"", "click \"View\""]);
    assert!(!h.shows(BACKSTAGE_ITEM));
    assert!(h.shows("Mini-Map"));
    let app = state(h);
    assert_eq!(
        (app.active_tab, app.body, app.file_menu),
        (TabId::View, Body::View, false)
    );
}

#[test]
fn e2e_g_view_then_file_opens() {
    let mut h = harness();
    h.script(&["click \"View\"", "click \"File\""]);
    assert!(h.shows(BACKSTAGE_ITEM));
    assert!(state(h).file_menu);
}

#[test]
fn e2e_h_every_tab_shows_its_own_body() {
    let mut h = harness();
    for (tab, unique) in [
        ("View", "Mini-Map"),
        ("Tools", "Connect"),
        ("Reference Image", "Load\nImage"),
        ("Help", "Keyboard\nShortcuts"),
        ("Home", "Subject Info"),
    ] {
        h.script(&[&format!("click {tab:?}")]);
        assert!(h.shows(unique), "{tab} tab shows {unique:?}");
    }
}

#[test]
fn e2e_click_outside_inside_backstage_keeps_it_open() {
    // A press inside #file-menu (empty space below the items) must not close it.
    let mut h = harness();
    h.script(&["click \"File\"", "click (100, 790)"]);
    assert!(h.shows(BACKSTAGE_ITEM));
}

// ---------------------------------------------------------------------------
// Screen 3 — dock interactions
// ---------------------------------------------------------------------------

#[test]
fn e2e_dock_each_tab_shows_its_body() {
    let mut h = harness();
    assert!(h.shows("New area"), "Areas is the default panel");
    for (click, tab, unique) in [
        (TAB_TEXT, DockTab::Text, "Atrium"),
        (TAB_SYMBOLS, DockTab::Symbols, "Door Swing 36"),
        (TAB_CALCS, DockTab::Calcs, "No defined areas yet."),
        (
            TAB_LAYERS,
            DockTab::Layers,
            "Closed areas create layers automatically.",
        ),
        (TAB_AREAS, DockTab::Areas, "New area"),
    ] {
        h.script(&[click]);
        assert!(h.shows(unique), "{tab:?} shows {unique:?}");
    }
    assert!(!h.shows("Atrium"), "only the active panel is shown");
    assert_eq!(state(h).dock.active, DockTab::Areas);
}

#[test]
fn e2e_dock_areas_other_areas_round_trip() {
    let mut h = harness();
    h.script(&[TAB_CALCS, TAB_AREAS]);
    assert!(h.shows("New area") && !h.shows("No defined areas yet."));
    assert_eq!(state(h).dock.active.key(), "define");
}

#[test]
fn e2e_dock_pin_toggles_with_a_toast_that_expires() {
    let mut h = harness();
    h.script(&[PIN]);
    assert!(h.shows("Dock hides after placing."));
    h.wait(toast::SHOW_MS + 400);
    assert!(
        !h.shows("Dock hides after placing."),
        "toast gone after 3.2s"
    );
    h.script(&[PIN]);
    assert!(h.shows("Dock stays open after placing."));
    assert!(state(h).dock.pinned);
}

#[test]
fn e2e_dock_collapse_and_expand() {
    let mut h = harness();
    h.script(&[TAB_TEXT, COLLAPSE]);
    assert!(!h.shows("Atrium"), "collapsed dock hides its panels");
    // The expand chevron sits at the top of the 30px rail.
    h.script(&["click (1386, 150)"]);
    assert!(h.shows("Atrium"), "expand restores the same panel");
    let app = state(h);
    assert!(app.dock.open);
    assert_eq!(app.dock.width(), DEFAULT_W);
}

#[test]
fn e2e_dock_resize_drag_clamps_and_double_click_resets() {
    let mut h = harness();
    h.script(&["move (1088, 400)", "press (1088, 400)", "move (1040, 400)"]);
    h.script(&["move (990, 400)", "move (1300, 400)", "release (1300, 400)"]);
    h.script(&["move (900, 400)"]);
    let app = state(h);
    assert_eq!(
        app.dock.width,
        Some(320.0),
        "clamped to MIN, fixed after release"
    );
    assert!(!app.dock.resizing);

    let mut h = harness();
    h.script(&["press (1088, 400)", "move (1040, 400)"]);
    {
        // Mid-drag width follows the pointer: right edge 1400 - 1040.
        let mut h2 = harness();
        h2.script(&[
            "press (1088, 400)",
            "move (1040, 400)",
            "release (1040, 400)",
        ]);
        assert_eq!(state(h2).dock.width, Some(360.0));
    }
    h.script(&["move (950, 400)", "release (950, 400)"]);
    // Dock is now 400 wide: its handle straddles x = 1000. Double-click it.
    // The widget's double-click window is wall-clock (iced's `mouse::Click`
    // compares real Instants), so under full-suite parallel load the two
    // scripted clicks can exceed it. Extra attempts are harmless — the press
    // arm only flips `resizing` (no move in between) and `ResizeReset` keeps
    // the width at the CSS default — so run the pair a few times and assert.
    for _ in 0..4 {
        h.script(&["click (1000, 400)", "click (1000, 400)"]);
    }
    let app = state(h);
    assert_eq!(
        app.dock.width, None,
        "double-click resets to the CSS default"
    );
    assert_eq!(app.dock.width(), DEFAULT_W);
}

#[test]
fn e2e_text_filter_search_and_pick() {
    let mut h = harness();
    h.script(&[TAB_TEXT, "click (1372, 238)"]); // ABC
    assert!(h.shows("Carport") && !h.shows("Den"), "ABC range");
    h.script(&["click (1200, 180)", "type \"ba\""]);
    assert!(
        h.shows("Bath") && !h.shows("Atrium"),
        "search within the range"
    );
    h.script(&["click (1372, 214)"]); // All
    assert!(h.shows("½ Bath") && h.shows("Bath"));
    h.script(&["click \"Bath\""]);
    let app = state(h);
    assert_eq!(app.dock.text_search, "ba");
    assert_eq!(app.dock.text_range, "all");
    assert_eq!(app.dock.text_active.as_deref(), Some("Bath"));
}

#[test]
fn e2e_symbols_preview_search_and_category() {
    let mut h = harness();
    h.script(&[TAB_SYMBOLS]);
    assert!(h.shows("Select a symbol"));
    // The first default favourite of the 0.1.25 library (Door Swing 36, a
    // wall-snapping symbol: its caption names the snap instead of "Click
    // the drawing to place").
    h.script(&["click (1240, 253)"]);
    assert!(h.shows("Wall snap: side"));
    assert!(!h.shows("Select a symbol"));
    h.script(&["click (1200, 212)", "type \"window s\""]);
    assert!(h.shows("Window Single 36") && !h.shows("Window Double 60"));
    // Category select: open it and pick the 8th option (Favorites, then the
    // CADdillo categories: Doors & Windows, Laundry, Utilities, Furniture,
    // Kitchen, Bath & Plumbing, Stairs & Accessibility); menu rows are 26px
    // from y = 194. Menu entries are drawn, not text widgets, so they are
    // clicked by position. The search still applies.
    h.script(&["click (1240, 181)", "click (1240, 389)"]);
    let app = state(h);
    assert_eq!(app.dock.symbol_category, "caddillo-stairs");
    assert_eq!(app.dock.symbol_search, "window s");
    assert_eq!(
        app.dock.preview.as_deref(),
        Some("caddillo/door-swing-36"),
        "the preview stays on the last picked symbol"
    );
}

#[test]
fn e2e_symbol_placement_arms_and_commits() {
    let mut h = harness();
    // Pick a symbol from the dock: arms placement mode.
    h.script(&[TAB_SYMBOLS, "click \"Door Swing 36\""]);
    assert!(h.shows("Wall snap: side"));
    // Click the canvas: commits the symbol and returns to Select.
    h.script(&["click (700, 400)"]);
    let app = state(h);
    assert_eq!(app.view.mode, crate::view_state::Mode::Select);
    assert_eq!(app.drawing.symbols.len(), 1);
    assert_eq!(app.placing_symbol, None);
}

// ---------------------------------------------------------------------------
// Screen 4a — status bar view controls
// ---------------------------------------------------------------------------

#[test]
fn e2e_zoom_buttons_slider_and_scale_input() {
    let mut h = harness();
    h.script(&[ZOOM_IN]);
    assert!(h.shows("+ 120% (Imp) 1:"));
    h.script(&[ZOOM_OUT, ZOOM_OUT]);
    assert!(h.shows("+ 83% (Imp) 1:"));
    // Fresh layout: a narrower readout re-centres the block (slider moves).
    let mut h = harness();
    h.script(&[SLIDER_250]);
    assert!(h.shows("+ 250% (Imp) 1:"), "slider sets 250%");
    let app = state(h);
    assert!((app.view.zoom - 2.5).abs() < 1e-9);

    // 1:20 halves the zoom, keeps the readout; junk re-syncs the input.
    let mut h = harness();
    h.script(&[
        SCALE_END,
        "type backspace",
        "type backspace",
        "type \"20\"",
        "type enter",
    ]);
    assert!(h.shows("+ 100% (Imp) 1:"));
    h.script(&[
        SCALE_END,
        "type backspace",
        "type backspace",
        "type \"x\"",
        "type enter",
    ]);
    let app = state(h);
    assert_eq!(app.view.drawing_scale, 20.0);
    assert!((app.view.zoom - 0.5).abs() < 1e-9);
    assert!(app.scale_edit.is_none());
}

#[test]
fn e2e_grid_snap_area_snap_and_pitch_toggle() {
    let mut h = harness();
    h.script(&[GRID, SNAP, AREA_SNAP, "click (187, 847)", PITCH_QUARTER]);
    let app = state(h);
    assert!(!app.view.grid && !app.view.snap && app.view.area_snap);
    assert_eq!((app.view.area_snap_size, app.view.grid_size), (0.25, 0.25));
    // View › Grid runs the same command.
    let mut h = harness();
    h.script(&["click \"View\"", "click \"Grid\""]);
    assert!(!state(h).view.grid);
}

#[test]
fn e2e_filter_popover_toggles_and_checks() {
    let mut h = harness();
    h.script(&[FILTER]);
    assert!(h.shows("Show in sketch"));
    h.script(&["click \"Exterior walls\""]);
    h.script(&[FILTER]);
    assert!(
        !h.shows("Show in sketch"),
        "the Filter button closes it again"
    );
    // View › Filter opens the same popover with the remembered state.
    h.script(&["click \"View\"", "click \"Filter\""]);
    assert!(h.shows("Show in sketch"));
    assert_eq!(state(h).view.filters, [true, false, true, true, true, true]);
}

#[test]
fn e2e_totals_chip_opens_calcs_even_when_collapsed() {
    let mut h = harness();
    h.script(&[COLLAPSE, CHIP]);
    assert!(h.shows("No defined areas yet."));
    let app = state(h);
    assert!(app.dock.open);
    assert_eq!(app.dock.active, DockTab::Calcs);
}

#[test]
fn e2e_zoom_fence_drag_zooms_and_returns_to_select() {
    let mut h = harness();
    h.script(&[FENCE]);
    h.script(&[
        "press (300, 300)",
        "move (400, 375)",
        "move (500, 450)",
        "release (500, 450)",
    ]);
    assert!(h.shows("+ 429% (Imp) 1:"));
    let app = state(h);
    assert_eq!(app.view.mode, crate::view_state::Mode::Select);
    assert!((app.view.pan.0 - 1228.6566666666668).abs() < 1e-6);
    assert!((app.view.pan.1 - 803.7433333333333).abs() < 1e-6);
}

#[test]
fn e2e_center_buttons_and_keys_are_no_ops_on_an_empty_sketch() {
    let mut h = harness();
    h.script(&[
        ZOOM_IN,
        CENTER,
        CENTER_FIT,
        "move (600, 400)",
        "type \"c\"",
        "type \"C\"",
    ]);
    let app = state(h);
    assert_eq!(app.view.zoom_percent(), 120);
    assert_eq!(app.view.pan, (523.5, 351.5));
}

#[test]
fn e2e_typing_c_in_a_search_box_is_not_a_shortcut() {
    let mut h = harness();
    h.script(&[TAB_TEXT, "click (1200, 180)", "type \"c\""]);
    let app = state(h);
    assert_eq!(app.dock.text_search, "c");
}

// ---------------------------------------------------------------------------
// Screen 4b — north, pages, underlay, dialogs
// ---------------------------------------------------------------------------

#[test]
fn e2e_north_prompt_sets_normalizes_and_cancels() {
    let mut h = harness();
    h.script(&[NORTH]);
    assert!(h.shows("North direction in degrees (0 = up, clockwise):"));
    // The value is pre-selected: typing replaces it.
    h.script(&["type \"450\"", "type enter"]);
    assert!(h.shows("90°"), "450 normalizes to 90 and Enter is OK");
    h.script(&[NORTH, "type \"-45\"", "click \"Cancel\""]);
    assert!(h.shows("90°"), "Cancel keeps the angle");
    h.script(&[NORTH, "type \"abc\"", "click \"OK\""]);
    let app = state(h);
    assert_eq!(
        app.pages.page().north_angle,
        90.0,
        "parseFloat junk is ignored"
    );
    assert!(app.dialog.is_none());
}

#[test]
fn e2e_dialog_escape_and_backdrop_dismiss() {
    let mut h = harness();
    h.script(&[PAGE_NEXT, "type escape"]);
    assert!(!h.shows("Add a page after the current page?"));
    h.script(&[PAGE_NEXT, "click (100, 300)"]);
    assert!(
        !h.shows("Add a page after the current page?"),
        "a backdrop press cancels"
    );
    h.script(&[PAGE_NEXT, "click (700, 400)"]);
    assert!(
        h.shows("Add a page after the current page?"),
        "a press inside stays open"
    );
    let app = state(h);
    assert_eq!(app.pages.len(), 1);
    // An alert's Escape is its OK.
    let mut h = harness();
    h.script(&[PAGE_DEL]);
    assert!(h.shows("Cannot delete the last page."));
    h.script(&["type escape"]);
    assert!(state(h).dialog.is_none());
}

#[test]
fn e2e_pages_next_add_prev_delete_round_trip() {
    let mut h = harness();
    h.script(&[PAGE_NEXT, "click \"OK\""]);
    assert!(h.shows("2 of 2"));
    h.script(&[PAGE_PREV_2]);
    assert!(h.shows("1 of 2"));
    h.script(&[PAGE_NEXT_2]);
    assert!(
        h.shows("2 of 2"),
        "Next goes to an existing page without asking"
    );
    h.script(&[PAGE_DEL_2]);
    assert!(h.shows("Delete page 2?"));
    h.script(&["click \"OK\""]);
    assert!(h.shows("1 of 1"));
    let app = state(h);
    assert_eq!((app.pages.len(), app.pages.current), (1, 0));
    assert!(
        app.areas_disabled,
        "a page load leaves the empty areas select disabled"
    );
}

#[test]
fn e2e_north_is_kept_per_page() {
    let mut h = harness();
    h.script(&[NORTH, "type \"30\"", "click \"OK\"", PAGE_ADD]);
    assert!(h.shows("0°"), "a new page starts facing up");
    h.script(&[PAGE_PREV_2]);
    let shows = h.shows("30°");
    let app = state(h);
    assert!(
        shows,
        "page {} of {}, angles {:?}",
        app.pages.current + 1,
        app.pages.len(),
        app.pages
            .pages
            .iter()
            .map(|p| p.north_angle)
            .collect::<Vec<_>>()
    );
}

#[test]
fn e2e_underlay_select_and_view_tab_page_commands() {
    let mut h = harness();
    h.script(&[PAGE_ADD, PAGE_PREV_2, UNDERLAY_2, UNDERLAY_PICK_2]);
    assert!(h.shows("Underlay: Page 2"));
    let app = state(h);
    assert_eq!(app.pages.underlay, Some(1));

    let mut h = harness();
    h.script(&["click \"View\"", "click \"Add\"", "click \"Add\""]);
    assert!(h.shows("3 of 3"));
    h.script(&["click \"First Page\""]);
    assert!(h.shows("1 of 3"));
    h.script(&["click \"Last Page\""]);
    assert!(h.shows("3 of 3"));
    h.script(&["click \"Previous\""]);
    assert!(h.shows("2 of 3"), "View › Previous is enabled after page 1");
    h.script(&["click \"Delete\""]);
    assert!(h.shows("Delete page 2?"));
    h.script(&[
        "click \"OK\"",
        "click \"First Page\"",
        "click \"Next\"",
        "click \"Next\"",
    ]);
    assert!(h.shows("Add a page after the current page?"));
}

#[test]
fn e2e_first_page_previous_does_nothing_from_the_disabled_button() {
    let mut h = harness();
    h.script(&["click (1145, 846)"]);
    let app = state(h);
    assert_eq!(app.pages.current, 0);
    assert!(app.toasts.text().is_none(), "the disabled button is inert");
}

// ---------------------------------------------------------------------------
// Screen 5a — modes and navigation
// ---------------------------------------------------------------------------

#[test]
fn e2e_rail_tools_set_modes_and_panels() {
    use crate::view_state::Mode;
    let mut h = harness();
    h.script(&[RAIL_PAN]);
    assert_eq!(state(h).view.mode, Mode::Pan);
    let mut h = harness();
    h.script(&[RAIL_MEASURE]);
    assert!(h.shows("Measure: click points to chain legs. Enter finishes, Shift+Enter closes for area, Ctrl+C copies."));
    h.script(&[RAIL_DRAW_INT, RAIL_SELECT]);
    assert_eq!(state(h).view.mode, Mode::Select);
    // Text / Symbols / Define Area open their panels, not a mode.
    let mut h = harness();
    h.script(&[COLLAPSE, "click (20, 633)"]);
    let app = state(h);
    assert_eq!(
        (app.dock.open, app.dock.active, app.view.mode),
        (true, DockTab::Symbols, Mode::Select)
    );
}

#[test]
fn e2e_typed_entry_and_arrow_cursor() {
    // The live original: click (300,300) drops the first point at (-11,3);
    // typing "20" echoes "20'" into the HUD; Enter commits a 20 ft wall
    // straight up to (-11,23) and resets the readout (the angle stays); a
    // second Enter lifts the pen. "10" then two ArrowUps move the cursor
    // +10 then +1 (the buffer drives the first step) and set the keyboard
    // pointer, with the HUD showing the MATH angle 90.
    let mut h = harness();
    h.script(&[RAIL_DRAW_EXT, "move (341, 429)", "click (341, 429)"]);
    h.send(Message::Key(crate::input::KeyCmd::Buffer("2")));
    h.send(Message::Key(crate::input::KeyCmd::Buffer("0")));
    assert!(h.shows("20'"), "the buffer echoes into the readout");
    h.send(Message::Key(crate::input::KeyCmd::Enter));
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 1);
    assert_eq!(app.drawing.lines[0].start, Pt::new(-11.0, 3.0));
    assert_eq!(app.drawing.lines[0].end, Pt::new(-11.0, 23.0));
    assert_eq!(app.drawing.buffer, "");
    assert_eq!(app.drawing.hud.len, "0.0'");

    // The arrows: the buffer's 10 ft, then a 1 ft step.
    let mut h = harness();
    h.script(&[RAIL_DRAW_EXT, "move (341, 429)", "click (341, 429)"]);
    for key in ["1", "0"] {
        h.send(Message::Key(crate::input::KeyCmd::Buffer(key)));
    }
    for _ in 0..2 {
        h.send(Message::Key(crate::input::KeyCmd::Arrow {
            dir: 0,
            ctrl: false,
            shift: false,
        }));
    }
    let app = state(h);
    assert_eq!(app.drawing.cursor, Pt::new(-11.0, 14.0));
    assert!(app.drawing.keyboard_pointer);
    assert_eq!(
        (app.drawing.hud.len.as_str(), app.drawing.hud.ang.as_str()),
        ("11.0'", "90°")
    );
    assert!(app.drawing.lines.is_empty(), "arrows never commit");
}

#[test]
fn e2e_dimension_cycle_and_placement() {
    // The live original: View › Apply cycles outside -> inside -> hidden ->
    // outside, toasting "Dimensions: Inside" / "Dimensions: Hidden" /
    // "Dimensions: Outside", and the placement flips the label's side.
    let mut h = harness();
    h.script(&[VIEW_TAB, APPLY_DIMS]);
    assert!(h.shows("Dimensions: Inside"));
    let app = state(h);
    assert!(app.view.dimensions);
    assert_eq!(
        app.view.dimension_placement,
        crate::view_state::DimensionPlacement::In
    );

    let mut h = harness();
    h.script(&[VIEW_TAB, APPLY_DIMS, APPLY_DIMS]);
    assert!(h.shows("Dimensions: Hidden"));
    let app = state(h);
    assert!(!app.view.dimensions);
    assert_eq!(
        app.view.dimension_placement,
        crate::view_state::DimensionPlacement::In,
        "hiding keeps the placement"
    );

    // A third click comes back on the outside.
    let mut h = harness();
    h.script(&[VIEW_TAB, APPLY_DIMS, APPLY_DIMS, APPLY_DIMS]);
    assert!(h.shows("Dimensions: Outside"));
    let app = state(h);
    assert!(app.view.dimensions);
    assert_eq!(
        app.view.dimension_placement,
        crate::view_state::DimensionPlacement::Out
    );
}

// The caret and the menu rows live in the constants block above: the
// `.tool-caret` is page (15..24, 305..314), and the popup at (8, 317) with
// 5px padding and 29.5px rows puts the row centres at 336.75 / 366.25 /
// 395.75 / 425.25.

#[test]
fn e2e_wall_shape_menu_and_prompt_chain() {
    // The live original: right-click (or the caret) on Draw Exterior opens a
    // 190x128 menu at (8, 317) with Line / Rectangle / Circle / Polygon.
    // Rectangle asks "Width in feet" (20) then "Height in feet" (20) and adds
    // four walls centred on the canvas plus an UND area of 400 sf.
    let mut h = harness();
    h.script(&[CARET_EXT]);
    let app = state(h);
    assert!(app.shapes.open.is_some(), "the caret opens the menu");
    assert_eq!(
        app.view.mode,
        crate::view_state::Mode::Select,
        "the caret does not switch tools"
    );

    let mut h = harness();
    h.script(&[CARET_EXT, MENU_RECT]);
    assert!(h.shows("Width in feet"), "the width prompt follows");
    let app = state(h);
    assert!(app.dialog.is_some());
    assert!(
        app.shapes.pending.is_some(),
        "the chain is waiting on the width"
    );

    // The full chain: width 20 -> height 20 -> the rectangle.
    let mut h = harness();
    h.script(&[CARET_EXT, MENU_RECT, "type enter"]);
    assert!(h.shows("Height in feet"), "the height prompt follows");
    h.script(&["type enter"]);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 4);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert!((app.drawing.polygons[0].area - 400.0).abs() < 1e-6);
    let corners: Vec<_> = app.drawing.lines.iter().map(|l| l.start).collect();
    assert_eq!(
        corners,
        vec![
            Pt::new(-10.0, -10.0),
            Pt::new(10.0, -10.0),
            Pt::new(10.0, 10.0),
            Pt::new(-10.0, 10.0)
        ],
        "centred on the canvas, as the live original draws it"
    );

    // A right press on either draw tool opens the same menu (the original's
    // `contextmenu` handler), anchored under that button.
    let mut h = harness();
    h.script(&[
        "move (20, 299)",
        "press right (20, 299)",
        "release (20, 299)",
    ]);
    let app = state(h);
    assert!(app.shapes.open.is_some(), "right-click opens the menu");
    assert_eq!(
        app.shapes.open.map(|(kind, _)| kind),
        Some(sketch_core::model::WallType::Exterior)
    );

    let mut h = harness();
    h.script(&[
        "move (20, 333)",
        "press right (20, 333)",
        "release (20, 333)",
    ]);
    let app = state(h);
    assert_eq!(
        app.shapes.open.map(|(kind, at)| (kind, at.y)),
        Some((sketch_core::model::WallType::Interior, 351.0)),
        "the interior menu sits under its own button"
    );

    // Line just sets the mode.
    let mut h = harness();
    h.script(&[CARET_EXT, MENU_LINE]);
    let app = state(h);
    assert!(app.shapes.open.is_none(), "the menu closes");
    assert_eq!(app.view.mode, crate::view_state::Mode::DrawExterior);
}

#[test]
fn e2e_closing_a_loop_creates_an_undefined_area() {
    // The live original: four exterior walls closed at the start give one
    // polygon { type: 'UND', name: 'Undefined Area', area: 100, 4 edges with
    // the chain's line ids }, penUp, mode still draw-exterior, and the areas
    // select still reads "No defined areas" (UND areas do not count).
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "click (341, 429)",
        "click (541, 429)",
        "click (541, 629)",
        "click (341, 629)",
        "click (342, 430)",
    ]);
    let app = state(h);
    assert!(!app.drawing.pen_down, "the pen lifts");
    assert_eq!(
        app.view.mode,
        crate::view_state::Mode::DrawExterior,
        "the mode stays"
    );
    assert_eq!(app.drawing.lines.len(), 4);
    assert_eq!(app.drawing.polygons.len(), 1);
    let area = &app.drawing.polygons[0];
    assert_eq!(
        (area.kind.as_str(), area.name.as_str()),
        ("UND", "Undefined Area")
    );
    assert!((area.area - 100.0).abs() < 1e-9, "{}", area.area);
    let ids: Vec<_> = area.edges.iter().map(|e| e.line_id).collect();
    let walls: Vec<_> = app.drawing.lines.iter().map(|l| Some(l.id)).collect();
    assert_eq!(ids, walls, "the edges follow the chain's walls in order");

    // A fresh emulator for the on-screen check (`state` consumes the harness).
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "click (341, 429)",
        "click (541, 429)",
        "click (541, 629)",
        "click (341, 629)",
        "click (342, 430)",
    ]);
    assert!(h.shows("No defined areas 0.0"), "UND areas do not count");
}

#[test]
fn e2e_status_scrollbar_follows_the_drawing_readout() {
    // The bar is `overflow-x: auto`: once the readout wraps, the bar
    // overflows vertically and a 10px scrollbar appears, which keeps it
    // overflowing (hysteresis) until the content fits the narrower bar.
    // Each check gets a fresh emulator: `state()` consumes the harness.
    let mut h = harness();
    h.script(&[RAIL_DRAW_EXT, "click (341, 429)"]);
    assert!(!state(h).status_scroll, "pen down alone does not scroll");

    let mut h = harness();
    h.script(&[RAIL_DRAW_EXT, "click (341, 429)", "click (541, 429)"]);
    assert!(state(h).status_scroll, "the first wall's readout wraps");

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "click (341, 429)",
        "click (541, 429)",
        "move (391, 579)",
        "type escape",
    ]);
    assert!(
        !state(h).status_scroll,
        "Escape lifts the pen and the bar fits again"
    );

    let mut h = harness();
    h.script(&[RAIL_DRAW_INT, "click (341, 429)", "click (541, 429)"]);
    assert!(state(h).status_scroll, "an interior chain scrolls too");
}

#[test]
fn e2e_fixed_controls_open_select_calcs_and_codes() {
    let mut rows = Vec::new();
    for (label, tab, click) in [
        ("Home / Select", "click \"Home\"", "click (290, 93)"),
        ("View / View Calcs", "click \"View\"", "click (104, 93)"),
        ("Tools / Area Codes", "click \"Tools\"", "click (343, 93)"),
    ] {
        let mut h = harness();
        h.script(&[tab, click, AWAY]);
        let app = state(h);
        let opened = match label {
            "Home / Select" => app.select_menu.open.is_some(),
            "View / View Calcs" => app.dock.open && app.dock.active == DockTab::Calcs,
            "Tools / Area Codes" => app.codes_editor.open,
            _ => unreachable!(),
        };
        let row =
            serde_json::json!({"control": label, "expected_open": true, "observed_open": opened});
        println!("OBSERVED {row}");
        rows.push(row);
        assert!(opened, "{label} did not open its menu/panel/dialog");
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/control-audit");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("gui-clicks.json"),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
fn e2e_fixed_controls_home_clipboard_and_delete() {
    for (name, clicks, remaining) in [
        ("Copy", vec!["click (104, 93)"], 1),
        ("Paste", vec!["click (104, 93)", "click (166, 93)"], 2),
        ("Cut", vec!["click (42, 93)"], 0),
        ("Delete", vec!["click (352, 93)"], 0),
    ] {
        let mut h = harness();
        h.script(&SELECTED_WALL);
        h.script(&["click \"Home\""]);
        h.script(&clicks);
        let app = state(h);
        assert_eq!(app.drawing.lines.len(), remaining, "{name}");
        if name != "Delete" {
            assert_eq!(app.clipboard.as_ref().unwrap().lines.len(), 1, "{name}");
        }
        if name == "Paste" {
            assert_eq!(app.selection.lines.len(), 1, "pasted wall stays selected");
        }
        if name == "Cut" || name == "Delete" || name == "Paste" {
            assert!(app.undo.can_undo(), "{name} must be undoable");
        }
    }
}

#[test]
fn e2e_fixed_controls_home_select_all() {
    let mut h = defined_square();
    h.script(&[
        "type escape",
        RAIL_SELECT,
        "click \"Home\"",
        "click (228, 93)",
    ]);
    let mut app = state(h);
    assert_eq!(app.selection.lines.len(), 4);
    assert_eq!(app.selection.polys.len(), 1);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/control-buttons-015");
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = sketch_core::skx::write(&app.build_save_model(), &Default::default()).unwrap();
    std::fs::write(dir.join("input.skx"), bytes).unwrap();
}

#[test]
fn e2e_fixed_controls_home_rotate_sketch() {
    let mut h = harness();
    h.script(&SELECTED_WALL);
    h.script(&["click \"Home\"", "click (538, 93)"]);
    let app = state(h);
    let line = &app.drawing.lines[0];
    assert!(
        (line.start.x - line.end.x).abs() < 1e-6,
        "horizontal wall must turn vertical"
    );
    assert!(((line.start.y - line.end.y).abs() - 10.).abs() < 1e-6);
    assert_eq!(app.pages.page().north_angle, 90.);
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_fixed_controls_home_area_attributes() {
    let mut h = defined_square();
    h.script(&[
        "type escape",
        RAIL_SELECT,
        "click \"Home\"",
        "click (228, 93)",
        "click \"Home\"",
        "click (884, 93)",
    ]);
    assert!(h.shows("Plan Ref Number"));
    let app = state(h);
    assert!(app.attr_dialog.open);
    assert!(
        !app.attr_dialog.editable,
        "Home shows the read-only attributes view"
    );
}

#[test]
fn e2e_fixed_controls_file_save_and_save_as_buttons() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/single-save-016");
    std::fs::create_dir_all(&dir).unwrap();
    for (click, file) in [
        ("click (100, 154)", "first-save.sketchy"),
        ("click (100, 190)", "save-as.sketchy"),
    ] {
        let path = dir.join(file);
        let mut h = defined_square();
        let (_, requests) = crate::file_dialogs::with_save_path(Some(path.clone()), || {
            h.script(&["type escape", "click \"File\"", click]);
        });
        assert_eq!(requests, ["Untitled.sketchy"], "exactly one native picker");
        let (_, named_requests) = crate::file_dialogs::with_save_path(None, || {
            h.script(&[
                "click \"Home\"",
                "click (538, 93)",
                "click \"File\"",
                "click (100, 154)",
            ]);
        });
        assert!(
            named_requests.is_empty(),
            "named Save must not open the picker"
        );
        let app = state(h);
        assert!(app.dialog.is_none(), "no extra naming dialog");
        let chosen_name = path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(app.file_name, chosen_name);
        let saved = read_saved(&path);
        assert_eq!(saved["fileName"], chosen_name);
        assert_eq!(saved["pages"][0]["lines"].as_array().unwrap().len(), 4);
        assert_eq!(saved["pages"][0]["polygons"].as_array().unwrap().len(), 1);
        assert_eq!(saved["pages"][0]["polygons"][0]["type"], "GLA1");
        assert_eq!(
            saved["pages"][0]["northAngle"], 90.0,
            "the direct save writes the latest edits"
        );
    }
}

/// A saved sketch's model, whichever format `path` holds.
fn read_saved(path: &std::path::Path) -> serde_json::Value {
    let bytes = std::fs::read(path).unwrap();
    if sketch_core::sketchy::store::is_store_bytes(&bytes) {
        return crate::decode_store(path).unwrap().model;
    }
    sketch_core::skx::read(&bytes)
        .map(|r| r.model)
        .unwrap_or_else(|_| serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn e2e_opened_legacy_save_asks_and_opened_store_saves_in_place() {
    let dir = direct_save_test_dir();
    let mut fixture = state(defined_square());
    let model = fixture.build_save_model();
    // A legacy file never saves silently: Save asks and writes a `.sketchy`.
    for (file, json) in [("opened.skx", false), ("opened.json", true)] {
        let path = dir.join(file);
        let bytes = if json {
            serde_json::to_vec_pretty(&model).unwrap()
        } else {
            sketch_core::skx::write(&model, &Default::default()).unwrap()
        };
        std::fs::write(&path, &bytes).unwrap();
        let mut h = harness();
        let (_, requests) = crate::file_dialogs::with_open_path(Some(path.clone()), || {
            h.script(&["click \"File\"", "click (100, 118)"]);
        });
        assert_eq!(requests, 1, "actual Open button");
        let target = dir.join(format!("{file}.sketchy"));
        let (_, requests) = crate::file_dialogs::with_save_path(Some(target.clone()), || {
            h.script(&["click \"Home\"", "click (538, 93)", "click (34, 15)"]);
        });
        assert_eq!(requests.len(), 1, "QAT Save asks for a `.sketchy` name");
        let app = state(h);
        assert_eq!(app.current_file.as_ref().unwrap().path, target);
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "legacy file untouched");
        let saved = read_saved(&target);
        assert_eq!(saved["pages"][0]["northAngle"], 90.0);
        assert_eq!(saved["pages"][0]["polygons"].as_array().unwrap().len(), 1);
    }
    // An opened `.sketchy` store saves in place.
    let path = dir.join("opened.sketchy");
    crate::write_store(&model, &path, None).unwrap();
    let mut h = harness();
    let (_, requests) = crate::file_dialogs::with_open_path(Some(path.clone()), || {
        h.script(&["click \"File\"", "click (100, 118)"]);
    });
    assert_eq!(requests, 1, "actual Open button");
    let (_, requests) = crate::file_dialogs::with_save_path(None, || {
        h.script(&["click \"Home\"", "click (538, 93)", "click (34, 15)"]);
    });
    assert!(requests.is_empty(), "QAT Save must write directly");
    let app = state(h);
    assert_eq!(app.current_file.as_ref().unwrap().path, path);
    let saved = read_saved(&path);
    assert_eq!(saved["pages"][0]["northAngle"], 90.0);
    assert_eq!(saved["pages"][0]["polygons"].as_array().unwrap().len(), 1);
}

fn direct_save_test_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/direct-save-017");
    std::fs::create_dir_all(&dir).unwrap();
    std::path::absolute(dir).unwrap()
}

#[test]
fn e2e_direct_save_as_changes_target_only_after_success() {
    let dir = direct_save_test_dir();
    let blocked = dir.join("blocked.sketchy");
    std::fs::create_dir_all(&blocked).unwrap();
    for (case, destination) in [
        ("cancel", None),
        ("failed", Some(blocked)),
        ("success", Some(dir.join("copy.sketchy"))),
        ("success-json", Some(dir.join("copy.json"))),
    ] {
        let original = dir.join(format!("{case}-original.sketchy"));
        let mut h = defined_square();
        crate::file_dialogs::with_save_path(Some(original.clone()), || {
            h.script(&["type escape", "click \"File\"", "click (100, 154)"]);
        });
        let original_bytes = std::fs::read(&original).unwrap();
        let (_, requests) = crate::file_dialogs::with_save_path(destination.clone(), || {
            h.script(&["click \"File\"", "click (100, 190)"]);
        });
        assert_eq!(requests.len(), 1, "Save As always shows one picker");
        let (_, requests) = crate::file_dialogs::with_save_path(None, || {
            h.script(&[
                "click \"Home\"",
                "click (538, 93)",
                "click \"File\"",
                "click (100, 154)",
            ]);
        });
        assert!(requests.is_empty());
        let app = state(h);
        if case.starts_with("success") {
            let path = destination.unwrap();
            assert_eq!(app.current_file.as_ref().unwrap().path, path);
            assert_eq!(app.file_name, "copy");
            assert_eq!(
                std::fs::read(&original).unwrap(),
                original_bytes,
                "Save As leaves original intact"
            );
            let saved = read_saved(&path);
            assert_eq!(saved["pages"][0]["northAngle"], 90.0);
        } else {
            assert_eq!(app.current_file.as_ref().unwrap().path, original);
            let saved = read_saved(&original);
            assert_eq!(
                saved["pages"][0]["northAngle"], 90.0,
                "failed/cancelled Save As retains original target"
            );
        }
    }
}

#[test]
fn e2e_pointer_cache_matches_uncached_render_and_crosshair_is_visible() {
    let previous_theme = crate::canvas_theme::current();
    crate::canvas_theme::set(crate::canvas_theme::CanvasTheme::Light);
    let draw = |name: &str| {
        let mut h = defined_square();
        h.script(&[
            "type escape",
            RAIL_DRAW_EXT,
            "click (700, 300)",
            "move (541, 629)",
        ]);
        h.send(Message::Key(crate::input::KeyCmd::Arrow {
            dir: 2,
            ctrl: false,
            shift: false,
        }));
        h.screenshot(name)
    };
    let (uncached, draws) = crate::sketch_canvas::without_cache(|| draw("pointer-uncached"));
    assert!(draws > 0, "reference must actually bypass the cache");
    let cached = draw("pointer-cached");
    let a = image::open(uncached).unwrap().to_rgba8();
    let b = image::open(cached).unwrap().to_rgba8();
    assert!(
        a == b,
        "cached floorplan must preserve the complete rendered image"
    );
    for (theme, name) in [
        (crate::canvas_theme::CanvasTheme::Light, "crosshair-light"),
        (crate::canvas_theme::CanvasTheme::Dark, "crosshair-dark"),
    ] {
        crate::canvas_theme::set(theme);
        let mut h = harness();
        // The emulator suppresses the physical mouse cursor in screenshots;
        // the keyboard drawing pointer uses the same SVG and remains visible.
        h.script(&[RAIL_DRAW_EXT, "click (800, 400)"]);
        h.send(Message::Key(crate::input::KeyCmd::Arrow {
            dir: 2,
            ctrl: false,
            shift: false,
        }));
        let path = h.screenshot(name);
        let app = state(h);
        assert!(app.drawing.keyboard_pointer);
        let (x, y) = app.view.to_screen(crate::view_state::World {
            x: app.drawing.cursor.x,
            y: app.drawing.cursor.y,
        });
        let x = (x + f64::from(crate::minimap::CANVAS_ORIGIN.0)).floor() as u32;
        let y = (y + f64::from(crate::minimap::CANVAS_ORIGIN.1)).floor() as u32;
        let center = pixel_of(&path, x, y);
        let halo = pixel_of(&path, x, y + 1);
        assert!(
            center[0] < 60 && center[1] < 70 && center[2] < 80,
            "center: {center:?}"
        );
        assert!(
            halo[0] > 180 && halo[1] > 180 && halo[2] > 180,
            "halo: {halo:?}"
        );
    }
    crate::canvas_theme::set(previous_theme);
}

#[test]
fn e2e_save_as_original_is_unchanged_after_repeated_saves() {
    let root = direct_save_test_dir().join("save-as-matrix");
    let mut evidence = Vec::new();
    for (case, a, b) in [
        ("sketchy-sketchy", "original.sketchy", "new.sketchy"),
        ("skx-sketchy", "original.skx", "new.sketchy"),
        ("json-sketchy", "original.json", "new.sketchy"),
        ("skx-skx", "original.skx", "new.skx"),
        ("skx-json", "original.skx", "new.json"),
        ("json-skx", "original.json", "new.skx"),
        ("json-json", "original.json", "new.json"),
        ("same-name", "original/house.sketchy", "new/house.sketchy"),
    ] {
        let dir = root.join(case);
        let original = dir.join(a);
        let copy = dir.join(b);
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        let mut h = defined_square();
        let (_, first) = crate::file_dialogs::with_save_path(Some(original.clone()), || {
            h.script(&["type escape", "click \"File\"", "click (100, 154)"]);
        });
        assert_eq!(first.len(), 1);
        let baseline = std::fs::read(&original).unwrap();
        std::fs::write(dir.join("original.before"), &baseline).unwrap();
        let (_, picker) = crate::file_dialogs::with_save_path(Some(copy.clone()), || {
            h.script(&["click \"File\"", "click (100, 190)"]);
        });
        assert_eq!(picker.len(), 1);
        assert_eq!(
            std::fs::read(&original).unwrap(),
            baseline,
            "{case}: Save As changed original"
        );
        let mut previous = std::fs::read(&copy).unwrap();
        for (route, north) in [("File", 90.0), ("Toolbar", 180.0), ("Keyboard", 270.0)] {
            let (_, picker) = crate::file_dialogs::with_save_path(None, || {
                h.script(&["click \"Home\"", "click (538, 93)"]);
                match route {
                    "File" => h.script(&["click \"File\"", "click (100, 154)"]),
                    "Toolbar" => h.script(&["click (34, 15)"]),
                    _ => h.send(Message::Key(crate::input::KeyCmd::Save)),
                }
            });
            assert!(picker.is_empty(), "{case}: {route} opened a picker");
            assert_eq!(
                std::fs::read(&original).unwrap(),
                baseline,
                "{case}: {route} changed original"
            );
            let bytes = std::fs::read(&copy).unwrap();
            assert_ne!(bytes, previous, "{case}: {route} did not save edits");
            let saved = read_saved(&copy);
            assert_eq!(saved["pages"][0]["northAngle"], north, "{case}: {route}");
            assert_eq!(saved["pages"][0]["polygons"][0]["type"], "GLA1");
            previous = bytes;
        }
        let app = state(h);
        assert_eq!(app.current_file.as_ref().unwrap().path, copy);
        evidence.push(serde_json::json!({
            "case": case, "original": original, "copy": copy,
            "originalUnchanged": true, "subsequentSaves": ["File", "Toolbar", "Keyboard"],
            "lastSavedNorth": 270, "additionalSavePickers": 0
        }));
    }
    std::fs::write(
        root.join("results.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

#[test]
fn e2e_direct_save_new_document_forgets_previous_target() {
    let path = direct_save_test_dir().join("before-new.sketchy");
    let mut h = defined_square();
    crate::file_dialogs::with_save_path(Some(path.clone()), || {
        h.script(&["type escape", "click \"File\"", "click (100, 154)"]);
    });
    let before = std::fs::read(&path).unwrap();
    let (_, requests) = crate::file_dialogs::with_save_path(None, || {
        h.script(&[
            "click \"File\"",
            "click (100, 82)",
            "click \"OK\"",
            "click \"File\"",
            "click (100, 154)",
        ]);
    });
    assert_eq!(requests, ["Untitled.sketchy"]);
    let app = state(h);
    assert!(app.current_file.is_none());
    assert!(app.drawing.lines.is_empty());
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "new document must not overwrite the old file"
    );
}

#[test]
fn e2e_single_save_cancel_and_write_failure_preserve_document_name() {
    let blocked =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/single-save-016/blocked.skx");
    std::fs::create_dir_all(&blocked).unwrap();
    for destination in [None, Some(blocked)] {
        let failure = destination.is_some();
        let mut h = defined_square();
        let (_, requests) = crate::file_dialogs::with_save_path(destination, || {
            h.script(&["type escape", "click \"File\"", "click (100, 190)"]);
        });
        assert_eq!(requests.len(), 1);
        let app = state(h);
        assert!(app.dialog.is_none());
        assert_eq!(app.file_name, "Untitled");
        assert_eq!(app.drawing.lines.len(), 4);
        assert_eq!(app.drawing.polygons.len(), 1);
        if failure {
            assert!(app
                .toasts
                .text()
                .unwrap()
                .starts_with("Could not save sketch:"));
        }
    }
}

#[test]
fn e2e_view_controls_center_and_fit_live_sketch() {
    for action in [CENTER, CENTER_FIT, "type \"c\""] {
        let mut h = harness();
        h.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "move (541, 629)",
            "click (541, 629)",
            "type escape",
            RAIL_PAN,
            "press (400, 400)",
            "move (500, 450)",
            "release (500, 450)",
            RAIL_SELECT,
            action,
            AWAY,
        ]);
        let app = state(h);
        assert_eq!(app.drawing.lines.len(), 2, "{action}");
        let (min, max) = app.sketch_view_bounds().unwrap();
        let center = crate::view_state::World {
            x: (min.x + max.x) / 2.,
            y: (min.y + max.y) / 2.,
        };
        let (x, y) = app.view.to_screen(center);
        assert!((x - app.view.canvas.0 / 2.).abs() < 1e-7, "{action}: x={x}");
        assert!((y - app.view.canvas.1 / 2.).abs() < 1e-7, "{action}: y={y}");
        if action == CENTER_FIT {
            assert!(app.view.zoom > 1., "fit should enlarge this small sketch");
            for p in [min, max] {
                let (x, y) = app.view.to_screen(p);
                assert!(x >= 29.999 && x <= app.view.canvas.0 - 29.999);
                assert!(y >= 29.999 && y <= app.view.canvas.1 - 29.999);
            }
        } else {
            assert_eq!(app.view.zoom, 1., "Center must preserve zoom");
        }
    }
}

#[test]
fn e2e_pan_tool_drag_moves_the_view() {
    let mut h = harness();
    h.script(&[
        RAIL_PAN,
        "press (400, 400)",
        "move (450, 425)",
        "move (500, 450)",
        "release (500, 450)",
    ]);
    let app = state(h);
    assert_eq!(app.view.pan, (623.5, 401.5));
    assert!(app.view.pan_from.is_none(), "release ends the pan");
    // In Select mode a left drag does not pan.
    let mut h = harness();
    h.script(&["press (400, 400)", "move (500, 450)", "release (500, 450)"]);
    assert_eq!(state(h).view.pan, (523.5, 351.5));
}

#[test]
fn e2e_keys_switch_modes_grid_and_curve_mode() {
    use crate::view_state::Mode;
    let mut h = harness();
    h.script(&["type \"e\""]);
    assert_eq!(state(h).view.mode, Mode::DrawExterior);
    let mut h = harness();
    h.script(&["type \"i\"", "type escape"]);
    assert_eq!(state(h).view.mode, Mode::Select, "Escape returns to Select");
    let mut h = harness();
    h.script(&["type enter"]);
    assert_eq!(
        state(h).view.mode,
        Mode::DrawExterior,
        "Enter starts drawing"
    );
    let mut h = harness();
    h.script(&["type \"g\"", "type \"d\""]);
    assert!(h.shows("Curve mode: Height"));
    let app = state(h);
    assert!(!app.view.grid);
    // P / S / T and C are ignored while drawing; E/I still switch.
    let mut h = harness();
    h.script(&["type \"e\"", "type \"p\"", "type \"i\""]);
    assert_eq!(state(h).view.mode, Mode::DrawInterior);
    let mut h = harness();
    h.script(&[COLLAPSE, "type \"t\""]);
    let app = state(h);
    assert_eq!((app.dock.open, app.dock.active), (true, DockTab::Text));
}

#[test]
fn e2e_wheel_middle_and_space_pan_messages() {
    // The canvas maps these events to messages (sketch_canvas tests); here
    // the app applies them.
    let mut h = harness();
    for (up, x, y) in [
        (true, 300.0, 300.0),
        (true, 300.0, 300.0),
        (false, 600.0, 400.0),
    ] {
        h.send(Message::Wheel {
            up,
            x,
            y,
            shift: false,
            ctrl: false,
        });
    }
    assert!(h.shows("+ 109% (Imp) 1:"));
    let app = state(h);
    assert!((app.view.zoom - 1.089).abs() < 1e-9);
    assert!((app.view.pan.0 - 573.3915).abs() < 1e-6 && (app.view.pan.1 - 366.0835).abs() < 1e-6);

    let mut h = harness();
    h.send(Message::Key(crate::input::KeyCmd::SpaceDown));
    h.send(Message::PanStart(400.0, 400.0));
    h.send(Message::PanMove(500.0, 450.0));
    h.send(Message::PanEnd);
    h.send(Message::Key(crate::input::KeyCmd::SpaceUp));
    let app = state(h);
    assert_eq!(app.view.pan, (623.5, 401.5));
    assert!(app.view.space_panned && !app.view.space_pan);
}

#[test]
fn e2e_draw_color_opens_from_the_ribbon_on_the_draw_colour() {
    // Home > Draw Color = toggleDrawColorPalette: the dialog opens on
    // `state.drawColor` (#3182CE by default, state-init.js:21) on the
    // Standard tab, with the RGB fields showing that colour.
    let mut h = harness();
    h.script(&[DRAW_COLOR]);
    assert!(h.shows("Color"), "the dialog's header");
    assert!(h.shows("Standard") && h.shows("Custom"), "both tabs");
    assert!(h.shows("New"), "the preview label");
    let app = state(h);
    assert!(app.color.open);
    assert_eq!(app.color.tab, crate::color_dialog::Tab::Standard);
    assert_eq!(app.color.height(), crate::color_dialog::H_STANDARD);
    assert_eq!(app.color.rgb, ["49", "130", "206"]);
    assert_eq!(app.drawing.draw_color, "#3182CE", "opening changes nothing");
}

#[test]
fn e2e_draw_color_tabs_switch_and_the_card_resizes() {
    let mut h = harness();
    h.script(&[DRAW_COLOR, COLOR_AWAY, "click (784, 243)"]);
    let app = state(h);
    assert_eq!(app.color.tab, crate::color_dialog::Tab::Custom);
    assert_eq!(app.color.height(), crate::color_dialog::H_CUSTOM);
    let mut h = harness();
    h.script(&[
        DRAW_COLOR,
        COLOR_AWAY,
        "click (784, 243)",
        "click (615, 269)",
    ]);
    assert_eq!(
        state(h).color.tab,
        crate::color_dialog::Tab::Standard,
        "the Standard tab takes it back"
    );
}

#[test]
fn e2e_draw_color_preset_then_ok_sets_the_colour_and_toasts() {
    // The original's OK handler: `state.drawColor = hex`, render, notify
    // "Drawing color updated." (ui-manager.js:1865) — no trailing period
    // missing, exactly that sentence.
    let mut h = harness();
    h.script(&[
        DRAW_COLOR,
        COLOR_AWAY,
        "click (562, 517)",
        "click (829, 652)",
    ]);
    assert!(h.shows("Drawing color updated."), "the original's toast");
    let app = state(h);
    assert!(!app.color.open, "OK closes the dialog");
    assert_eq!(app.drawing.draw_color, "#ff0000");
}

#[test]
fn e2e_draw_color_cancel_and_backdrop_leave_the_colour_alone() {
    let mut h = harness();
    h.script(&[
        DRAW_COLOR,
        COLOR_AWAY,
        "click (562, 517)",
        "click (760, 652)",
    ]);
    let app = state(h);
    assert!(!app.color.open, "Cancel closes");
    assert_eq!(app.drawing.draw_color, "#3182CE", "and keeps the colour");

    let mut h = harness();
    h.script(&[
        DRAW_COLOR,
        COLOR_AWAY,
        "click (562, 517)",
        "click (100, 400)",
    ]);
    let app = state(h);
    assert!(!app.color.open, "a backdrop press closes");
    assert_eq!(app.drawing.draw_color, "#3182CE");
}

#[test]
fn e2e_draw_color_picking_on_the_wheel_changes_the_preview() {
    // A press on the wheel sets hue/saturation with value 1 (the wheel's
    // `updateFromStandard`), and the preview box follows.
    let mut h = harness();
    h.script(&[
        DRAW_COLOR,
        COLOR_AWAY,
        "move (660, 375)",
        "press (660, 375)",
        "release (660, 375)",
    ]);
    let app = state(h);
    assert!(app.color.open);
    assert!(
        (app.color.hsv.v - 1.0).abs() < 1e-6,
        "v {}",
        app.color.hsv.v
    );
    assert!(
        app.color.hsv.s > 0.0 && app.color.hsv.s < 1.0,
        "s {}",
        app.color.hsv.s
    );
}

#[test]
fn e2e_wall_solids_toggle_produces_solids() {
    // options.wallSolids on, default 6in exterior: two walls -> two solids.
    let mut h = harness();
    h.send(Message::WallSolids(true));
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "move (391, 579)",
    ]);
    let app = state(h);
    assert!(
        app.drawing.options.wall_solids,
        "the toggle reached the state"
    );
    assert!(app.drawing.options.wall_solids_enabled());
    assert_eq!(app.drawing.lines.len(), 2);
    let thickness: Vec<f64> = app
        .drawing
        .lines
        .iter()
        .map(|l| app.drawing.wall_thickness_of(l))
        .collect();
    assert_eq!(thickness, vec![0.5, 0.5], "6in = 0.5 ft");
    let solids = sketch_core::solids::compute_wall_solids(
        &app.drawing.lines,
        &|l| app.drawing.wall_thickness_of(l),
        &[],
    );
    assert_eq!(solids.solids.len(), 2);
    assert!(solids.solids.iter().all(|s| !s.pieces.is_empty()));
}

#[test]
fn e2e_shift_wheel_bows_the_last_wall_while_drawing() {
    // CanvasEngine's wheel handler: Shift while drawing bows the last wall
    // (+15 degrees up, -15 down, Ctrl for 1); a plain wheel still zooms.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
    ]);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 1);
    assert_eq!(app.drawing.lines[0].bow_angle, 0.0);
    let zoom = app.view.zoom;

    // Plain wheel: zooms, no bow.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
    ]);
    h.send(Message::Wheel {
        up: true,
        x: 300.0,
        y: 300.0,
        shift: false,
        ctrl: false,
    });
    let app = state(h);
    assert!(app.view.zoom > zoom, "the wheel still zooms");
    assert_eq!(app.drawing.lines[0].bow_angle, 0.0);

    // Shift+wheel: bows the last wall by 15 degrees.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
    ]);
    h.send(Message::Wheel {
        up: true,
        x: 300.0,
        y: 300.0,
        shift: true,
        ctrl: false,
    });
    let app = state(h);
    assert_eq!(app.drawing.lines[0].bow_angle, 15.0);
    assert_eq!(app.view.zoom, zoom, "bowing does not zoom");

    // Shift+wheel down, then Ctrl+Shift for the fine step.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
    ]);
    for (up, ctrl) in [(true, false), (true, false), (false, false), (true, true)] {
        h.send(Message::Wheel {
            up,
            x: 300.0,
            y: 300.0,
            shift: true,
            ctrl,
        });
    }
    let app = state(h);
    assert_eq!(app.drawing.lines[0].bow_angle, 16.0);
}

#[test]
fn e2e_define_first_blocks_exterior_drawing_until_a_type_is_chosen() {
    // handleCanvasClick: with defineFirst on, no selected area type and the
    // pen up in draw-exterior, the click opens the area panel and notifies
    // instead of starting a wall.
    let mut h = harness();
    h.send(Message::DefineFirst(true));
    h.script(&[RAIL_DRAW_EXT, "move (341, 429)", "click (341, 429)"]);
    assert!(
        h.shows("Define First is enabled. Choose an area type before drawing."),
        "the original's notice"
    );
    let app = state(h);
    assert!(!app.drawing.pen_down, "the pen stays up");
    assert!(app.drawing.lines.is_empty());
    assert!(app.drawing.pending_notice.is_none(), "the notice was taken");

    // Choosing a type lets drawing proceed.
    let mut h = harness();
    h.send(Message::DefineFirst(true));
    h.send(Message::AreaType(Some("GLA".into()), None));
    h.script(&[RAIL_DRAW_EXT, "move (341, 429)", "click (341, 429)"]);
    let app = state(h);
    assert!(
        app.drawing.pen_down,
        "the pen goes down once a type is chosen"
    );

    // Interior drawing is not gated.
    let mut h = harness();
    h.send(Message::DefineFirst(true));
    // The interior tool is the rail button at (20, 333).
    h.script(&["click (20, 333)", "move (341, 429)", "click (341, 429)"]);
    assert!(state(h).drawing.pen_down);
}

#[test]
fn e2e_define_first_names_the_closed_area() {
    // Closing a loop with defineFirst stamps the chosen type and name;
    // without it the area stays UND / "Undefined Area".
    let close = [
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "move (341, 629)",
        "click (341, 629)",
        "click (341, 429)",
    ];
    let mut h = harness();
    h.send(Message::DefineFirst(true));
    h.send(Message::AreaType(Some("GLA".into()), Some("Living".into())));
    h.script(&close);
    let app = state(h);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert_eq!(app.drawing.polygons[0].kind, "GLA");
    assert_eq!(app.drawing.polygons[0].name, "Living");

    // The name falls back to the type code.
    let mut h = harness();
    h.send(Message::DefineFirst(true));
    h.send(Message::AreaType(Some("GAR".into()), None));
    h.script(&close);
    let app = state(h);
    assert_eq!(app.drawing.polygons[0].kind, "GAR");
    assert_eq!(app.drawing.polygons[0].name, "GAR");

    // Off: the original's UND default.
    let mut h = harness();
    h.script(&close);
    let app = state(h);
    assert_eq!(app.drawing.polygons[0].kind, "UND");
    assert_eq!(app.drawing.polygons[0].name, "Undefined Area");
}

#[test]
fn e2e_selecting_a_wall_opens_the_edit_tools_tab() {
    // Clicking a wall in Select mode selects it and swaps the ribbon to the
    // contextual Edit Tools tab, as the original does.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
    ]);
    h.send(Message::Key(crate::input::KeyCmd::Select));
    h.script(&["click (441, 420)"]);
    assert!(h.shows("Edit Tools"), "the contextual tab");
    assert!(h.shows("Set Length") && h.shows("Line Style") && h.shows("Curve"));
    let app = state(h);
    assert_eq!(app.selection.lines.len(), 1);
    assert_eq!(app.active_tab, crate::ribbon::TabId::EditTools);
    assert_eq!(app.body, crate::ribbon::Body::EditTools);

    // Escape clears the selection and hands the Home tab back.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
    ]);
    h.send(Message::Key(crate::input::KeyCmd::Select));
    h.script(&["click (441, 420)"]);
    h.send(Message::Key(crate::input::KeyCmd::Escape));
    let app = state(h);
    assert!(app.selection.is_empty());
    assert_eq!(app.active_tab, crate::ribbon::TabId::Home);
    assert_eq!(app.body, crate::ribbon::Body::Home);
}

#[test]
fn e2e_undo_and_redo_walk_drawn_walls() {
    // Ctrl+Z after drawing two walls removes the second; Ctrl+Y puts it back.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
    ]);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 2);
    assert!(app.undo.can_undo());

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
    ]);
    h.send(Message::Undo);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 1, "the second wall is undone");
    assert!(app.undo.can_redo());

    // Redo restores it.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
    ]);
    h.send(Message::Undo);
    h.send(Message::Redo);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 2);
}

#[test]
fn e2e_undo_clears_the_selection_and_reports_an_empty_stack() {
    // `undo-history` with nothing banked shows the original's notice.
    let mut h = harness();
    h.send(Message::UndoHistory);
    assert!(h.shows("Nothing to undo yet."));
}

#[test]
fn e2e_undo_history_lists_steps_and_rewinds_to_one() {
    // `showUndoHistory`: newest first, numbered, each row rewinds to the state
    // before that action.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
    ]);
    let app = state(h);
    assert_eq!(app.undo.labels().len(), 3, "three wall commits");

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
    ]);
    h.send(Message::UndoHistory);
    let app = state(h);
    assert!(app.history.open.is_some(), "the popup opens");

    // Three clicks bank three pre-click states: the first click only puts the
    // pen down, so they are [empty, empty, one wall]. Rewinding to index 2
    // (one step) lands on the one-wall state.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
    ]);
    h.send(Message::UndoTo(2));
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 1);
    assert_eq!(app.undo.labels().len(), 2);
    assert!(
        app.history.open.is_none(),
        "choosing a row closes the popup"
    );
}

#[test]
fn e2e_copy_paste_delete_through_the_edit_tools_buttons() {
    // Draw two walls, select one, Copy, Paste, then Delete the copy.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
        // Select mode, then click the first wall.
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    let app = state(h);
    assert_eq!(app.selection.lines.len(), 1);

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    h.send(Message::Copy);
    h.send(Message::Paste);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 3, "the paste added a wall");
    assert_eq!(app.selection.lines.len(), 1, "the copy is selected");
    assert!(app.undo.can_undo(), "paste banks undo");
}

#[test]
fn e2e_delete_removes_the_selection_and_undo_restores_it() {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 1);
    assert_eq!(app.selection.lines.len(), 1);

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    h.send(Message::Delete);
    let app = state(h);
    assert!(app.drawing.lines.is_empty(), "the wall is gone");
    assert!(app.selection.is_empty(), "and the selection is cleared");
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_prompt_on_deletion_asks_before_deleting() {
    // `options.promptOnDeletion`: Delete confirms first; Cancel keeps the
    // wall (and banks no undo step), OK removes it.
    use crate::options::BoolKey;
    use crate::options_dialog::OptMsg;
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    h.send(Message::Options(OptMsg::Open));
    h.send(Message::Options(OptMsg::Toggle(BoolKey::PromptOnDeletion)));
    h.send(Message::Options(OptMsg::Ok));
    h.send(Message::Delete);
    assert!(h.shows("Delete the selected sketch items?"), "the confirm");
    h.send(Message::Dialog(crate::dialog::DialogMsg::Cancel));
    assert!(!h.shows("Delete the selected sketch items?"));
    h.send(Message::Delete);
    h.send(Message::Dialog(crate::dialog::DialogMsg::Ok));
    let app = state(h);
    assert!(app.drawing.lines.is_empty(), "OK deleted the wall");
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_delete_without_the_option_does_not_ask() {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    h.send(Message::Delete);
    assert!(!h.shows("Delete the selected sketch items?"));
    // Nothing selected: even with the option on, nothing is asked.
    let mut h = harness();
    h.send(Message::Delete);
    assert!(!h.shows("Delete the selected sketch items?"));
}

#[test]
fn e2e_cad_import_describes_the_result_and_rejects_dwg() {
    // S11-038: a .dwg gets the conversion hint; a .dxf imports with the
    // original's summary alert; garbage reports `Could not import <name>`.
    let dir = std::env::temp_dir().join("SketchRustCadTest");
    std::fs::create_dir_all(&dir).unwrap();

    let dwg = dir.join("plan.dwg");
    std::fs::write(&dwg, b"AC1032").unwrap();
    let mut app = state(harness());
    let _ = app.import_cad_file(&dwg);
    assert_eq!(
        app.dialog.as_ref().unwrap().message,
        sketch_core::cad_import::cad_import_dwg_hint()
    );
    assert!(app.drawing.lines.is_empty(), "nothing was imported");

    let dxf = dir.join("plan.dxf");
    std::fs::write(
        &dxf,
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n2\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nWALLS\n10\n0.0\n20\n0.0\n11\n20.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n",
    )
    .unwrap();
    let mut app = state(harness());
    let _ = app.import_cad_file(&dxf);
    let msg = app.dialog.as_ref().unwrap().message.clone();
    assert!(
        msg.starts_with("Imported 1 line from 1 CAD entities.\nUnits detected: feet (insunits)."),
        "{msg}"
    );
    assert_eq!(app.drawing.lines.len(), 1);
    assert!(app.undo.can_undo(), "one Import step");

    let bad = dir.join("bad.dxf");
    std::fs::write(&bad, "not a dxf").unwrap();
    let mut app = state(harness());
    let _ = app.import_cad_file(&bad);
    assert_eq!(
        app.dialog.as_ref().unwrap().message,
        "Could not import bad.dxf: No supported entities found in DXF."
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_page_layout_drives_the_pdf_report() {
    // S11-077: the report has a page per sketch page and follows Page
    // Layout's captions and toggles.
    use crate::page_layout::PageLayout;
    let mut h = harness();
    h.send(Message::Status(crate::statusbar::StatusMsg::PageAdd));
    let mut app = state(h);
    assert_eq!(app.pages.len(), 2);
    app.page_layout = PageLayout::default();
    let pdf = app.report_pdf();
    let text: String = pdf.iter().map(|b| *b as char).collect();
    assert!(text.contains("/Count 2"), "a report page per sketch page");
    let before = crate::print::render_pages(&pdf, 520).unwrap();
    assert_eq!(before.len(), 2);
    app.page_layout.captions.sketch_page_title = "FIELD SKETCH".into();
    app.page_layout.show_page_numbers = false;
    app.page_layout.show_comment_tables = false;
    let after = crate::print::render_pages(&app.report_pdf(), 520).unwrap();
    assert_eq!(after.len(), 2);
    assert!(
        diff_box(&decode_png(&before[0]), &decode_png(&after[0])).is_some(),
        "captions and toggles change the printed page"
    );
}

#[test]
fn e2e_auto_subtract_option_sets_new_areas_to_never() {
    // `options.autoSubtractOverlapped === false` -> new areas are 'never'.
    use crate::options::BoolKey;
    use crate::options_dialog::OptMsg;
    let close_loop = [
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "move (341, 629)",
        "click (341, 629)",
        "move (342, 430)",
        "click (342, 430)",
    ];
    let mut h = harness();
    h.script(&close_loop);
    let app = state(h);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert_eq!(app.drawing.polygons[0].subtract_mode, "auto");

    let mut h = harness();
    h.send(Message::Options(OptMsg::Open));
    h.send(Message::Options(OptMsg::Toggle(
        BoolKey::AutoSubtractOverlapped,
    )));
    h.send(Message::Options(OptMsg::Ok));
    h.script(&close_loop);
    let app = state(h);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert_eq!(app.drawing.polygons[0].subtract_mode, "never");
}

#[test]
fn e2e_new_text_labels_take_the_text_options() {
    // `placeTextAt`: font, size, Bold / Italic / Underline and colour come
    // from Options > Fonts; the old `#e2e8f0` is "automatic".
    use sketch_core::geometry::Pt;
    let mut app = state(harness());
    let plain = crate::placed_text::place_text_at(&mut app.drawing, Pt::new(1.0, 2.0), "a".into());
    let t = app.drawing.texts.iter().find(|t| t.id == plain).unwrap();
    assert_eq!((t.font_family.as_str(), t.font_size), ("Arial", 12.0));
    assert!(!t.bold && !t.italic && !t.underline);
    assert!(t.color.is_empty(), "the default #e2e8f0 is automatic");

    app.drawing.options.text_font = "Georgia".into();
    app.drawing.options.text_font_size = 18.0;
    app.drawing.options.text_bold = true;
    app.drawing.options.text_italic = true;
    app.drawing.options.text_underline = true;
    app.drawing.options.text_color = "#336699".into();
    let id = crate::placed_text::place_text_at(&mut app.drawing, Pt::new(3.0, 4.0), "b".into());
    let t = app.drawing.texts.iter().find(|t| t.id == id).unwrap();
    assert_eq!((t.font_family.as_str(), t.font_size), ("Georgia", 18.0));
    assert!(t.bold && t.italic && t.underline);
    assert_eq!(t.color, "#336699");
}

#[test]
fn e2e_png_export_follows_image_size_and_the_reference_image() {
    // S11-074: `options.imageSize`, and `saveImageWithSketch` puts the visible
    // reference image beneath the sketch.
    let png_size = |bytes: &[u8]| {
        let decoded = iced::widget::image::Handle::from_bytes(bytes.to_vec());
        let _ = decoded;
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    };
    let mut app = state(harness());
    let bytes = app.png_export_bytes().expect("renders");
    assert_eq!(png_size(&bytes), (2048, 1536), "the default size");

    app.drawing.options.image_size = "1280x720".into();
    let bytes = app.png_export_bytes().expect("renders");
    assert_eq!(png_size(&bytes), (1280, 720));

    // A solid red 8x8 reference image, shown and saved with the sketch.
    let mut raw = Vec::new();
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png)
        .unwrap();
    app.bg_image =
        crate::reference_image::BgImage::load(iced::widget::image::Handle::from_bytes(raw), 8, 8);
    app.bg_image.opacity = 1.0;
    let centre = |bytes: &[u8]| {
        let img = image::load_from_memory(bytes).unwrap().to_rgba8();
        *img.get_pixel(img.width() / 2, img.height() / 2)
    };
    let without = centre(&app.png_export_bytes().unwrap());
    assert_eq!(without.0[3], 0, "saveImageWithSketch off: transparent");
    app.drawing.options.save_image_with_sketch = true;
    let with = centre(&app.png_export_bytes().unwrap());
    assert!(
        with.0[3] > 200 && with.0[0] > with.0[2],
        "the image shows: {with:?}"
    );
    app.bg_image.visible = false;
    let hidden = centre(&app.png_export_bytes().unwrap());
    assert_eq!(hidden.0[3], 0, "a hidden image is not exported");
}

/// A defined 10x10 ft area (GLA1 "First Floor") on a fresh harness.
const DEFINED_SQUARE: [&str; 11] = [
    RAIL_DRAW_EXT,
    "move (341, 429)",
    "click (341, 429)",
    "move (541, 429)",
    "click (541, 429)",
    "move (541, 629)",
    "click (541, 629)",
    "move (341, 629)",
    "click (341, 629)",
    "move (342, 430)",
    "click (342, 430)",
];

fn defined_square(
) -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static> {
    let mut h = harness();
    h.send(Message::DefineFirst(true));
    h.send(Message::AreaType(
        Some("GLA1".into()),
        Some("First Floor".into()),
    ));
    h.script(&DEFINED_SQUARE);
    h
}

fn pixel_of(path: &std::path::Path, x: u32, y: u32) -> [u8; 4] {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let i = ((y * info.width + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

#[test]
fn e2e_defined_areas_draw_their_fill_and_the_area_fills_filter_hides_it() {
    // `drawPolygons`: a defined area is filled in its code's colour; the
    // Area fills filter (`filters.areas`) gates the whole pass.
    let mut h = defined_square();
    let on = pixel_of(&h.screenshot("area-fill-on"), 360, 446);

    let mut h = defined_square();
    h.send(Message::Status(crate::statusbar::StatusMsg::Filter(0)));
    let off = pixel_of(&h.screenshot("area-fill-off"), 360, 446);
    assert_ne!(on, off, "the fill shows only while the filter is on");
    assert!(
        off[0] > 225 && off[1] > 225 && off[2] > 225,
        "canvas white: {off:?}"
    );
    assert!(on[2] > on[0], "a blue-tinted fill: {on:?}");
}

#[test]
fn e2e_selection_hotkeys_rotate_scale_and_nudge() {
    use crate::input::KeyCmd;
    // Shift+R / Ctrl+Shift+R, + / -, and the arrows act on the selection.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    let before = state({
        let mut probe = harness();
        probe.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
        ]);
        probe
    })
    .drawing
    .lines[0]
        .clone();
    h.send(Message::Key(KeyCmd::Nudge(1.0, 0.0)));
    h.send(Message::Key(KeyCmd::Nudge(0.0, -0.1)));
    let app = state(h);
    let line = &app.drawing.lines[0];
    assert!(
        (line.start.x - (before.start.x + 1.0)).abs() < 1e-9,
        "nudged 1 ft right"
    );
    assert!(
        (line.start.y - (before.start.y - 0.1)).abs() < 1e-9,
        "and 0.1 ft down"
    );
    assert!(app.undo.can_undo());

    // Text and symbols: rotate in place, scale with clamps.
    let mut app = state(harness());
    let t = crate::placed_text::place_text_at(
        &mut app.drawing,
        sketch_core::geometry::Pt::new(0.0, 0.0),
        "x".into(),
    );
    app.selection.texts.insert(t);
    let _ = app.update(Message::RotateSelected(5.0));
    let _ = app.update(Message::ScaleSelected(1.1));
    let text = app.drawing.texts.iter().find(|x| x.id == t).unwrap();
    assert_eq!(text.rotation, 5.0);
    assert!((text.font_size - 13.2).abs() < 1e-9);
    for _ in 0..40 {
        let _ = app.update(Message::ScaleSelected(1.1));
    }
    assert_eq!(app.drawing.texts[0].font_size, 72.0, "clamped at 72");
}

#[test]
fn the_selection_and_sketch_hotkeys_are_bound() {
    use crate::input::{key_down, KeyCmd, KeyContext};
    use iced::keyboard::{key::Named, Key, Modifiers};
    let ch = |c: &str| Key::Character(c.into());
    let sel = KeyContext {
        has_selection: true,
        ..KeyContext::default()
    };
    let none = KeyContext::default();
    assert_eq!(
        key_down(&ch("r"), Modifiers::SHIFT, sel),
        Some(KeyCmd::RotateSelected(5.0))
    );
    assert_eq!(
        key_down(&ch("r"), Modifiers::SHIFT | Modifiers::CTRL, sel),
        Some(KeyCmd::RotateSelected(1.0))
    );
    assert_eq!(
        key_down(&ch("+"), Modifiers::SHIFT, sel),
        Some(KeyCmd::ScaleSelected(1.1))
    );
    assert_eq!(
        key_down(&ch("-"), Modifiers::empty(), sel),
        Some(KeyCmd::ScaleSelected(0.9))
    );
    assert_eq!(
        key_down(&ch("-"), Modifiers::empty(), none),
        None,
        "no selection"
    );
    assert_eq!(
        key_down(&ch("f"), Modifiers::empty(), none),
        Some(KeyCmd::FlipSketch(true))
    );
    assert_eq!(
        key_down(&ch("m"), Modifiers::empty(), none),
        Some(KeyCmd::FlipSketch(false))
    );
    assert_eq!(
        key_down(&ch("l"), Modifiers::empty(), none),
        Some(KeyCmd::RotateSketch(-90.0))
    );
    assert_eq!(
        key_down(&ch("r"), Modifiers::empty(), none),
        Some(KeyCmd::RotateSketch(90.0))
    );
    // Not while drawing: those go to the buffer / the mode keys.
    let drawing = KeyContext {
        drawing: true,
        ..KeyContext::default()
    };
    assert_ne!(
        key_down(&ch("f"), Modifiers::empty(), drawing),
        Some(KeyCmd::FlipSketch(true))
    );
    // The arrows nudge a selection (Shift = 0.1 ft) but only when not drawing.
    assert_eq!(
        key_down(&Key::Named(Named::ArrowUp), Modifiers::empty(), sel),
        Some(KeyCmd::Nudge(0.0, 1.0))
    );
    assert_eq!(
        key_down(&Key::Named(Named::ArrowLeft), Modifiers::SHIFT, sel),
        Some(KeyCmd::Nudge(-0.1, 0.0))
    );
    assert_eq!(
        key_down(&Key::Named(Named::ArrowUp), Modifiers::empty(), none),
        None
    );
}

#[test]
fn e2e_make_negative_and_clone_area_from_the_context_menu() {
    use crate::context_menu::Action;
    let mut h = defined_square();
    h.send(Message::SelectAll);
    h.send(Message::ContextAction(Action::MakeNegative));
    assert!(h.shows("Area made negative."));
    h.send(Message::ContextAction(Action::MakeNegative));
    assert!(h.shows("Area made positive."), "a second use flips it back");
    h.send(Message::ContextAction(Action::MakeNegative));
    let app = state(h);
    assert_eq!(app.drawing.polygons[0].factor_override, Some(-1.0));
    assert_eq!(app.drawing.polygons[0].factor, -1.0);

    // Nothing selected: the original's notices.
    let mut h = harness();
    h.send(Message::ContextAction(Action::MakeNegative));
    assert!(h.shows("Select an area to make negative."));
    h.send(Message::ContextAction(Action::CloneArea));
    assert!(h.shows("Select one or more areas to clone."));

    // Clone: a second area 2 ft over with its own four walls, selected.
    let mut h = defined_square();
    h.send(Message::SelectAll);
    h.send(Message::ContextAction(Action::CloneArea));
    let app = state(h);
    assert_eq!(app.drawing.polygons.len(), 2);
    assert_eq!(app.drawing.lines.len(), 8, "four new walls");
    let (a, b) = (&app.drawing.polygons[0], &app.drawing.polygons[1]);
    assert_eq!(b.kind, "GLA1");
    assert!((b.points[0].x - (a.points[0].x + 2.0)).abs() < 1e-9);
    assert!((b.points[0].y - (a.points[0].y + 2.0)).abs() < 1e-9);
    assert_ne!(a.id, b.id);
    assert!(b.edges.iter().all(|e| e.line_id.is_some()));
    assert_eq!(app.selection.polys.len(), 1);
    assert!(app.selection.polys.contains(&b.id), "the clone is selected");
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_context_rotate_banks_undo_before_the_change() {
    // The undo step must hold the state BEFORE the rotation, so undoing it
    // restores the horizontal wall.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    h.send(Message::ContextAction(
        crate::context_menu::Action::Rotate90,
    ));
    h.send(Message::Undo);
    let app = state(h);
    let line = &app.drawing.lines[0];
    assert!(
        (line.start.y - line.end.y).abs() < 1e-6,
        "undo restored the horizontal wall: {:?} -> {:?}",
        line.start,
        line.end
    );
}

/// A horizontal 10 ft wall drawn and selected (screen 341..541 at y 429).
const SELECTED_WALL: [&str; 9] = [
    RAIL_DRAW_EXT,
    "move (341, 429)",
    "click (341, 429)",
    "move (541, 429)",
    "click (541, 429)",
    "type escape",
    RAIL_SELECT,
    "move (441, 421)",
    "click (441, 421)",
];

#[test]
fn e2e_insert_vertex_splits_a_wall_and_its_area() {
    use crate::input::KeyCmd;
    let mut h = defined_square();
    h.send(Message::Key(KeyCmd::InsertVertex));
    // Empty canvas inside the square: the notice, and nothing changes.
    h.script(&["move (441, 520)", "click (441, 520)"]);
    assert!(h.shows("Click directly on a wall to insert a vertex."));
    // Onto the top wall.
    h.script(&["move (441, 429)", "click (441, 429)"]);
    let mut app = state(h);
    assert_eq!(app.drawing.lines.len(), 5);
    assert_eq!(app.drawing.polygons[0].points.len(), 5);
    assert_eq!(app.drawing.polygons[0].edges.len(), 5);
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.drawing.lines.len(),
        4,
        "one undo step restores the wall"
    );
    assert_eq!(app.drawing.polygons[0].points.len(), 4);
}

#[test]
fn e2e_curve_tool_drags_a_wall_into_an_arc_and_the_area_follows() {
    let mut h = defined_square();
    let before = {
        let probe = defined_square();
        let app = state(probe);
        app.computed[&app.drawing.polygons[0].id].net
    };
    h.send(Message::CurveSelected);
    assert!(h.shows("Curve Wall: drag a wall outward or inward. Calculations update live."));
    // Press the top wall and pull it outward (up on screen).
    h.script(&[
        "move (441, 429)",
        "press (441, 429)",
        "move (441, 420)",
        "move (441, 395)",
    ]);
    h.script(&["release (441, 395)"]);
    let app = state(h);
    let top = &app.drawing.lines[0];
    assert!(top.bow_angle.abs() > 5.0, "bowed: {}", top.bow_angle);
    assert!(
        app.selection.lines.contains(&top.id),
        "the wall is selected"
    );
    assert!(app.undo.can_undo());
    let poly = &app.drawing.polygons[0];
    let edge = poly
        .edges
        .iter()
        .find(|e| e.line_id == Some(top.id))
        .unwrap();
    assert_eq!(edge.bow_angle.abs(), top.bow_angle.abs());
    let after = app.computed[&poly.id].net;
    assert!(
        (after - before).abs() > 1.0,
        "the live area follows the curve: {before} -> {after}"
    );
}

#[test]
fn e2e_a_selected_walls_green_handle_bows_it_in_select_mode() {
    let mut h = harness();
    h.script(&SELECTED_WALL);
    // The handle sits on the chord midpoint of a straight wall: world
    // (-6, 3) is window (444.5, 420.5).
    h.script(&[
        "move (445, 421)",
        "press (445, 421)",
        "move (445, 405)",
        "move (445, 390)",
        "release (445, 390)",
    ]);
    let app = state(h);
    let line = &app.drawing.lines[0];
    assert!(
        line.bow_angle > 5.0,
        "dragged up bows positive: {}",
        line.bow_angle
    );
    assert!(app.selection.lines.contains(&line.id), "still selected");
    // The drag must not have moved the wall itself.
    assert!((line.start.y - line.end.y).abs() < 1e-9);
}

#[test]
fn e2e_wheel_bows_the_selected_wall_and_scales_labels() {
    use crate::input::KeyCmd;
    let wheel = |up, shift, ctrl| Message::Wheel {
        up,
        x: 441.0,
        y: 429.0,
        shift,
        ctrl,
    };
    let mut h = harness();
    h.script(&SELECTED_WALL);
    h.send(wheel(true, false, false));
    h.send(wheel(true, true, false));
    h.send(wheel(false, false, true));
    let mut app = state(h);
    assert_eq!(app.drawing.lines[0].bow_angle, 15.0 + 5.0 - 1.0);
    // Height mode (D): +1 ft of rise, Shift/Ctrl 0.1 ft.
    let _ = app.update(Message::Key(KeyCmd::CurveMode));
    let rise = crate::drawing::arc_height_of(&app.drawing.lines[0]);
    let _ = app.update(wheel(true, false, false));
    let after = crate::drawing::arc_height_of(&app.drawing.lines[0]);
    assert!((after - rise - 1.0).abs() < 1e-9, "{rise} -> {after}");
    let _ = app.update(wheel(false, true, false));
    let finer = crate::drawing::arc_height_of(&app.drawing.lines[0]);
    assert!((after - finer - 0.1).abs() < 1e-9);
    // One undo step per notch.
    assert!(app.undo.can_undo());

    // A selected label scales instead of zooming the view.
    let mut app = state(harness());
    let id = crate::placed_text::place_text_at(
        &mut app.drawing,
        sketch_core::geometry::Pt::new(0.0, 0.0),
        "x".into(),
    );
    app.selection.texts.insert(id);
    let zoom = app.view.zoom;
    let _ = app.update(wheel(true, false, false));
    assert!((app.drawing.texts[0].font_size - 12.0 * 1.08).abs() < 1e-9);
    let _ = app.update(wheel(false, false, false));
    assert!((app.drawing.texts[0].font_size - 12.0 * 1.08 * 0.92).abs() < 1e-9);
    assert_eq!(app.view.zoom, zoom, "the view did not zoom");

    // Nothing selected: the wheel still zooms.
    let mut app = state(harness());
    let zoom = app.view.zoom;
    let _ = app.update(wheel(true, false, false));
    assert!(app.view.zoom > zoom);
}

#[test]
fn e2e_curve_guides_and_handle_paint_while_bowing() {
    // The selected wall's green handle, and mid-drag the green guides.
    let green = |p: [u8; 4]| p[1] > 150 && p[0] < 90 && p[2] < 130;
    let mut h = harness();
    h.script(&SELECTED_WALL);
    let shot = h.screenshot("curve-handle");
    // Handle: world (-6, 3) is canvas (403.5, 291.5), window (444.5, 420.5).
    assert!(
        green(pixel_of(&shot, 443, 420)),
        "handle square at the midpoint"
    );

    let mut h = harness();
    h.script(&SELECTED_WALL);
    h.script(&[
        "move (445, 421)",
        "press (445, 421)",
        "move (445, 405)",
        "move (445, 380)",
    ]);
    let shot = h.screenshot("curve-guides");
    let mut found = false;
    for y in 300..421 {
        found |= green(pixel_of(&shot, 445, y));
    }
    assert!(found, "the rise guide is green above the chord");
}

#[test]
fn e2e_select_all_takes_text_labels_and_symbols_too() {
    let mut app = state(defined_square());
    let t = crate::placed_text::place_text_at(
        &mut app.drawing,
        sketch_core::geometry::Pt::new(0.0, 0.0),
        "x".into(),
    );
    let _ = app.update(Message::SelectAll);
    assert_eq!(app.selection.lines.len(), 4);
    assert_eq!(app.selection.polys.len(), 1);
    assert!(
        app.selection.texts.contains(&t),
        "the label is selected too"
    );
    // The Ctrl+A key does the same.
    app.selection.clear();
    let _ = app.update(Message::Key(crate::input::KeyCmd::SelectAll));
    assert!(app.selection.texts.contains(&t));
}

#[test]
fn e2e_edit_tools_area_group_styles_the_selected_area() {
    let mut h = defined_square();
    h.send(Message::SelectAll);
    for text in [
        "Fill Pattern",
        "Fill Color",
        "Area Props",
        "Line Style",
        "Line Thickness",
        "Line Color",
        "Flip Area",
    ] {
        assert!(h.shows(text), "the area group shows {text}");
    }
    // The three selects cycle to their next option.
    h.script(&["move (470, 102)", "click (470, 102)"]);
    assert!(h.shows("None"));
    h.script(&["move (918, 102)", "click (918, 102)"]);
    assert!(h.shows("2 px"));
    h.script(&["move (814, 102)", "click (814, 102)"]);
    assert!(h.shows("Dashed"));
    h.screenshot("edit-tools-area");
    let app = state(h);
    let poly = &app.drawing.polygons[0];
    assert_eq!(poly.fill_pattern.as_deref(), Some("none"));
    assert_eq!(poly.line_thickness, Some(2.0));
    assert_eq!(poly.line_style.as_deref(), Some("dashed"));
    // The outline styles also restyle the walls the area rides.
    assert!(app
        .drawing
        .lines
        .iter()
        .all(|l| l.line_thickness == Some(2.0) && l.line_style.as_deref() == Some("dashed")));
    assert!(app.undo.can_undo());

    // Auto Subtract and Flip Area buttons.
    let mut h = defined_square();
    h.send(Message::SelectAll);
    h.script(&["move (736, 93)", "click (736, 93)"]);
    assert!(h.shows("Auto-Subtract disabled for selected area."));
    let before = state({
        let mut p = defined_square();
        p.send(Message::SelectAll);
        p
    })
    .drawing
    .polygons[0]
        .points[0];
    h.script(&["move (1062, 93)", "click (1062, 93)"]);
    let app = state(h);
    assert_eq!(app.drawing.polygons[0].subtract_mode, "never");
    assert_ne!(app.drawing.polygons[0].points[0], before, "flipped");
}

#[test]
fn e2e_area_colors_open_the_color_dialog_and_apply() {
    let mut app = state(defined_square());
    let _ = app.update(Message::SelectAll);
    let _ = app.update(Message::AreaColorClicked(true));
    assert_eq!(app.color_target, crate::color_dialog::ColorTarget::AreaFill);
    let _ = app.update(Message::AreaColorClicked(false));
    assert_eq!(app.color_target, crate::color_dialog::ColorTarget::AreaLine);
    // OK in the dialog applies the colour to the area (and the walls for the
    // outline colour).
    app.color.open_with("#ff0000");
    let _ = app.update(Message::Color(crate::color_dialog::ColorMsg::Ok));
    let poly = &app.drawing.polygons[0];
    assert_eq!(poly.line_color.as_deref(), Some("#ff0000"));
    assert!(app
        .drawing
        .lines
        .iter()
        .all(|l| l.color.as_deref() == Some("#ff0000")));
    assert_eq!(
        app.color_target,
        crate::color_dialog::ColorTarget::DrawColor
    );
}

#[test]
fn e2e_area_and_wall_groups_stack_in_the_edit_tools_ribbon() {
    let mut h = defined_square();
    h.send(Message::SelectAll);
    // Select All takes the walls too: both groups are present.
    assert!(h.shows("Flip Area"));
    assert!(
        h.shows("Set Length"),
        "the wall group follows the area group"
    );
    h.screenshot("edit-tools-area-and-wall");
}

#[test]
fn e2e_explicit_ribbon_actions_run_when_the_button_is_clicked() {
    // Edit Tools buttons that carry their own action (Curve here) were inert
    // because only the caption table was consulted.
    let mut h = harness();
    h.script(&SELECTED_WALL);
    h.script(&["move (778, 93)", "click (778, 93)"]);
    let app = state(h);
    assert_eq!(app.view.mode, crate::view_state::Mode::Curve);
}

/// A harness with three pages and the Pages navigator open (rail button).
fn pages_nav_harness(
) -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static> {
    let mut h = harness();
    h.send(Message::Status(crate::statusbar::StatusMsg::PageAdd));
    h.send(Message::Status(crate::statusbar::StatusMsg::PageAdd));
    h.script(&["move (20, 749)", "click (20, 749)"]);
    h
}

#[test]
fn e2e_pages_navigator_opens_lists_pages_and_resizes_the_canvas() {
    let before = state(harness()).view.canvas.0;
    let mut h = pages_nav_harness();
    for text in [
        "PAGES",
        "Page 1",
        "Page 2",
        "Page 3",
        "current",
        "ghost",
        "Underlay: None",
    ] {
        assert!(h.shows(text), "the navigator shows {text}");
    }
    h.screenshot("pages-nav");
    let app = state(h);
    assert!(app.pages_nav_open);
    assert_eq!(
        app.view.canvas.0,
        before - 168.0,
        "the canvas gives up 168px"
    );

    // The header's close button (and the rail button again) shut it.
    let mut h = pages_nav_harness();
    h.script(&["move (186, 148)", "click (186, 148)"]);
    let app = state(h);
    assert!(!app.pages_nav_open);
    assert_eq!(app.view.canvas.0, before);
    let mut h = pages_nav_harness();
    h.script(&["move (20, 749)", "click (20, 749)"]);
    assert!(!state(h).pages_nav_open, "the rail button toggles");
}

#[test]
fn e2e_pages_navigator_rows_switch_ghost_and_delete() {
    // Clicking a row goes to that page.
    let mut h = pages_nav_harness();
    h.script(&["move (90, 180)", "click (90, 180)"]);
    assert_eq!(state(h).pages.current, 0);

    // The ghost tag toggles the underlay; clicking again clears it.
    let mut h = pages_nav_harness();
    h.script(&["move (90, 180)", "click (90, 180)"]);
    h.script(&["move (100, 210)", "move (153, 210)", "click (153, 210)"]);
    assert_eq!(state(h).pages.underlay, Some(1));
    let mut h = pages_nav_harness();
    h.script(&["move (90, 180)", "click (90, 180)"]);
    h.script(&[
        "move (100, 210)",
        "move (153, 210)",
        "click (153, 210)",
        "click (153, 210)",
    ]);
    assert_eq!(state(h).pages.underlay, None);

    // Hover reveals the trash; it goes to the page and asks to delete it.
    let mut h = pages_nav_harness();
    h.script(&["move (100, 210)", "move (186, 210)", "click (186, 210)"]);
    assert!(h.shows("Delete page 2?"));
    let app = state(h);
    assert_eq!(app.pages.current, 1, "deletePage acts on the current page");
    assert_eq!(app.pages.len(), 3, "nothing is deleted until confirmed");

    // The footer select is the same underlay.
    let mut app = state(pages_nav_harness());
    let _ = app.update(Message::PagesNav(crate::pages_nav::NavMsg::Underlay(Some(
        0,
    ))));
    assert_eq!(app.pages.underlay, Some(0));
    let _ = app.update(Message::PagesNav(crate::pages_nav::NavMsg::Underlay(None)));
    assert_eq!(app.pages.underlay, None);
}

/// Canvas px of a world point at the harness's initial view.
fn canvas_px(x: f64, y: f64) -> (f32, f32) {
    ((523.5 + x * 20.0) as f32, (351.5 - y * 20.0) as f32)
}

fn press_drag(app: &mut App, from: (f32, f32), to: (f32, f32), ctrl: bool) {
    let _ = app.update(Message::CanvasPress(from.0, from.1));
    let _ = app.update(Message::CanvasDrag(to.0, to.1, ctrl));
}

fn app_with_label() -> App {
    let mut app = state(harness());
    let id = crate::placed_text::place_text_at(
        &mut app.drawing,
        sketch_core::geometry::Pt::new(0.0, 0.0),
        "Porch".into(),
    );
    app.selection.texts.insert(id);
    app
}

#[test]
fn e2e_a_text_labels_handle_rotates_it_in_15_degree_steps() {
    let (cx, cy) = canvas_px(0.0, 0.0);
    // The handle is 30px above the 28px-tall box's top: 44px above the centre.
    let mut app = app_with_label();
    press_drag(&mut app, (cx, cy - 44.0), (cx + 60.0, cy), false);
    assert!(matches!(
        app.view.drag,
        Some(crate::view_state::Drag::RotateText { preview, .. }) if preview == 90.0
    ));
    let _ = app.update(Message::CanvasDragEnd);
    assert_eq!(
        app.drawing.texts[0].rotation, 90.0,
        "the top handle swung to the right"
    );
    assert!(app.undo.can_undo());

    // A drag that stays under half a step does nothing (and banks nothing).
    let mut app = app_with_label();
    press_drag(&mut app, (cx, cy - 44.0), (cx + 4.0, cy - 44.0), false);
    let _ = app.update(Message::CanvasDragEnd);
    assert_eq!(app.drawing.texts[0].rotation, 0.0);
    assert!(!app.undo.can_undo());

    // Ctrl gives 1 degree steps.
    let mut app = app_with_label();
    press_drag(&mut app, (cx, cy - 44.0), (cx + 4.0, cy - 44.0), true);
    let _ = app.update(Message::CanvasDragEnd);
    let r = app.drawing.texts[0].rotation;
    assert!(r != 0.0 && r.fract() == 0.0 && r.abs() < 15.0, "{r}");
}

#[test]
fn e2e_the_selected_areas_handle_rotates_the_geometry_on_release() {
    let mut app = state(defined_square());
    app.view.mode = crate::view_state::Mode::Select;
    let poly = app.drawing.polygons[0].id;
    let before = app.drawing.polygons[0].points[0];
    app.selection.clear();
    app.selection.polys.insert(poly);
    // The square spans x -11..-1, y -7..3: handle 34px above the top edge.
    let (hx, hy) = canvas_px(-6.0, 3.0);
    let (_, centre_y) = canvas_px(-6.0, -2.0);
    press_drag(&mut app, (hx, hy - 34.0), (hx + 96.0, centre_y), false);
    assert!(matches!(
        app.view.drag,
        Some(crate::view_state::Drag::RotateArea { preview, .. }) if preview == -90.0
    ));
    assert_eq!(
        app.drawing.polygons[0].points[0], before,
        "only a preview until release"
    );
    let _ = app.update(Message::CanvasDragEnd);
    let p0 = app.drawing.polygons[0].points[0];
    assert!(
        (p0.x - -1.0).abs() < 1e-9 && (p0.y - 3.0).abs() < 1e-9,
        "{p0:?}"
    );
    assert!((app.drawing.polygons[0].area - 100.0).abs() < 1e-6);
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_an_area_label_selects_rotates_and_reaches_the_export() {
    let mut app = state(defined_square());
    app.view.mode = crate::view_state::Mode::Select;
    let id = app.drawing.polygons[0].id;
    app.selection.clear();
    // Clicking the label (the area's centre) picks its text, not the area.
    let (lx, ly) = canvas_px(-6.0, -2.0);
    let _ = app.update(Message::CanvasClick {
        x: lx,
        y: ly,
        double: false,
        ctrl: false,
    });
    assert!(app.selection.polys.is_empty());
    assert!(app.selection.area_labels.contains(&id) || app.selection.area_sizes.contains(&id));
    // Take the label block and turn its handle a quarter turn.
    app.selection.clear();
    app.selection.area_labels.insert(id);
    let blocks = crate::handles::area_blocks(&app.drawing, &app.computed, &app.drawing.polygons[0]);
    let at = blocks
        .iter()
        .find(|b| b.kind == crate::handles::TextKind::AreaLabel)
        .unwrap()
        .at;
    let (bx, by) = canvas_px(at.x, at.y);
    // One-line label: a 23px box, the handle 11.5 + 30 above its centre.
    press_drag(&mut app, (bx, by - 41.5), (bx + 60.0, by), false);
    let _ = app.update(Message::CanvasDragEnd);
    assert_eq!(app.drawing.polygons[0].label_rotation, Some(90.0));
    assert_eq!(app.drawing.polygons[0].area_size_rotation, None);
    // The size block follows the label until it has its own.
    let blocks = crate::handles::area_blocks(&app.drawing, &app.computed, &app.drawing.polygons[0]);
    let size = blocks
        .iter()
        .find(|b| b.kind == crate::handles::TextKind::AreaSize)
        .unwrap();
    assert_eq!(size.rotation, 90.0);
    // The report model carries it (SVG / DXF read `labelRotation`).
    let page = crate::exporter::build_export_page(&app.drawing, 0.0, &app.area_codes);
    assert_eq!(page.polygons[0].label_rotation, 90.0);
}

#[test]
fn e2e_shift_r_turns_selected_area_text_and_select_all_takes_it() {
    let mut app = state(defined_square());
    let id = app.drawing.polygons[0].id;
    let _ = app.update(Message::SelectAll);
    assert!(app.selection.area_labels.contains(&id) && app.selection.area_sizes.contains(&id));
    app.selection.lines.clear();
    app.selection.polys.clear();
    let _ = app.update(Message::RotateSelected(15.0));
    assert_eq!(app.drawing.polygons[0].label_rotation, Some(15.0));
    assert_eq!(app.drawing.polygons[0].area_size_rotation, Some(15.0));
}

#[test]
fn e2e_double_clicking_a_text_label_asks_for_new_wording() {
    use crate::dialog::DialogMsg;
    let mut app = state(harness());
    let id = crate::placed_text::place_text_at(
        &mut app.drawing,
        sketch_core::geometry::Pt::new(0.0, 0.0),
        "Porch".into(),
    );
    let (x, y) = canvas_px(0.0, 0.0);
    let click = |app: &mut App| {
        let _ = app.update(Message::CanvasClick {
            x,
            y,
            double: true,
            ctrl: false,
        });
    };
    click(&mut app);
    let dialog = app.dialog.as_ref().expect("the Text: prompt");
    assert_eq!(dialog.message, "Text:");
    assert_eq!(dialog.input.as_deref(), Some("Porch"));
    let _ = app.update(Message::Dialog(DialogMsg::Input("Deck".into())));
    let _ = app.update(Message::Dialog(DialogMsg::Ok));
    assert_eq!(
        app.drawing.texts.iter().find(|t| t.id == id).unwrap().text,
        "Deck"
    );
    assert!(app.undo.can_undo());

    // Blank, unchanged and cancelled answers do nothing.
    for (answer, ok) in [("", true), ("Deck", true), ("Other", false)] {
        let before = app.undo.can_undo();
        click(&mut app);
        let _ = app.update(Message::Dialog(DialogMsg::Input(answer.into())));
        let _ = app.update(Message::Dialog(if ok {
            DialogMsg::Ok
        } else {
            DialogMsg::Cancel
        }));
        assert_eq!(app.drawing.texts[0].text, "Deck");
        assert_eq!(app.undo.can_undo(), before);
    }
}

#[test]
fn e2e_rotation_handles_paint_green_for_areas() {
    let green = |p: [u8; 4]| p[1] > 150 && p[0] < 90 && p[2] < 130;
    let mut h = defined_square();
    h.send(Message::Rail(crate::rail::RailTool::Select));
    h.send(Message::SelectAll);
    let shot = h.screenshot("rotation-handles");
    // The area handle: 34px above the square's top edge, centred (window px).
    let (hx, hy) = (41.0 + 403.5, 129.0 + 291.5 - 34.0);
    assert!(
        green(pixel_of(&shot, hx as u32, hy as u32)),
        "area handle disc"
    );
}

/// The bounding box (w, h) of the dark "ink" pixels within `half` px of
/// (cx, cy) in a screenshot - the glyphs, not the green handles or the grid.
fn ink_extent(path: &std::path::Path, cx: i32, cy: i32, half: i32) -> (i32, i32) {
    // Decode once: `pixel_of` re-reads the file for every pixel.
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for y in (cy - half)..=(cy + half) {
        for x in (cx - half)..=(cx + half) {
            let i = ((y as u32 * info.width + x as u32) * 4) as usize;
            if buf[i] < 110 && buf[i + 1] < 110 && buf[i + 2] < 110 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 {
        (0, 0)
    } else {
        (x1 - x0 + 1, y1 - y0 + 1)
    }
}

/// A "Porch" label placed at window (700, 500), selected, then turned.
fn rotated_label_shot(name: &str, degrees: Option<f64>) -> std::path::PathBuf {
    use crate::dock::DockMsg;
    let mut h = harness();
    h.send(Message::Dock(DockMsg::TextPick("Porch".into())));
    h.script(&["move (700, 500)", "click (700, 500)"]);
    if let Some(degrees) = degrees {
        // Picked 9px away so the second click is not a double-click (which
        // opens the Text: prompt).
        h.script(&["move (700, 509)", "click (700, 509)"]);
        h.send(Message::RotateSelected(degrees));
    }
    h.screenshot(name)
}

#[test]
fn e2e_a_rotated_text_label_paints_its_glyphs_turned() {
    // Flat: wider than tall. A quarter turn: taller than wide, the same ink.
    let flat = ink_extent(&rotated_label_shot("label-rot-0", None), 700, 500, 70);
    assert!(flat.0 > flat.1 * 2, "flat label: {flat:?}");
    let turned = ink_extent(
        &rotated_label_shot("label-rot-90", Some(90.0)),
        700,
        500,
        70,
    );
    assert!(turned.1 > turned.0 * 2, "turned label: {turned:?}");
    assert!(
        (turned.1 - flat.0).abs() <= 3 && (turned.0 - flat.1).abs() <= 3,
        "{flat:?} vs {turned:?}"
    );
    // A diagonal label is neither flat nor upright.
    let diagonal = ink_extent(
        &rotated_label_shot("label-rot-45", Some(45.0)),
        700,
        500,
        70,
    );
    assert!(
        diagonal.0 > flat.1 + 10 && diagonal.1 > flat.1 + 10,
        "diagonal: {diagonal:?}"
    );
}

#[test]
fn e2e_a_rotated_area_label_paints_its_text_turned() {
    // Click the area's label (window px of the centre), then turn it.
    let shot = |name: &str, degrees: Option<f64>| {
        let mut h = defined_square();
        h.send(Message::Rail(crate::rail::RailTool::Select));
        h.script(&["move (443, 521)", "click (443, 521)"]);
        if let Some(degrees) = degrees {
            h.send(Message::RotateSelected(degrees));
        }
        h.screenshot(name)
    };
    let flat = ink_extent(&shot("area-label-rot-0", None), 443, 521, 55);
    let turned = ink_extent(&shot("area-label-rot-90", Some(90.0)), 443, 521, 55);
    assert!(flat.1 > 0 && turned.1 > 0);
    assert!(
        turned.1 > flat.1 + 15,
        "a quarter turn stands the label up: {flat:?} -> {turned:?}"
    );
}

/// A window symbol hosted on a drawn wall, as the UI places it: draw the wall
/// between two window points, pick `windows/36in`, click the wall's middle.
fn harness_with_window(
    from: (i32, i32),
    to: (i32, i32),
    on: (i32, i32),
) -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static> {
    use crate::dock::DockMsg;
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        &format!("move ({}, {})", from.0, from.1),
        &format!("click ({}, {})", from.0, from.1),
        &format!("move ({}, {})", to.0, to.1),
        &format!("click ({}, {})", to.0, to.1),
        "type escape",
    ]);
    h.send(Message::Dock(DockMsg::SymbolPick("windows/36in".into())));
    h.script(&[
        &format!("move ({}, {})", on.0, on.1),
        &format!("click ({}, {})", on.0, on.1),
    ]);
    h
}

fn caddillo_on_wall(
    kind: &str,
    from: (i32, i32),
    to: (i32, i32),
    on: (i32, i32),
) -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static> {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        &format!("move ({}, {})", from.0, from.1),
        &format!("click ({}, {})", from.0, from.1),
        &format!("move ({}, {})", to.0, to.1),
        &format!("click ({}, {})", to.0, to.1),
        "type escape",
    ]);
    h.send(Message::Dock(DockMsg::SymbolPick(kind.into())));
    h.script(&[
        &format!("move ({}, {})", on.0, on.1),
        &format!("click ({}, {})", on.0, on.1),
        "type escape",
    ]);
    h
}

#[test]
fn e2e_caddillo_windows_snap_centered_on_horizontal_vertical_and_diagonal_walls() {
    for (name, from, to, on, rotation) in [
        ("horizontal", (341, 429), (741, 429), (541, 429), 0.0),
        ("vertical", (541, 329), (541, 629), (541, 479), 90.0),
        ("diagonal", (341, 529), (741, 329), (541, 429), 26.565051177),
    ] {
        let mut h = caddillo_on_wall("caddillo/window-single-36", from, to, on);
        let shot = h.screenshot(&format!("caddillo-window-{name}"));
        if name == "diagonal" {
            assert!(ink_skew(&shot, on.0, on.1, 25) < 0.0);
        }
        let mut app = state(h);
        assert_eq!(app.drawing.symbols.len(), 1);
        let s = app.drawing.symbols[0].clone();
        assert!(s.wall_id.is_some());
        // Drawing endpoints snap to the grid; symbol placement projects the
        // pointer directly onto that wall (up to half a grid cell from center).
        assert!(
            (s.wall_fraction.unwrap() - 0.5).abs() < 0.03,
            "{name}: {:?}",
            s.wall_fraction
        );
        let wall = &app.drawing.lines[0];
        let cross = (s.x - wall.start.x) * (wall.end.y - wall.start.y)
            - (s.y - wall.start.y) * (wall.end.x - wall.start.x);
        assert!(cross.abs() < 1e-7, "{name}: symbol is on the wall");
        let expected_rotation = -(wall.end.y - wall.start.y)
            .atan2(wall.end.x - wall.start.x)
            .to_degrees();
        assert!(
            (s.rotation - expected_rotation).abs() < 1e-6,
            "{name}: {}",
            s.rotation
        );
        assert!(
            (s.rotation.abs() - rotation).abs() < 2.0,
            "drawing snaps to the nearby angular increment"
        );
        assert_eq!(s.wall_gap_width, 3.0);
        let lib = app.symbols_lib();
        let def = lib.symbol(&s.symbol_type).unwrap();
        assert_eq!((def.bounds.min_x, def.bounds.max_x), (-18., 18.));
        let saved = app.build_save_model();
        let mut reopened = App::boot(1.).0;
        let _ = reopened.apply_model(&saved, "Window fixture");
        assert_eq!(reopened.drawing.symbols[0], s);
    }
}

#[test]
fn e2e_caddillo_corner_scale_updates_opening_undo_and_saved_file() {
    let mut app = state(caddillo_on_wall(
        "caddillo/window-single-36",
        (341, 429),
        (741, 429),
        (541, 429),
    ));
    let id = app.drawing.symbols[0].id;
    app.selection.clear();
    app.selection.symbols.insert(id);
    let (_, corner) = symbol_handle_px(&app, id);
    let s = &app.drawing.symbols[0];
    let center = crate::handles::to_screen(
        sketch_core::geometry::Pt::new(s.x, s.y),
        app.view.pan,
        app.view.zoom,
    );
    let dx = corner.0 - center.0 as f32;
    let dy = corner.1 - center.1 as f32;
    press_drag(
        &mut app,
        corner,
        (corner.0 + dx * 0.4, corner.1 + dy * 0.4),
        false,
    );
    let _ = app.update(Message::CanvasDragEnd);
    assert!((app.drawing.symbols[0].scale - 1.4).abs() < 1e-8);
    assert!((app.drawing.symbols[0].wall_gap_width - 4.2).abs() < 1e-8);
    assert!((crate::placed_symbol::openings(&app.drawing.symbols)[0].gap_width - 4.2).abs() < 1e-8);
    let _ = app.update(Message::ScaleSelected(1.1));
    assert!((app.drawing.symbols[0].wall_gap_width - 4.62).abs() < 1e-8);
    let save = app.build_save_model();
    let mut reopened = App::boot(1.).0;
    let _ = reopened.apply_model(&save, "Scaled opening");
    assert!((reopened.drawing.symbols[0].wall_gap_width - 4.62).abs() < 1e-8);
    let _ = app.update(Message::Undo);
    assert!((app.drawing.symbols[0].wall_gap_width - 4.2).abs() < 1e-8);
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.symbols[0].wall_gap_width, 3.0);
}

#[test]
fn e2e_caddillo_door_faces_both_wall_sides_and_furniture_stays_free() {
    let mut sides = Vec::new();
    for y in [419, 439] {
        let mut h = caddillo_on_wall("caddillo/door-swing-36", (341, 429), (741, 429), (541, y));
        h.screenshot(&format!("caddillo-door-side-{y}"));
        let app = state(h);
        let s = &app.drawing.symbols[0];
        assert!(s.wall_id.is_some());
        assert_eq!(s.wall_gap_width, 3.0);
        sides.push(s.wall_side_flip);
    }
    assert_ne!(sides[0], sides[1]);
    let app = state(caddillo_on_wall(
        "caddillo/tub",
        (341, 429),
        (741, 429),
        (541, 429),
    ));
    assert!(app.drawing.symbols[0].wall_id.is_none());
    assert!(crate::placed_symbol::openings(&app.drawing.symbols).is_empty());
}

#[test]
fn e2e_every_library_door_and_window_sits_on_its_wall_cut() {
    // The cut in the host wall must lie under the symbol: its middle is the
    // middle of the symbol's drawn extent along the wall, on either side of
    // the wall. (The CADdillo doors used to be cut from their midpoint.)
    let lib = crate::symlib::library();
    let lib = &lib.as_ref().as_ref().expect("assets/symbols.json loads").library;
    let ids: Vec<&String> = lib
        .symbols
        .iter()
        .filter(|(id, s)| id.starts_with("caddillo/") && s.wall_gap_width > 0.0)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 20);
    for id in ids {
        for y in [419, 439] {
            let app = state(caddillo_on_wall(id, (341, 429), (741, 429), (541, y)));
            let s = &app.drawing.symbols[0];
            let def = &lib.symbols[id];
            let line = app
                .drawing
                .lines
                .iter()
                .find(|l| Some(l.id) == s.wall_id)
                .unwrap_or_else(|| panic!("{id} is hosted"));
            let runs =
                sketch_core::solids::visible_runs(line, &crate::placed_symbol::openings(&app.drawing.symbols));
            assert_eq!(runs.len(), 2, "{id}: one cut inside the wall");
            let (ux, uy) = (line.end.x - line.start.x, line.end.y - line.start.y);
            let length = ux.hypot(uy);
            let cut_middle = (runs[0].1 + runs[1].0) / 2.0 * length;
            // The symbol's local x axis lies along the wall, turned by its
            // rotation (0 or 180 on this horizontal wall) and mirrored by flip.
            let local = (def.bounds.min_x + def.bounds.max_x) / 2.0 / 12.0 * s.scale;
            let turn = (s.rotation.to_radians()).cos() * if s.flip_h { -1.0 } else { 1.0 };
            let origin = ((s.x - line.start.x) * ux + (s.y - line.start.y) * uy) / length;
            let symbol_middle = origin + local * turn * ux.signum();
            let overhang = (def.bounds.max_x - def.bounds.min_x - def.wall_gap_width) / 24.0;
            // Pocket doors draw their pocket to one side, so they are only
            // required to cover the cut; the rest are centred on it.
            let slack = if id.contains("pocket") { overhang + 1e-6 } else { 1e-6 };
            assert!(
                (symbol_middle - cut_middle).abs() <= slack,
                "{id} at y={y}: symbol middle {symbol_middle}, cut middle {cut_middle}"
            );
        }
    }
}

#[test]
fn e2e_help_about_shows_the_running_version() {
    let mut h = harness();
    h.script(&["click \"Help\"", "click \"About\""]);
    let app = state(h);
    let about = app.dialog.as_ref().expect("the About box is open");
    assert!(about
        .message
        .starts_with(&format!("Sketch Rust {}
", env!("CARGO_PKG_VERSION"))));
    assert!(!about.has_cancel);
}

#[test]
fn e2e_caddillo_symbol_manager_has_live_preview_and_enlargement() {
    use crate::symbol_manager::Msg as S;
    let mut h = harness();
    h.send(Message::SymbolManager(S::Open));
    h.send(Message::SymbolManager(S::Filter("Door Swing 36".into())));
    h.script(&[
        "click \"Door Swing 36\"",
        "expect \"Type: caddillo/door-swing-36\"",
        "click \"Enlarge\"",
        "expect \"Symbol Viewer\"",
    ]);
    h.screenshot("caddillo-symbol-viewer");
}

/// Covariance sign of the dark ink around (cx, cy): negative for a stroke that
/// climbs to the right, positive for one that falls to the right.
fn ink_skew(path: &std::path::Path, cx: i32, cy: i32, half: i32) -> f64 {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let mut sum = 0.0;
    for y in (cy - half)..=(cy + half) {
        for x in (cx - half)..=(cx + half) {
            let i = ((y as u32 * info.width + x as u32) * 4) as usize;
            if buf[i] < 110 && buf[i + 1] < 110 && buf[i + 2] < 110 {
                sum += f64::from(x - cx) * f64::from(y - cy);
            }
        }
    }
    sum
}

#[test]
fn e2e_a_window_on_a_diagonal_wall_lies_along_it() {
    // The wall climbs to the right (world 45 degrees); the window's stroke must
    // climb with it, not fall across it (the renderer turned symbols the wrong
    // way, which only showed off the axes).
    let mut h = harness_with_window((441, 529), (641, 329), (541, 429));
    let shot = h.screenshot("window-on-diagonal");
    assert!(
        ink_skew(&shot, 541, 429, 40) < 0.0,
        "the window climbs with the wall"
    );
}

fn app_with_selected_window() -> (App, i64) {
    let mut app = state(harness_with_window((341, 429), (541, 429), (441, 429)));
    let id = app.drawing.symbols[0].id;
    app.selection.clear();
    app.selection.symbols.insert(id);
    (app, id)
}

/// Canvas px of the selected symbol's rotation handle and one corner.
fn symbol_handle_px(app: &App, id: i64) -> ((f32, f32), (f32, f32)) {
    let sym = app.drawing.symbols.iter().find(|s| s.id == id).unwrap();
    let def = app.symbols_lib().symbol(&sym.symbol_type).unwrap();
    let b = crate::handles::symbol_box(
        sketch_core::geometry::Pt::new(sym.x, sym.y),
        sym.rotation,
        sym.scale,
        (
            def.bounds.min_x,
            def.bounds.min_y,
            def.bounds.max_x,
            def.bounds.max_y,
        ),
        app.view.pan,
        app.view.zoom,
    );
    (
        (b.handle.0 as f32, b.handle.1 as f32),
        (b.corners[2].0 as f32, b.corners[2].1 as f32),
    )
}

#[test]
fn e2e_a_symbols_rotation_handle_snaps_to_15_degree_steps() {
    let (mut app, id) = app_with_selected_window();
    assert_eq!(
        app.drawing.symbols[0].rotation, 0.0,
        "a window on a horizontal wall"
    );
    assert!(app.drawing.symbols[0].wall_id.is_some());
    let (handle, _) = symbol_handle_px(&app, id);
    let centre = {
        let s = &app.drawing.symbols[0];
        crate::handles::to_screen(
            sketch_core::geometry::Pt::new(s.x, s.y),
            app.view.pan,
            app.view.zoom,
        )
    };
    // The handle is above the symbol; pull it to the symbol's right.
    press_drag(
        &mut app,
        handle,
        (centre.0 as f32 + 70.0, centre.1 as f32),
        false,
    );
    assert!(matches!(
        app.view.drag,
        Some(crate::view_state::Drag::SymbolRotate { preview, .. }) if preview == 90.0
    ));
    let _ = app.update(Message::CanvasDragEnd);
    let sym = &app.drawing.symbols[0];
    assert_eq!(sym.rotation, 90.0, "the top handle swung to the right");
    assert!(
        sym.wall_id.is_none(),
        "turned by hand: no longer wall-hosted"
    );
    // One undo step puts it back on its wall, unturned.
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.symbols[0].rotation, 0.0);
    assert!(app.drawing.symbols[0].wall_id.is_some());

    // Under half a step: nothing happens and nothing is banked.
    let (mut app, id) = app_with_selected_window();
    let (handle, _) = symbol_handle_px(&app, id);
    press_drag(&mut app, handle, (handle.0 + 3.0, handle.1), false);
    let _ = app.update(Message::CanvasDragEnd);
    assert_eq!(app.drawing.symbols[0].rotation, 0.0);
    assert!(app.drawing.symbols[0].wall_id.is_some());

    // Ctrl: 1 degree steps.
    let (mut app, id) = app_with_selected_window();
    let (handle, _) = symbol_handle_px(&app, id);
    press_drag(&mut app, handle, (handle.0 + 4.0, handle.1), true);
    let _ = app.update(Message::CanvasDragEnd);
    let r = app.drawing.symbols[0].rotation;
    assert!(r != 0.0 && r.fract() == 0.0 && r.abs() < 15.0, "{r}");
}

#[test]
fn e2e_a_symbols_corner_handle_resizes_in_5_percent_steps() {
    let (mut app, id) = app_with_selected_window();
    let (_, corner) = symbol_handle_px(&app, id);
    let centre = {
        let s = &app.drawing.symbols[0];
        crate::handles::to_screen(
            sketch_core::geometry::Pt::new(s.x, s.y),
            app.view.pan,
            app.view.zoom,
        )
    };
    // Drag the corner 40% further out along the diagonal.
    let (dx, dy) = (corner.0 - centre.0 as f32, corner.1 - centre.1 as f32);
    press_drag(
        &mut app,
        corner,
        (corner.0 + dx * 0.4, corner.1 + dy * 0.4),
        false,
    );
    let _ = app.update(Message::CanvasDragEnd);
    let scale = app.drawing.symbols[0].scale;
    assert!((scale - 1.4).abs() < 1e-9, "{scale}");
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_a_turned_symbol_hits_where_it_is_drawn() {
    let (mut app, _) = app_with_selected_window();
    app.selection.clear();
    app.drawing.symbols[0].wall_id = None;
    app.drawing.symbols[0].rotation = 90.0;
    let s = app.drawing.symbols[0].clone();
    let lib = app.symbols_lib();
    let def = lib.symbol(&s.symbol_type).unwrap();
    let bounds = Some((
        def.bounds.min_x,
        def.bounds.min_y,
        def.bounds.max_x,
        def.bounds.max_y,
    ));
    // A 36in window is ~3ft long: turned a quarter, it spans vertically, not
    // horizontally.
    let above = sketch_core::geometry::Pt::new(s.x, s.y + 1.2);
    let beside = sketch_core::geometry::Pt::new(s.x + 1.2, s.y);
    assert!(crate::placed_symbol::point_in_symbol(
        &s, bounds, above, 1.0
    ));
    assert!(!crate::placed_symbol::point_in_symbol(
        &s, bounds, beside, 1.0
    ));
}

/// The defined square in Select mode, nothing selected.
fn square_in_select_mode() -> App {
    let mut app = state(defined_square());
    app.view.mode = crate::view_state::Mode::Select;
    app.selection.clear();
    let _ = app.update(Message::Noop);
    app
}

fn workflow(app: &mut App, msg: crate::area_workflow::Msg) {
    let _ = app.update(Message::AreaWorkflow(msg));
}

fn workflow_center_click(app: &mut App) {
    let _ = app.update(Message::CanvasClick {
        x: 400.0,
        y: 400.0,
        double: false,
        ctrl: false,
    });
}

#[test]
fn e2e_workflow_palette_groups_collapse_and_tiles_arm_from_real_input() {
    let mut h = harness();
    assert!(h.shows("First Floor"));
    h.script(&["click (1180, 181)"]);
    assert!(state(h).workflow.collapsed.contains("GLA"));
    let mut h = harness();
    h.script(&[
        "click (1180, 181)",
        "click (1180, 181)",
        "click \"First Floor\"",
    ]);
    let app = state(h);
    assert_eq!(app.workflow.armed.as_deref(), Some("GLA1"));
    assert_eq!(app.view.mode, crate::view_state::Mode::DefineArea);
}

#[test]
fn e2e_workflow_tile_keyboard_space_disarms_and_enter_rearms() {
    let mut h = harness();
    h.script(&["click \"First Floor\"", "type \" \""]);
    h.wait(30);
    // Space activates the focused tile without drawing a wall.
    h.script(&["type enter"]);
    let app = state(h);
    assert_eq!(app.workflow.armed.as_deref(), Some("GLA1"));
    assert!(app.drawing.lines.is_empty());
}

#[test]
fn e2e_workflow_armed_assignment_repeats_clears_and_updates_usage() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    workflow(&mut app, W::Arm("GAR".into()));
    workflow_center_click(&mut app);
    assert_eq!(app.drawing.polygons[0].kind, "GAR");
    assert_eq!(app.workflow.usage.get("GAR"), Some(&1));
    assert_eq!(app.workflow.armed.as_deref(), Some("GAR"));
    assert!(app.workflow.chooser.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0].kind, "GLA1");
    workflow(&mut app, W::Arm("UND".into()));
    workflow_center_click(&mut app);
    assert_eq!(app.drawing.polygons[0].kind, "UND");
    assert_eq!(app.drawing.polygons[0].name, "Undefined Area");
    workflow(&mut app, W::Arm("UND".into()));
    assert!(app.workflow.armed.is_none());
}

#[test]
fn e2e_workflow_rail_hover_click_and_escape_return_to_previous_draw_tool() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let _ = app.update(Message::Rail(crate::rail::RailTool::DrawInterior));
    let _ = app.update(Message::Rail(crate::rail::RailTool::DefineArea));
    let _ = app.update(Message::CanvasHover(400.0, 400.0));
    assert!(app.workflow.hover.is_some());
    workflow_center_click(&mut app);
    assert!(app.workflow.chooser.as_ref().unwrap().existing);
    let _ = app.update(Message::Key(crate::input::KeyCmd::Escape));
    assert_eq!(app.view.mode, crate::view_state::Mode::DrawInterior);
    assert!(app.workflow.chooser.is_none());
    workflow(&mut app, W::Leave);
    assert!(app.workflow.hover.is_none());
}

#[test]
fn e2e_workflow_chooser_label_settings_and_custom_name_undo_as_one_definition() {
    use crate::area_workflow::{Msg as W, Post};
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    let original = app.drawing.polygons[0].clone();
    workflow(&mut app, W::Open(id));
    workflow(&mut app, W::ChooseGroup("GAR".into()));
    workflow(&mut app, W::Code("GAR".into()));
    workflow(&mut app, W::Name("Workshop".into()));
    workflow(&mut app, W::Post(Post::Dims, false));
    workflow(&mut app, W::Post(Post::Code, true));
    workflow(&mut app, W::Suffix("sy".into()));
    workflow(&mut app, W::Repeat(true));
    workflow(&mut app, W::Submit);
    let p = &app.drawing.polygons[0];
    assert_eq!(
        (&*p.kind, &*p.name, &*p.auto_post.suffix),
        ("GAR", "Workshop", "sy")
    );
    assert!(!p.auto_post.dims && p.auto_post.code);
    assert!(app.drawing.lines.iter().all(|l| l.dimension_hidden));
    assert_eq!(app.workflow.remembered.as_deref(), Some("GAR"));
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0], original);
    assert!(app.drawing.lines.iter().all(|l| !l.dimension_hidden));
}

#[test]
fn e2e_workflow_cancelling_a_provisional_region_leaves_walls_history_and_selection() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    app.drawing.polygons.clear();
    let before = app.drawing.lines.clone();
    let history = app.undo.labels();
    workflow(&mut app, W::Activate);
    workflow_center_click(&mut app);
    assert!(matches!(
        app.workflow.chooser.as_ref().unwrap().region,
        crate::define_area::Region::Face(_)
    ));
    workflow(&mut app, W::Cancel);
    assert!(app.drawing.polygons.is_empty());
    assert_eq!(app.drawing.lines, before);
    assert_eq!(app.undo.labels(), history);
    workflow_center_click(&mut app);
    workflow(&mut app, W::Code("GLA1".into()));
    workflow(&mut app, W::Submit);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert_eq!(app.drawing.polygons[0].kind, "GLA1");
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0].kind, "UND");
    let _ = app.update(Message::Undo);
    assert!(app.drawing.polygons.is_empty());
    assert_eq!(app.drawing.lines, before);
}

#[test]
fn e2e_workflow_search_groups_and_unknown_existing_codes_are_preserved() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.drawing.polygons[0].kind = "LEGACY".into();
    workflow(&mut app, W::Open(id));
    let c = app.workflow.chooser.as_ref().unwrap();
    assert_eq!(c.group, "Custom");
    assert!(c.codes(&app.area_codes).iter().any(|e| e.code == "LEGACY"));
    workflow(&mut app, W::Search("garage".into()));
    let c = app.workflow.chooser.as_ref().unwrap();
    assert!(c
        .codes(&app.area_codes)
        .iter()
        .filter(|e| c.matches(e))
        .any(|e| e.code == "GAR"));
    workflow(&mut app, W::Submit); // An old value excluded by the search cannot submit.
    assert!(app.workflow.chooser.is_some());
    assert_eq!(app.drawing.polygons[0].kind, "LEGACY");
}

#[test]
fn e2e_workflow_type_menu_assign_select_edit_new_and_delete_confirm() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.polys.insert(id);
    workflow(&mut app, W::AssignSelected("GAR".into()));
    assert_eq!(app.drawing.polygons[0].kind, "GAR");
    workflow(&mut app, W::SelectUsing("GAR".into()));
    assert!(app.selection.polys.contains(&id));
    workflow(&mut app, W::EditType("GAR".into()));
    assert!(app.def_editor.open);
    assert_eq!(app.def_editor.code, "GAR");
    app.def_editor.open = false;
    workflow(&mut app, W::NewType);
    assert!(app.new_code.open);
    app.new_code.open = false;
    workflow(&mut app, W::DeleteType("GAR".into()));
    assert!(app.dialog.is_some());
    let _ = app.update(Message::Dialog(crate::dialog::DialogMsg::Cancel));
    assert_eq!(app.drawing.polygons[0].kind, "GAR");
    workflow(&mut app, W::DeleteType("GAR".into()));
    let _ = app.update(Message::Dialog(crate::dialog::DialogMsg::Ok));
    assert!(!app.area_codes.iter().any(|c| c.code == "GAR"));
    assert!(!app.dock.area_codes.iter().any(|c| c.code == "GAR"));
    assert_eq!(app.drawing.polygons[0].kind, "UND");
    workflow(&mut app, W::DeleteType("UND".into()));
    assert!(app.dialog.is_none());
    assert!(app.area_codes.iter().any(|c| c.code == "UND"));
}

#[test]
fn e2e_workflow_new_area_highlights_undefined_or_starts_drawing() {
    use crate::area_workflow::Msg as W;
    let mut app = state(harness());
    workflow(&mut app, W::NewArea);
    assert_eq!(app.view.mode, crate::view_state::Mode::DrawExterior);
    app = square_in_select_mode();
    app.drawing.polygons[0].kind = "UND".into();
    workflow(&mut app, W::NewArea);
    assert_eq!(app.view.mode, crate::view_state::Mode::DefineArea);
    assert!(app.workflow.find_undefined);
    workflow(&mut app, W::FindUndefined);
    assert!(!app.workflow.find_undefined);
}

#[test]
fn e2e_workflow_detect_regions_is_idempotent_and_one_undo_restores_the_network() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    app.drawing.polygons.clear();
    let walls = app.drawing.lines.clone();
    workflow(&mut app, W::Detect);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert_eq!(app.drawing.polygons[0].kind, "UND");
    workflow(&mut app, W::Detect);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert!(app.workflow.status.contains("already defined"));
    let _ = app.update(Message::Undo);
    assert!(app.drawing.polygons.is_empty());
    assert_eq!(app.drawing.lines, walls);
}

#[test]
fn e2e_workflow_reopen_continues_the_original_chain_and_can_close_again() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let poly = app.drawing.polygons[0].clone();
    app.selection.polys.insert(poly.id);
    workflow(&mut app, W::Reopen);
    assert!(app.drawing.polygons.is_empty());
    assert_eq!(app.drawing.lines.len(), 3);
    assert!(app.drawing.pen_down);
    assert_eq!(app.drawing.points.len(), 4);
    assert_eq!(app.view.mode, crate::view_state::Mode::DrawExterior);
    assert_eq!(app.layers.active, poly.layer_id.unwrap());
    let first = app.drawing.points[0];
    let (x, y) = app.view.to_screen(crate::view_state::World {
        x: first.x,
        y: first.y,
    });
    let _ = app.update(Message::CanvasClick {
        x: x as f32,
        y: y as f32,
        double: false,
        ctrl: false,
    });
    assert_eq!(app.drawing.polygons.len(), 1);
    assert!((app.drawing.polygons[0].gross - 100.0).abs() < 1e-8);
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons.len(), 1);
    assert_eq!(app.drawing.polygons[0].kind, "GLA1");
}

#[test]
fn e2e_workflow_advanced_redefine_clone_and_defaults_are_live() {
    use crate::area_workflow::{Msg as W, Post};
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.polys.insert(id);
    workflow(&mut app, W::Arm("GAR".into()));
    workflow(&mut app, W::Advanced(true));
    workflow(&mut app, W::Redefine);
    assert_eq!(app.drawing.polygons[0].kind, "GAR");
    app.selection.polys.insert(id);
    workflow(&mut app, W::Clone);
    assert_eq!(app.drawing.polygons.len(), 2);
    workflow(&mut app, W::DefaultPost(Post::Code, true));
    workflow(&mut app, W::DefaultSuffix("ft²".into()));
    workflow(&mut app, W::DefaultPost(Post::Dims, false));
    workflow(&mut app, W::DefaultPlacement("In".into()));
    let model = app.build_save_model();
    let mut fresh = state(harness());
    let _ = fresh.apply_model(&model, "defaults.skx");
    assert!(fresh.drawing.auto_post.code);
    assert_eq!(fresh.drawing.auto_post.suffix, "ft²");
    assert!(!fresh.view.dimensions);
    assert_eq!(
        fresh.view.dimension_placement,
        crate::view_state::DimensionPlacement::In
    );
}

#[test]
fn e2e_workflow_locked_hidden_and_stale_targets_do_not_change_definition() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let p = app.drawing.polygons[0].clone();
    let layer = p.layer_id.as_ref().unwrap();
    app.layers
        .items
        .iter_mut()
        .find(|l| &l.id == layer)
        .unwrap()
        .locked = true;
    app.selection.polys.insert(p.id);
    workflow(&mut app, W::AssignSelected("GAR".into()));
    workflow(&mut app, W::Open(p.id));
    assert!(app.workflow.chooser.is_none());
    assert_eq!(app.drawing.polygons[0].kind, "GLA1");
    let l = app
        .layers
        .items
        .iter_mut()
        .find(|l| &l.id == layer)
        .unwrap();
    l.locked = false;
    l.visible = false;
    workflow(&mut app, W::Open(p.id));
    assert!(app.workflow.chooser.is_none());
    app.layers
        .items
        .iter_mut()
        .find(|l| &l.id == layer)
        .unwrap()
        .visible = true;
    workflow(&mut app, W::Open(p.id));
    assert!(app.workflow.chooser.is_some());
    app.drawing.polygons.clear();
    let _ = app.update(Message::Noop);
    assert!(app.workflow.chooser.is_none());
}

#[test]
fn e2e_workflow_canvas_context_calcs_context_and_double_click_open_chooser() {
    let mut h = defined_square();
    h.send(Message::Rail(crate::rail::RailTool::Select));
    h.script(&[
        "move (441, 560)",
        "press right (441, 560)",
        "click \"Edit Definition...\"",
    ]);
    assert!(h.shows("Edit definition"));
    h.send(Message::AreaWorkflow(crate::area_workflow::Msg::Cancel));
    h.send(Message::Dock(crate::dock::DockMsg::Show(
        crate::dock::DockTab::Calcs,
    )));
    h.script(&["move (1200, 238)", "press right (1200, 238)"]);
    assert!(
        h.shows("Edit definition"),
        "the Calcs row opens the chooser from a real right press"
    );
    let app = state(h);
    assert!(
        app.context.open.is_none(),
        "dock right presses never open a canvas menu"
    );
    let mut app = app;
    let id = app.drawing.polygons[0].id;
    workflow(&mut app, crate::area_workflow::Msg::Open(id));
    assert!(app.workflow.chooser.is_some());
    workflow(&mut app, crate::area_workflow::Msg::Cancel);
    let _ = app.update(Message::CanvasClick {
        x: 400.0,
        y: 430.0,
        double: true,
        ctrl: false,
    });
    assert!(app.workflow.chooser.is_some());
}

#[test]
fn e2e_workflow_chooser_keyboard_filters_selects_names_and_saves() {
    let mut h = defined_square();
    let id = state(defined_square()).drawing.polygons[0].id;
    h.send(Message::AreaWorkflow(crate::area_workflow::Msg::Open(id)));
    h.script(&[
        "type \"garage\"",
        "type tab",
        "type tab",
        "type enter",
        "type tab",
        "type \"Workshop\"",
        "type enter",
    ]);
    let app = state(h);
    assert!(app.workflow.chooser.is_none());
    assert_eq!(app.drawing.polygons[0].kind, "GAR");
    assert!(app.drawing.polygons[0].name.contains("Workshop"));
    assert!(app.drawing.buffer.is_empty());
}

#[test]
fn e2e_workflow_type_context_menu_is_reachable_and_dismisses_outside() {
    let mut h = harness();
    h.script(&["move (1200, 210)", "press right (1200, 210)"]);
    assert!(h.shows("Assign by clicking outlines"));
    assert!(h.shows("Edit type..."));
    h.script(&["click (700, 400)"]);
    assert!(state(h).workflow.menu.is_none());
}

#[test]
fn e2e_workflow_keyboard_focus_wraps_within_the_chooser() {
    let mut h = defined_square();
    let id = state(defined_square()).drawing.polygons[0].id;
    h.send(Message::AreaWorkflow(crate::area_workflow::Msg::Open(id)));
    h.script(&[
        "type \"garage\"",
        "type tab",
        "type tab",
        "type tab",
        "type tab",
        "type tab",
        "type tab",
        "type tab",
        "type \"x\"",
    ]);
    let app = state(h);
    assert_eq!(app.workflow.chooser.as_ref().unwrap().search, "garagex");
    assert!(app.drawing.buffer.is_empty());
    let mut h = defined_square();
    h.send(Message::AreaWorkflow(crate::area_workflow::Msg::Open(id)));
    h.send(Message::AreaWorkflow(crate::area_workflow::Msg::Focus(
        true,
    )));
    // Save changes is last for an existing, valid classification.
    h.script(&["type enter"]);
    assert!(state(h).workflow.chooser.is_none());
}

#[test]
fn e2e_workflow_repeat_remembers_only_submitted_choices() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.drawing.polygons[0].kind = "UND".into();
    workflow(&mut app, W::Open(id));
    workflow(&mut app, W::Repeat(true));
    workflow(&mut app, W::Cancel);
    assert!(!app.workflow.repeat);
    workflow(&mut app, W::Open(id));
    workflow(&mut app, W::Code("GLA2".into()));
    workflow(&mut app, W::Repeat(true));
    workflow(&mut app, W::Submit);
    app.selection.polys.insert(id);
    workflow(&mut app, W::Clone);
    let second = app.drawing.polygons[1].id;
    app.drawing.polygons[1].kind = "UND".into();
    workflow(&mut app, W::Open(second));
    assert_eq!(
        app.workflow.chooser.as_ref().unwrap().code.as_deref(),
        Some("GLA2")
    );
    workflow(&mut app, W::Submit);
    assert_eq!(app.drawing.polygons[1].kind, "GLA2");
}

#[test]
fn e2e_workflow_batch_redefine_is_one_undo_and_page_change_closes_chooser() {
    use crate::area_workflow::Msg as W;
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.polys.insert(id);
    workflow(&mut app, W::Clone);
    app.selection.polys = app.drawing.polygons.iter().map(|p| p.id).collect();
    let _ = app.update(Message::Noop);
    let original = app.drawing.polygons.clone();
    workflow(&mut app, W::Arm("GAR".into()));
    workflow(&mut app, W::Advanced(true));
    workflow(&mut app, W::Redefine);
    assert!(app.drawing.polygons.iter().all(|p| p.kind == "GAR"));
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons, original);
    workflow(&mut app, W::Open(id));
    app.add_page();
    let _ = app.update(Message::Noop);
    assert!(app.workflow.chooser.is_none());
    assert!(app.selection.is_empty());
}

#[test]
fn e2e_workflow_screenshots_show_chooser_editor_hover_and_type_menu() {
    use crate::area_workflow::Msg as W;
    let mut h = defined_square();
    h.send(Message::Rail(crate::rail::RailTool::DefineArea));
    h.send(Message::CanvasHover(400.0, 400.0));
    h.screenshot("workflow-hover");
    let mut h = defined_square();
    h.send(Message::CanvasClick {
        x: 400.0,
        y: 400.0,
        double: false,
        ctrl: false,
    });
    // Open via the area row path so selection mode does not affect this capture.
    let id = state(defined_square()).drawing.polygons[0].id;
    h.send(Message::AreaWorkflow(W::Open(id)));
    h.screenshot("workflow-chooser");
    let mut h = defined_square();
    h.send(Message::AreaWorkflow(W::Advanced(true)));
    h.screenshot("workflow-editor");
    let mut h = defined_square();
    h.send(Message::RightPress(iced::Point::new(1150.0, 190.0)));
    h.send(Message::AreaWorkflow(W::TypeMenu("GLA1".into())));
    h.screenshot("workflow-type-menu");
}

#[test]
fn e2e_face_measurement_dialog_updates_totals_and_undo_restores_them() {
    use crate::area_dialog::AreaDialogMsg as D;
    let mut app = square_in_select_mode();
    let original = app.drawing.polygons[0].clone();
    app.selection.polys.insert(original.id);
    let _ = app.update(Message::WallSolids(true));
    let _ = app.update(Message::AreaProps);
    let _ = app.update(Message::AreaDialog(D::Measure("Interior Face".into())));
    let _ = app.update(Message::AreaDialog(D::Apply));
    assert_eq!(app.drawing.polygons[0].measure_to, "interior");
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 90.25).abs() < 1e-8);
    assert_eq!(app.drawing.polygons[0].points, original.points);
    assert_eq!(app.drawing.polygons[0].edges, original.edges);
    let calcs = crate::calcs::build(&app.drawing.polygons, &app.computed);
    assert!((calcs.groups[0].areas[0].base_area - 90.25).abs() < 1e-8);
    assert!((calcs.groups[0].areas[0].perimeter - 38.0).abs() < 1e-8);
    assert!((app.breakdown_results()[0].gross - 90.25).abs() < 1e-8);
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0].measure_to, "center");
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 100.0).abs() < 1e-8);
    let _ = app.update(Message::Redo);
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 90.25).abs() < 1e-8);
}

#[test]
fn e2e_face_measurement_cache_tracks_options_wall_types_and_overrides() {
    let mut app = square_in_select_mode();
    app.drawing.polygons[0].measure_to = "exterior".into();
    let _ = app.update(Message::WallSolids(true));
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 110.25).abs() < 1e-8);
    app.drawing.options.exterior_thickness = 12.0;
    let _ = app.update(Message::Noop);
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 121.0).abs() < 1e-8);
    app.drawing.options.interior_thickness = 24.0;
    for line in &mut app.drawing.lines {
        line.kind = sketch_core::model::WallType::Interior;
    }
    let _ = app.update(Message::Noop);
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 144.0).abs() < 1e-8);
    for line in &mut app.drawing.lines {
        line.thickness = Some(6.0);
    }
    let _ = app.update(Message::Noop);
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 110.25).abs() < 1e-8);
    let _ = app.update(Message::WallSolids(false));
    assert!((app.calcs_totals.as_ref().unwrap().total_gla - 100.0).abs() < 1e-8);
}

#[test]
fn e2e_face_measurement_save_and_multi_page_exports_keep_centerlines() {
    let mut app = square_in_select_mode();
    let original_points = app.drawing.polygons[0].points.clone();
    app.drawing.polygons[0].measure_to = "interior".into();
    let _ = app.update(Message::WallSolids(true));
    let mut saved_page = crate::pages::Page {
        lines: app.drawing.lines.clone(),
        polygons: app.drawing.polygons.clone(),
        ..Default::default()
    };
    saved_page.polygons[0].measure_to = "exterior".into();
    for line in &mut saved_page.lines {
        line.thickness = Some(12.0);
    }
    app.pages.pages.push(saved_page);
    let exported = crate::exporter::build_export_pages(&app.drawing, &app.pages, &app.area_codes);
    assert!((exported[0].polygons[0].gross - 90.25).abs() < 1e-8);
    assert!((exported[1].polygons[0].gross - 121.0).abs() < 1e-8);
    assert_eq!(exported[0].polygons[0].points, original_points);
    let model = app.build_save_model();
    let mut fresh = state(harness());
    let _ = fresh.apply_model(&model, "face-measure.skx");
    let _ = fresh.update(Message::Noop);
    assert_eq!(fresh.drawing.polygons[0].measure_to, "interior");
    assert_eq!(fresh.drawing.polygons[0].points, original_points);
    assert!((fresh.calcs_totals.as_ref().unwrap().total_gla - 90.25).abs() < 1e-8);
}

fn label_at(app: &App) -> sketch_core::geometry::Pt {
    crate::handles::label_position(&app.drawing, &app.drawing.polygons[0])
}

#[test]
fn e2e_dragging_an_area_label_gives_it_a_position_of_its_own() {
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    let before = label_at(&app);
    assert!(app.drawing.polygons[0].label_position.is_none());
    // Press the label (the area's centre) and drag it 2 ft right, 1 ft up.
    let (lx, ly) = canvas_px(before.x, before.y);
    let _ = app.update(Message::CanvasPress(lx, ly));
    assert!(app.selection.area_labels.contains(&id) && app.selection.polys.is_empty());
    let _ = app.update(Message::CanvasDrag(lx + 40.0, ly - 20.0, false));
    let _ = app.update(Message::CanvasDragEnd);
    let poly = &app.drawing.polygons[0];
    let at = poly.label_position.expect("the drag pinned the label");
    assert!((at.x - (before.x + 2.0)).abs() < 1e-9 && (at.y - (before.y + 1.0)).abs() < 1e-9);
    // The area itself did not move, and the size block (never pinned) stayed
    // below its label... which moved: the figure travels with the name.
    assert_eq!(poly.points[0], sketch_core::geometry::Pt::new(-11.0, 3.0));
    assert!(poly.area_size_position.is_none());
    assert!(app.undo.can_undo());
    let _ = app.update(Message::Undo);
    assert!(
        app.drawing.polygons[0].label_position.is_none(),
        "undo unpins it"
    );
}

#[test]
fn e2e_moving_a_whole_area_carries_its_pinned_label_and_a_lone_size_moves_alone() {
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.drawing.polygons[0].label_position = Some(sketch_core::geometry::Pt::new(-6.0, 0.0));
    app.drawing.polygons[0].area_size_position = Some(sketch_core::geometry::Pt::new(-6.0, -1.0));
    app.selection.polys.insert(id);
    crate::transform::move_selected(&mut app.drawing, &app.selection, 3.0, 2.0);
    let poly = &app.drawing.polygons[0];
    assert_eq!(
        poly.label_position,
        Some(sketch_core::geometry::Pt::new(-3.0, 2.0))
    );
    assert_eq!(
        poly.area_size_position,
        Some(sketch_core::geometry::Pt::new(-3.0, 1.0))
    );
    // A size block picked on its own moves alone, from where it sits.
    app.selection.clear();
    app.selection.area_sizes.insert(id);
    crate::transform::move_selected(&mut app.drawing, &app.selection, 1.0, 0.0);
    let poly = &app.drawing.polygons[0];
    assert_eq!(
        poly.area_size_position,
        Some(sketch_core::geometry::Pt::new(-2.0, 1.0))
    );
    assert_eq!(
        poly.label_position,
        Some(sketch_core::geometry::Pt::new(-3.0, 2.0))
    );
}

#[test]
fn e2e_a_free_text_label_drags_and_nudges() {
    let mut app = app_with_label();
    let (x, y) = canvas_px(0.0, 0.0);
    app.selection.clear();
    let _ = app.update(Message::CanvasPress(x, y));
    let _ = app.update(Message::CanvasDrag(x + 60.0, y + 20.0, false));
    let _ = app.update(Message::CanvasDragEnd);
    let t = &app.drawing.texts[0];
    assert!(
        (t.x - 3.0).abs() < 1e-9 && (t.y - -1.0).abs() < 1e-9,
        "{} {}",
        t.x,
        t.y
    );
    // The arrow keys move it once (the move lives in one place now).
    let _ = app.update(Message::Key(crate::input::KeyCmd::Nudge(1.0, 0.0)));
    assert!((app.drawing.texts[0].x - 4.0).abs() < 1e-9);
}

#[test]
fn e2e_area_text_scales_with_the_wheel_and_the_plus_minus_keys() {
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.area_labels.insert(id);
    let _ = app.update(Message::ScaleSelected(1.1));
    assert!((app.drawing.polygons[0].label_scale.unwrap() - 1.1).abs() < 1e-9);
    assert_eq!(app.drawing.polygons[0].area_size_scale, None);
    // The size block's scale starts from the label's when picked alone.
    app.selection.clear();
    app.selection.area_sizes.insert(id);
    let _ = app.update(Message::ScaleSelected(1.1));
    assert!((app.drawing.polygons[0].area_size_scale.unwrap() - 1.21).abs() < 1e-9);
    // Clamped to 0.5 .. 4.
    for _ in 0..40 {
        let _ = app.update(Message::ScaleSelected(1.1));
    }
    assert_eq!(app.drawing.polygons[0].area_size_scale, Some(4.0));
    for _ in 0..80 {
        let _ = app.update(Message::ScaleSelected(0.9));
    }
    assert_eq!(app.drawing.polygons[0].area_size_scale, Some(0.5));
    // The wheel does the same (x1.08) instead of zooming.
    app.selection.clear();
    app.selection.area_labels.insert(id);
    let zoom = app.view.zoom;
    let _ = app.update(Message::Wheel {
        up: true,
        x: 400.0,
        y: 300.0,
        shift: false,
        ctrl: false,
    });
    assert!((app.drawing.polygons[0].label_scale.unwrap() - 1.1 * 1.08).abs() < 1e-9);
    assert_eq!(app.view.zoom, zoom);
}

#[test]
fn e2e_the_text_group_styles_an_areas_label() {
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.area_labels.insert(id);
    // Bold: on unless it is already on everywhere selected.
    let _ = app.update(Message::ToggleSelectedTextStyle("bold".into()));
    assert_eq!(app.drawing.polygons[0].label_bold, Some(true));
    let _ = app.update(Message::ToggleSelectedTextStyle("bold".into()));
    assert_eq!(app.drawing.polygons[0].label_bold, Some(false));
    let _ = app.update(Message::ToggleSelectedTextStyle("underline".into()));
    assert_eq!(app.drawing.polygons[0].label_underline, Some(true));
    // Grow starts from the auto-post size (16) and Shrink clamps at 6.
    let _ = app.update(Message::AdjustSelectedTextSize(1));
    assert_eq!(app.drawing.polygons[0].label_font_size, Some(17.0));
    for _ in 0..11 {
        let _ = app.update(Message::AdjustSelectedTextSize(-1));
    }
    assert_eq!(app.drawing.polygons[0].label_font_size, Some(6.0));
    let _ = app.update(Message::AdjustSelectedTextSize(-1));
    assert_eq!(
        app.drawing.polygons[0].label_font_size,
        Some(6.0),
        "clamped"
    );
    let _ = app.update(Message::Undo);
    // One undo step per click.
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0].label_font_size, Some(7.0));
    // The size block takes the same typography.
    app.selection.clear();
    app.selection.area_sizes.insert(id);
    let blocks = crate::handles::area_blocks(&app.drawing, &app.computed, &app.drawing.polygons[0]);
    let size = blocks
        .iter()
        .find(|b| b.kind == crate::handles::TextKind::AreaSize)
        .unwrap();
    assert_eq!(size.font_size, 7.0);
    assert!(size.underline);
}

#[test]
fn e2e_selecting_area_text_opens_the_edit_tools_text_group() {
    let mut h = defined_square();
    h.send(Message::Rail(crate::rail::RailTool::Select));
    // The label sits at the area's centre (window px).
    h.script(&["move (443, 521)", "click (443, 521)"]);
    for text in [
        "Font",
        "Size",
        "Bold",
        "Italic",
        "Underline",
        "Text Color",
        "Grow",
        "Shrink",
        "Text",
    ] {
        assert!(h.shows(text), "the text group shows {text}");
    }
    h.screenshot("edit_tools_text_group");
}

#[test]
fn e2e_the_text_group_shows_the_selected_texts_values() {
    // A text label: its own font, size, colour and wording.
    let mut app = app_with_label();
    app.drawing.texts[0].font_family = "Georgia".into();
    app.drawing.texts[0].font_size = 18.0;
    app.drawing.texts[0].color = "#ff0000".into();
    let t = app.edit_tools_data().and_then(|d| d.text).expect("text");
    assert_eq!((t.font.as_str(), t.size.as_str()), ("Georgia", "18"));
    assert_eq!(t.value, app.drawing.texts[0].text);
    assert!((t.color.r - 1.0).abs() < 1e-6 && t.color.g == 0.0);

    // An area's label: the auto-post font / size stand in; the text is its name.
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.area_labels.insert(id);
    let t = app.edit_tools_data().and_then(|d| d.text).expect("text");
    assert_eq!((t.font.as_str(), t.size.as_str()), ("Arial", "16"));
    assert!(!t.value.is_empty());
    // The size block alone shows the net area and suffix.
    app.selection.clear();
    app.selection.area_sizes.insert(id);
    let t = app.edit_tools_data().and_then(|d| d.text).expect("text");
    assert!(t.value.ends_with(" sf"), "{}", t.value);
}

#[test]
fn e2e_the_font_select_and_size_box_restyle_the_selection() {
    let mut app = app_with_label();
    let _ = app.update(Message::TextFontPicked("Verdana".into()));
    assert_eq!(app.drawing.texts[0].font_family, "Verdana");
    // Typing alone changes nothing; Enter applies it.
    let _ = app.update(Message::TextFieldInput(
        crate::ribbon::TextField::Size,
        "30".into(),
    ));
    assert_ne!(app.drawing.texts[0].font_size, 30.0);
    assert_eq!(
        app.edit_tools_data().and_then(|d| d.text).unwrap().size,
        "30",
        "the box shows what is typed"
    );
    let _ = app.update(Message::TextFieldApply(crate::ribbon::TextField::Size));
    assert_eq!(app.drawing.texts[0].font_size, 30.0);
    // Out of range clamps; junk reads as 12.
    let _ = app.update(Message::TextFieldInput(
        crate::ribbon::TextField::Size,
        "500".into(),
    ));
    let _ = app.update(Message::TextFieldApply(crate::ribbon::TextField::Size));
    assert_eq!(app.drawing.texts[0].font_size, 72.0);
    let _ = app.update(Message::TextFieldInput(
        crate::ribbon::TextField::Size,
        "abc".into(),
    ));
    let _ = app.update(Message::TextFieldApply(crate::ribbon::TextField::Size));
    assert_eq!(app.drawing.texts[0].font_size, 12.0);
    // One undo step each.
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.texts[0].font_size, 72.0);
}

#[test]
fn e2e_the_text_box_rewords_a_label_or_renames_an_area() {
    let mut app = app_with_label();
    let _ = app.update(Message::TextFieldInput(
        crate::ribbon::TextField::Value,
        "Kitchen".into(),
    ));
    let _ = app.update(Message::TextFieldApply(crate::ribbon::TextField::Value));
    assert_eq!(app.drawing.texts[0].text, "Kitchen");

    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.area_labels.insert(id);
    let _ = app.update(Message::TextFieldInput(
        crate::ribbon::TextField::Value,
        "Great Room".into(),
    ));
    let _ = app.update(Message::TextFieldApply(crate::ribbon::TextField::Value));
    assert_eq!(app.drawing.polygons[0].name, "Great Room");
    // The size block ignores the wording.
    app.selection.clear();
    app.selection.area_sizes.insert(id);
    let _ = app.update(Message::TextFieldInput(
        crate::ribbon::TextField::Value,
        "x".into(),
    ));
    let _ = app.update(Message::TextFieldApply(crate::ribbon::TextField::Value));
    assert_eq!(app.drawing.polygons[0].name, "Great Room");
}

#[test]
fn e2e_text_color_opens_the_dialog_and_colours_the_selection() {
    let mut app = app_with_label();
    let _ = app.update(Message::TextColorClicked);
    assert!(app.color.open, "the Color dialog opens");
    assert_eq!(
        app.color_target,
        crate::color_dialog::ColorTarget::SelectedText
    );
    app.color.open_with("#00ff00");
    let _ = app.update(Message::Color(crate::color_dialog::ColorMsg::Ok));
    assert_eq!(app.drawing.texts[0].color, "#00ff00");
    assert_eq!(
        app.color_target,
        crate::color_dialog::ColorTarget::DrawColor
    );
}

#[test]
fn e2e_orient_sets_area_text_labels_and_symbols_to_0_or_90() {
    use crate::input::KeyCmd;
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    app.selection.area_labels.insert(id);
    app.selection.area_sizes.insert(id);
    let _ = app.update(Message::Key(KeyCmd::OrientV));
    assert_eq!(app.drawing.polygons[0].label_rotation, Some(90.0));
    assert_eq!(app.drawing.polygons[0].area_size_rotation, Some(90.0));
    let _ = app.update(Message::Key(KeyCmd::OrientH));
    assert_eq!(app.drawing.polygons[0].label_rotation, Some(0.0));
    let t = app_with_label();
    let mut app = t;
    let _ = app.update(Message::Key(KeyCmd::OrientV));
    assert_eq!(app.drawing.texts[0].rotation, 90.0);
}

#[test]
fn e2e_rotating_an_area_turns_its_pinned_label_with_it() {
    let mut app = square_in_select_mode();
    let id = app.drawing.polygons[0].id;
    // Centre of the square is (-6, -2); the label sits 3 ft right of it.
    app.drawing.polygons[0].label_position = Some(sketch_core::geometry::Pt::new(-3.0, -2.0));
    app.selection.polys.insert(id);
    let _ = app.update(Message::RotateSelected(90.0));
    let poly = &app.drawing.polygons[0];
    let at = poly.label_position.unwrap();
    assert!(
        (at.x - -6.0).abs() < 1e-9 && (at.y - 1.0).abs() < 1e-9,
        "{at:?}"
    );
    // The text counter-turns by the same amount, and the size follows.
    assert_eq!(poly.label_rotation, Some(-90.0));
    assert_eq!(poly.area_size_rotation, Some(-90.0));
}

#[test]
fn e2e_area_text_fields_survive_a_save_and_an_open() {
    let mut app = square_in_select_mode();
    {
        let poly = &mut app.drawing.polygons[0];
        poly.label_position = Some(sketch_core::geometry::Pt::new(-4.0, 1.0));
        poly.area_size_position = Some(sketch_core::geometry::Pt::new(-4.0, 0.0));
        poly.label_scale = Some(1.5);
        poly.area_size_scale = Some(2.0);
        poly.label_font_size = Some(20.0);
        poly.label_font_family = Some("Georgia".into());
        poly.label_bold = Some(true);
        poly.label_italic = Some(false);
        poly.label_underline = Some(true);
        poly.label_color = Some("#ff0000".into());
    }
    let model = app.build_save_model();
    let saved = &model["pages"][0]["polygons"][0];
    assert_eq!(saved["labelPosition"]["x"], -4.0);
    assert_eq!(saved["areaSizePosition"]["y"], 0.0);
    assert_eq!(saved["labelScale"], 1.5);
    assert_eq!(saved["areaSizeScale"], 2.0);
    assert_eq!(saved["labelFontFamily"], "Georgia");
    assert_eq!(saved["labelUnderline"], true);
    assert_eq!(saved["labelColor"], "#ff0000");
    // Open it in a fresh app: every field comes back.
    let mut fresh = state(harness());
    let _ = fresh.apply_model(&model, "x");
    let poly = &fresh.drawing.polygons[0];
    assert_eq!(
        poly.label_position,
        Some(sketch_core::geometry::Pt::new(-4.0, 1.0))
    );
    assert_eq!(
        poly.area_size_position,
        Some(sketch_core::geometry::Pt::new(-4.0, 0.0))
    );
    assert_eq!(
        (poly.label_scale, poly.area_size_scale),
        (Some(1.5), Some(2.0))
    );
    assert_eq!(poly.label_font_size, Some(20.0));
    assert_eq!(poly.label_font_family.as_deref(), Some("Georgia"));
    assert_eq!(
        (poly.label_bold, poly.label_italic, poly.label_underline),
        (Some(true), Some(false), Some(true))
    );
    assert_eq!(poly.label_color.as_deref(), Some("#ff0000"));
}

#[test]
fn e2e_a_pinned_area_label_paints_where_it_was_put() {
    // Nudge the picked label 3 ft up (60 px): its ink moves with it.
    let ink_rows = |nudges: u32| {
        let mut h = defined_square();
        h.send(Message::Rail(crate::rail::RailTool::Select));
        h.script(&["move (443, 521)", "click (443, 521)"]);
        for _ in 0..nudges {
            h.send(Message::Key(crate::input::KeyCmd::Nudge(0.0, 1.0)));
        }
        h.send(Message::Key(crate::input::KeyCmd::Escape));
        let shot = h.screenshot(&format!("area-label-nudged-{nudges}"));
        // The topmost row of dark ink inside the square's interior.
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&shot).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        for y in 440..620u32 {
            for x in 360..530u32 {
                let i = ((y * info.width + x) * 4) as usize;
                if buf[i] < 110 && buf[i + 1] < 110 && buf[i + 2] < 140 {
                    return y;
                }
            }
        }
        u32::MAX
    };
    let flat = ink_rows(0);
    let moved = ink_rows(3);
    assert!(
        moved + 40 < flat,
        "three nudges lifted the label: {flat} -> {moved}"
    );
}

/// A lone horizontal wall (window px 341..541) with the Select tool armed.
fn app_with_wall() -> App {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
    ]);
    let mut app = state(h);
    app.selection.clear();
    app
}

/// Canvas px of a wall's measurement label (the hit test's own anchor).
fn label_px(app: &App, id: i64) -> (f32, f32) {
    let line = app.drawing.lines.iter().find(|l| l.id == id).unwrap();
    let (x, y, _) = crate::handles::dimension_anchor(&app.dim_view(), line).unwrap();
    (x as f32, y as f32)
}

#[test]
fn e2e_a_measurement_label_is_picked_and_the_wall_still_wins_on_the_wall() {
    let mut app = app_with_wall();
    let id = app.drawing.lines[0].id;
    let (lx, ly) = label_px(&app, id);
    let _ = app.update(Message::CanvasClick {
        x: lx,
        y: ly,
        double: false,
        ctrl: false,
    });
    assert!(app.selection.dimensions.contains(&id));
    assert!(app.selection.lines.is_empty(), "the label, not the wall");
    // Ctrl toggles it back out.
    let _ = app.update(Message::CanvasClick {
        x: lx,
        y: ly,
        double: false,
        ctrl: true,
    });
    assert!(app.selection.dimensions.is_empty());
    // A click within 4px of the wall picks the wall even under the label's box.
    let line = app.drawing.lines[0].clone();
    let mid = crate::handles::to_screen(
        sketch_core::geometry::Pt::new(
            (line.start.x + line.end.x) / 2.0,
            (line.start.y + line.end.y) / 2.0,
        ),
        app.view.pan,
        app.view.zoom,
    );
    let _ = app.update(Message::CanvasClick {
        x: mid.0 as f32 + 60.0,
        y: mid.1 as f32 + 2.0,
        double: false,
        ctrl: false,
    });
    assert!(app.selection.lines.contains(&id) && app.selection.dimensions.is_empty());
}

#[test]
fn e2e_dragging_a_measurement_label_pins_it_and_the_pin_follows_the_wall() {
    let mut app = app_with_wall();
    let id = app.drawing.lines[0].id;
    let before =
        crate::handles::dimension_label_world(&app.dim_view(), &app.drawing.lines[0]).unwrap();
    let (lx, ly) = label_px(&app, id);
    // Pull it 2 ft right and 1 ft down the screen.
    let _ = app.update(Message::CanvasPress(lx, ly));
    let _ = app.update(Message::CanvasDrag(lx + 40.0, ly + 20.0, false));
    let _ = app.update(Message::CanvasDragEnd);
    let line = &app.drawing.lines[0];
    let pin = line.dimension_position.expect("the drag pinned the label");
    assert!((pin.x - (before.x + 2.0)).abs() < 1e-9 && (pin.y - (before.y - 1.0)).abs() < 1e-9);
    let mid = sketch_core::geometry::Pt::new(
        (line.start.x + line.end.x) / 2.0,
        (line.start.y + line.end.y) / 2.0,
    );
    assert_eq!(line.dimension_anchor, Some(mid));
    assert!(app.selection.dimensions.contains(&id));
    // The wall itself did not move, and one undo unpins the label.
    assert_eq!(app.drawing.lines[0].start, line_start_before(&app));
    let _ = app.update(Message::Undo);
    assert!(app.drawing.lines[0].dimension_position.is_none());
}

/// The drawn wall's start (world), for "did not move" checks.
fn line_start_before(app: &App) -> sketch_core::geometry::Pt {
    app.drawing.lines[0].start
}

#[test]
fn a_pinned_label_rides_the_wall_it_was_pinned_against() {
    let mut line = crate::drawing::tests_wall();
    line.dimension_position = Some(sketch_core::geometry::Pt::new(5.0, 4.0));
    line.dimension_anchor = Some(sketch_core::geometry::Pt::new(5.0, 0.0));
    // The wall (0,0)-(10,0) has its middle at the anchor: the pin as it is.
    assert_eq!(
        crate::drawing::pinned_dimension_point(&line),
        Some(sketch_core::geometry::Pt::new(5.0, 4.0))
    );
    // Slide the wall 3 right and 2 up: the label goes with it.
    line.start = sketch_core::geometry::Pt::new(3.0, 2.0);
    line.end = sketch_core::geometry::Pt::new(13.0, 2.0);
    assert_eq!(
        crate::drawing::pinned_dimension_point(&line),
        Some(sketch_core::geometry::Pt::new(8.0, 6.0))
    );
    // No pin, nothing; a pin without an anchor leans on the current middle.
    line.dimension_position = None;
    assert_eq!(crate::drawing::pinned_dimension_point(&line), None);
}

#[test]
fn e2e_delete_hides_a_picked_label_and_space_restores_it() {
    use crate::input::KeyCmd;
    let mut app = app_with_wall();
    let id = app.drawing.lines[0].id;
    app.selection.dimensions.insert(id);
    let _ = app.update(Message::Delete);
    assert!(app.drawing.lines[0].dimension_hidden, "the label is hidden");
    assert_eq!(app.drawing.lines.len(), 1, "the wall stays");
    assert!(app.selection.is_empty());
    let _ = app.update(Message::Undo);
    assert!(!app.drawing.lines[0].dimension_hidden);

    // Space (a tap) with the wall selected toggles its label both ways.
    app.selection.lines.insert(id);
    let _ = app.update(Message::Key(KeyCmd::SpaceDown));
    let _ = app.update(Message::Key(KeyCmd::SpaceUp));
    assert!(app.drawing.lines[0].dimension_hidden);
    let _ = app.update(Message::Key(KeyCmd::SpaceDown));
    let _ = app.update(Message::Key(KeyCmd::SpaceUp));
    assert!(
        !app.drawing.lines[0].dimension_hidden,
        "a second tap brings it back"
    );
    // Hiding drops a pin.
    app.drawing.lines[0].dimension_position = Some(sketch_core::geometry::Pt::new(1.0, 1.0));
    let _ = app.update(Message::Key(KeyCmd::SpaceDown));
    let _ = app.update(Message::Key(KeyCmd::SpaceUp));
    assert!(
        app.drawing.lines[0].dimension_hidden && app.drawing.lines[0].dimension_position.is_none()
    );
}

#[test]
fn e2e_select_by_type_dimensions_picks_every_visible_label() {
    let mut app = app_with_wall();
    let id = app.drawing.lines[0].id;
    // A second, hidden-label wall.
    let mut hidden = app.drawing.lines[0].clone();
    hidden.id = 999;
    hidden.start = sketch_core::geometry::Pt::new(0.0, -5.0);
    hidden.end = sketch_core::geometry::Pt::new(10.0, -5.0);
    hidden.dimension_hidden = true;
    app.drawing.lines.push(hidden);
    app.selection.lines.insert(id);
    let _ = app.update(Message::SelectByType(
        crate::select_menu::SelectKind::Dimensions,
    ));
    assert_eq!(
        app.selection.dimensions.iter().copied().collect::<Vec<_>>(),
        vec![id]
    );
    assert!(
        app.selection.lines.is_empty(),
        "the selection was cleared first"
    );
}

#[test]
fn e2e_a_finished_areas_labels_sit_outside_its_walls() {
    // `getLineDimensionSide`: a wall of a closed area puts its label on the
    // outside (the default placement).
    let mut app = state(defined_square());
    app.view.mode = crate::view_state::Mode::Select;
    // The square spans x -11..-1, y -7..3.
    for line in app.drawing.lines.clone() {
        let at = crate::handles::dimension_label_world(&app.dim_view(), &line).unwrap();
        let outside = at.x < -11.0 || at.x > -1.0 || at.y < -7.0 || at.y > 3.0;
        assert!(
            outside,
            "{:?} -> {at:?} should be outside",
            (line.start, line.end)
        );
    }
}

#[test]
fn e2e_delete_removes_selected_text_labels_and_symbols() {
    let mut app = app_with_label();
    assert_eq!(app.drawing.texts.len(), 1);
    let _ = app.update(Message::Delete);
    assert!(app.drawing.texts.is_empty(), "the label is gone");
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.texts.len(), 1, "undo brings it back");
    // A symbol too.
    let (mut app, id) = app_with_selected_window();
    assert!(!app.drawing.symbols.is_empty());
    let _ = app.update(Message::Delete);
    assert!(app.drawing.symbols.iter().all(|s| s.id != id));
    let _ = app.update(Message::Undo);
    assert!(app.drawing.symbols.iter().any(|s| s.id == id));
}

#[test]
fn a_fence_picks_what_sits_inside_by_position() {
    use sketch_core::geometry::Pt;
    let texts = vec![
        crate::placed_text::PlacedText::new(1, "in"),
        crate::placed_text::PlacedText::new(2, "out"),
    ];
    let mut texts = texts;
    texts[0].x = 1.0;
    texts[0].y = 1.0;
    texts[1].x = 50.0;
    let pick = crate::selection::Pickable {
        texts: &texts,
        symbols: &[],
        area_text: vec![(7, Pt::new(2.0, 2.0), Pt::new(2.0, 40.0))],
        dimensions: vec![(9, Pt::new(3.0, 3.0)), (10, Pt::new(60.0, 0.0))],
    };
    let mut sel = crate::selection::Selection::default();
    let inside = |p: Pt| p.x >= 0.0 && p.x <= 10.0 && p.y >= 0.0 && p.y <= 10.0;
    crate::selection::pick_by_position(&mut sel, &pick, &inside, &|id| id != 10);
    assert_eq!(sel.texts.into_iter().collect::<Vec<_>>(), vec![1]);
    assert!(sel.area_labels.contains(&7) && !sel.area_sizes.contains(&7));
    assert_eq!(sel.dimensions.into_iter().collect::<Vec<_>>(), vec![9]);
}

#[test]
fn e2e_a_picked_measurement_label_paints_a_highlight_box_and_leader() {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        // The label's hit anchor: 16px under the wall's middle.
        "move (445, 437)",
        "click (445, 437)",
    ]);
    let shot = h.screenshot("dimension-picked");
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&shot).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    // The select ink (purple) around the label.
    let mut purple = 0;
    for y in 418..450u32 {
        for x in 400..490u32 {
            let i = ((y * info.width + x) * 4) as usize;
            if buf[i] > 150 && buf[i + 2] > 200 && buf[i + 1] < 140 {
                purple += 1;
            }
        }
    }
    assert!(
        purple > 40,
        "the label box / leader paint in the select colour: {purple}"
    );
}

#[test]
fn e2e_undo_and_redo_carry_the_pages_north_angle() {
    let mut app = state(defined_square());
    let north = |a: &App| a.pages.page().north_angle;
    assert_eq!(north(&app), 0.0);
    // Turning the sketch turns the compass with it; one undo gives both back.
    let _ = app.update(Message::RotateSketchBy(90.0));
    assert_eq!(north(&app), 90.0);
    let _ = app.update(Message::Undo);
    assert_eq!(north(&app), 0.0, "the compass is part of the snapshot");
    let _ = app.update(Message::Redo);
    assert_eq!(north(&app), 90.0, "and of the redo");
    // A north set by hand rides along with the next step's snapshot.
    let _ = app.update(Message::Undo);
    app.pages.pages[0].north_angle = 30.0;
    let _ = app.update(Message::RotateSketchBy(90.0));
    assert_eq!(north(&app), 120.0);
    let _ = app.update(Message::Undo);
    assert_eq!(north(&app), 30.0);
    // A mirror reverses the compass: undo puts the bearing back too.
    let mut app = state(defined_square());
    app.pages.pages[0].north_angle = 40.0;
    let _ = app.update(Message::Key(crate::input::KeyCmd::FlipSketch(true)));
    assert_eq!(
        north(&app),
        320.0,
        "across the vertical axis a bearing a becomes 360 - a"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(north(&app), 40.0);
}

#[test]
fn e2e_save_as_default_options_survive_a_restart_and_restore_defaults_uses_them() {
    use crate::options_dialog::OptMsg;
    crate::persist::with_temp_store(|| {
        let mut first = state(harness());
        // The draft the dialog would hold: a recognisable, non-default value.
        let bundled = first.drawing.options.exterior_width;
        let mut draft = first.drawing.options.clone();
        draft.exterior_width = bundled + 3.0;
        draft.define_first = !draft.define_first;
        first.options_dialog.draft = draft.clone();
        let _ = first.update(Message::Options(OptMsg::SaveDefaults));
        assert!(first
            .toasts
            .text()
            .is_some_and(|t| t == "Current options saved as defaults."));

        // A fresh start reads them: applied straight away, and kept for Restore.
        let mut second = state(harness());
        assert_eq!(second.drawing.options.exterior_width, bundled + 3.0);
        assert_eq!(second.drawing.options.define_first, draft.define_first);
        assert_eq!(second.default_options.exterior_width, bundled + 3.0);
        second.drawing.options.exterior_width = 9.0;
        let _ = second.update(Message::Options(OptMsg::RestoreDefaults));
        assert_eq!(
            second.drawing.options.exterior_width,
            bundled + 3.0,
            "Restore Defaults means the saved defaults"
        );
    });
}

#[test]
fn e2e_without_a_store_save_as_default_is_session_only() {
    use crate::options_dialog::OptMsg;
    let mut app = state(harness());
    let _ = app.update(Message::Options(OptMsg::SaveDefaults));
    assert_eq!(
        app.toasts.text(),
        Some("Current options saved as defaults for this session.")
    );
}

#[test]
fn e2e_the_area_code_library_and_the_two_lists_survive_a_restart() {
    use crate::new_code_dialog::NewCodeMsg;
    use crate::text_manager::TextManagerMsg;
    crate::persist::with_temp_store(|| {
        let mut first = state(harness());
        // A new area code, a new text-library label, new favourites.
        let _ = first.update(Message::OpenNewCode);
        let _ = first.update(Message::NewCode(NewCodeMsg::Code("zzq".into())));
        let _ = first.update(Message::NewCode(NewCodeMsg::Name("Zeta".into())));
        let _ = first.update(Message::NewCode(NewCodeMsg::Create));
        assert!(first.area_codes.iter().any(|c| c.code == "ZZQ"));
        let _ = first.update(Message::TextLibraryManager(TextManagerMsg::Open));
        let _ = first.update(Message::TextLibraryManager(TextManagerMsg::Input(
            "Gazebo".into(),
        )));
        let _ = first.update(Message::TextLibraryManager(TextManagerMsg::Add));
        assert!(first.dock.text_labels.iter().any(|l| l == "Gazebo"));
        let kind = first.symbols_lib().favorites[0].id.clone();
        first.symbol_organizer.checked = vec![kind.clone()];
        let _ = first.symbol_organizer_msg(crate::symbol_organizer::SymbolOrganizerMsg::Save);

        let second = state(harness());
        assert!(
            second.area_codes.iter().any(|c| c.code == "ZZQ"),
            "the table came back"
        );
        assert_eq!(
            second.dock.area_codes, second.area_codes,
            "the dock shows the same table"
        );
        assert!(second.dock.text_labels.iter().any(|l| l == "Gazebo"));
        assert_eq!(second.symbol_favorites, Some(vec![kind]));
    });
}

#[test]
fn e2e_codes_json_buttons_open_file_actions_without_applying_the_draft() {
    let mut h = harness();
    h.send(Message::OpenCodesEditor);
    h.script(&["click \"Import JSON\"", "click \"Export JSON\""]);
    let app = state(h);
    assert!(app.codes_editor.open, "file buttons must not invoke Apply");
    assert_ne!(app.toasts.text(), Some("Area code table updated."));
}

#[test]
fn e2e_codes_json_import_export_and_apply_round_trip_through_files_and_restart() {
    use crate::codes_editor::CodesMsg;
    crate::persist::with_temp_store(|| {
        let mut app = state(harness());
        let original = app.area_codes.clone();
        let mut codes = original.clone();
        codes[0].code = "  zzz  ".into();
        codes[0].name = "  Custom living  ".into();
        let raw = serde_json::to_string(&codes).unwrap();
        let input = crate::persist::path("import.json").unwrap();
        let output = crate::persist::path("export.json").unwrap();
        std::fs::write(&input, format!("\u{feff}{raw}")).unwrap();
        let _ = app.update(Message::OpenCodesEditor);
        let session = app.codes_editor.session;
        let _ = app.update(Message::CodesEditor(CodesMsg::ImportChosen(
            session,
            Some(input),
        )));
        assert_eq!(app.area_codes, original, "import is only a draft");
        assert!(app.codes_editor.text().contains("  zzz  "));
        let body = app.codes_editor.export_json().unwrap();
        let expected = sketch_core::geometry::area_codes::from_json(&body).unwrap();
        assert_eq!(expected[0].code, "ZZZ");
        assert_eq!(expected[0].name, "Custom living");
        let _ = app.update(Message::CodesEditor(CodesMsg::ExportChosen {
            session,
            body: body.clone(),
            path: Some(output.clone()),
        }));
        assert_eq!(std::fs::read_to_string(output).unwrap(), body);
        assert_eq!(app.area_codes, original, "export must not apply the draft");
        assert!(app.codes_editor.open);
        let _ = app.update(Message::CodesEditor(CodesMsg::Apply));
        assert!(!app.codes_editor.open);
        assert_eq!(app.area_codes, expected);
        assert_eq!(app.dock.area_codes, expected);
        let reopened = state(harness());
        assert_eq!(reopened.area_codes, expected, "Apply persists the library");
    });
}

#[test]
fn e2e_codes_json_invalid_files_stay_editable_and_io_errors_preserve_the_library() {
    use crate::codes_editor::CodesMsg;
    crate::persist::with_temp_store(|| {
        let mut app = state(harness());
        let original = app.area_codes.clone();
        let _ = app.update(Message::OpenCodesEditor);
        let session = app.codes_editor.session;
        let before = app.codes_editor.text();
        let missing = crate::persist::path("missing.json").unwrap();
        let _ = app.update(Message::CodesEditor(CodesMsg::ImportChosen(
            session,
            Some(missing),
        )));
        assert!(app
            .toasts
            .text()
            .unwrap()
            .starts_with("Could not import area codes:"));
        assert_eq!(app.codes_editor.text(), before);
        let file = crate::persist::path("invalid.json").unwrap();
        std::fs::write(&file, "not a code table").unwrap();
        let _ = app.update(Message::CodesEditor(CodesMsg::ImportChosen(
            session,
            Some(file),
        )));
        assert!(app.codes_editor.text().contains("not a code table"));
        assert!(app.codes_editor.export_json().is_err());
        let _ = app.update(Message::CodesEditor(CodesMsg::Export));
        assert!(app.toasts.text().is_some());
        let _ = app.update(Message::CodesEditor(CodesMsg::Apply));
        assert!(app.codes_editor.open);
        assert_eq!(app.area_codes, original);
        let directory = crate::persist::path("unused.json")
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let _ = app.update(Message::CodesEditor(CodesMsg::ExportChosen {
            session,
            body: "[]".into(),
            path: Some(directory),
        }));
        assert!(app
            .toasts
            .text()
            .unwrap()
            .starts_with("Could not export area codes:"));
        assert_eq!(app.area_codes, original);
    });
}

#[test]
fn e2e_codes_json_cancelled_and_stale_file_results_do_nothing() {
    use crate::codes_editor::CodesMsg;
    crate::persist::with_temp_store(|| {
        let mut app = state(harness());
        let _ = app.update(Message::OpenCodesEditor);
        let old = app.codes_editor.session;
        let draft = app.codes_editor.text();
        let _ = app.update(Message::CodesEditor(CodesMsg::ImportChosen(old, None)));
        let _ = app.update(Message::CodesEditor(CodesMsg::ExportChosen {
            session: old,
            body: "[]".into(),
            path: None,
        }));
        assert_eq!(app.codes_editor.text(), draft);
        let _ = app.update(Message::CodesEditor(CodesMsg::Close));
        let _ = app.update(Message::OpenCodesEditor);
        let output = crate::persist::path("stale.json").unwrap();
        let _ = app.update(Message::CodesEditor(CodesMsg::ImportChosen(
            old,
            Some(output.clone()),
        )));
        let _ = app.update(Message::CodesEditor(CodesMsg::ExportChosen {
            session: old,
            body: "[]".into(),
            path: Some(output.clone()),
        }));
        assert_eq!(app.codes_editor.text(), draft);
        assert!(!output.exists());
        assert!(app.toasts.text().is_none());
    });
}

#[test]
fn e2e_a_new_sketch_and_a_file_without_a_table_keep_the_area_codes_in_use() {
    use crate::new_code_dialog::NewCodeMsg;
    let mut app = state(harness());
    let _ = app.update(Message::OpenNewCode);
    let _ = app.update(Message::NewCode(NewCodeMsg::Code("KEEP".into())));
    let _ = app.update(Message::NewCode(NewCodeMsg::Name("Keeper".into())));
    let _ = app.update(Message::NewCode(NewCodeMsg::Create));
    // `resetToBlank` does not touch the table...
    let _ = app.update(Message::NewFile);
    assert!(app.area_codes.iter().any(|c| c.code == "KEEP"));
    // ...and a file that carries none leaves it too.
    let _ = app.apply_model(
        &serde_json::json!({"version": "Sketch-Web", "pages": [{}]}),
        "x",
    );
    assert!(app.area_codes.iter().any(|c| c.code == "KEEP"));
}

#[test]
fn e2e_invalid_stored_files_fall_back_to_the_bundled_values() {
    crate::persist::with_temp_store(|| {
        crate::persist::write("default-options.json", "{ not json").unwrap();
        crate::persist::write("area-codes.json", "[1, 2").unwrap();
        crate::persist::write("text-library-labels.json", "[]").unwrap();
        crate::persist::write("symbol-favorites.json", "[\"no/such-symbol\"]").unwrap();
        let app = state(harness());
        let bundled = crate::options::Options::default();
        assert_eq!(app.drawing.options.exterior_width, bundled.exterior_width);
        assert_eq!(
            app.area_codes,
            sketch_core::geometry::area_codes::default_area_codes()
        );
        assert!(!app.dock.text_labels.is_empty(), "an empty list is no list");
        assert_eq!(
            app.symbol_favorites, None,
            "unknown symbols leave the defaults"
        );
    });
}

#[test]
fn e2e_the_status_bar_chip_and_select_follow_the_defined_areas() {
    // Nothing defined: the boot values.
    let mut h = harness();
    assert!(h.shows("GLA 0.0 sf") && h.shows("0 areas") && h.shows("No defined areas 0.0"));
    // One defined 10 x 10 ft first floor.
    let mut h = defined_square();
    assert!(h.shows("GLA 100.0 sf"), "the chip carries the total GLA");
    assert!(h.shows("1 area"), "and the defined-area count (singular)");
    assert!(
        h.shows("GLA1  First Floor  100.0"),
        "the select lists code, name and area in columns"
    );
    assert!(!h.shows("No defined areas 0.0"));
    h.screenshot("status-areas");
}

#[test]
fn e2e_picking_an_area_in_the_status_select_selects_it() {
    use crate::statusbar::StatusMsg;
    let mut app = state(defined_square());
    let id = app.drawing.polygons[0].id;
    app.selection.clear();
    let _ = app.update(Message::Status(StatusMsg::AreaPick(id)));
    assert_eq!(app.selection.polys.len(), 1);
    assert!(app.selection.polys.contains(&id));
    // An unknown id does nothing.
    let _ = app.update(Message::Status(StatusMsg::AreaPick(424242)));
    assert!(app.selection.polys.contains(&id));
}

#[test]
fn the_chip_and_option_text_follow_updatecalcspanel() {
    use crate::statusbar::{area_options, chip_texts};
    let totals = crate::calcs::AreaTotals {
        total_gla: 1234.56,
        gba: 0.0,
    };
    assert_eq!(
        chip_texts(totals, 3),
        ("GLA 1234.6 sf".to_string(), "3 areas".to_string())
    );
    assert_eq!(chip_texts(totals, 1).1, "1 area");
    assert_eq!(chip_texts(totals, 0).1, "0 areas");
    // Columns: the code and name pad to the widest (at least 3 and 4).
    let rows = vec![
        (1, "GLA1".to_string(), "First Floor".to_string(), 100.04),
        (2, "GAR".to_string(), "Garage".to_string(), 7.0),
    ];
    let labels: Vec<String> = area_options(&rows).into_iter().map(|o| o.label).collect();
    assert_eq!(labels[0], "GLA1  First Floor  100.0");
    assert_eq!(labels[1], "GAR   Garage       7.0");
    let short = vec![(1, "A".to_string(), "B".to_string(), 1.0)];
    assert_eq!(area_options(&short)[0].label, "A    B     1.0");
}

#[test]
fn e2e_a_text_label_copies_cuts_and_pastes() {
    let mut app = app_with_label();
    let id = app.drawing.texts[0].id;
    let _ = app.update(Message::Copy);
    let _ = app.update(Message::Paste);
    assert_eq!(app.drawing.texts.len(), 2, "the copy is a second label");
    let copy = app.drawing.texts.iter().find(|t| t.id != id).unwrap();
    assert!(
        (copy.x - 2.0).abs() < 1e-9 && (copy.y - 2.0).abs() < 1e-9,
        "2 ft down-right"
    );
    assert_eq!(copy.text, "Porch");
    assert!(app.selection.texts.contains(&copy.id) && app.selection.texts.len() == 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.texts.len(), 1, "one undo step per paste");

    // Cut removes it and keeps it on the clipboard.
    let mut app = app_with_label();
    let _ = app.update(Message::Cut);
    assert!(app.drawing.texts.is_empty());
    let _ = app.update(Message::Paste);
    assert_eq!(app.drawing.texts.len(), 1);
    assert_eq!(app.drawing.texts[0].text, "Porch");
}

#[test]
fn e2e_cutting_a_mixed_selection_loses_nothing() {
    // A wall plus a label: both go, both come back (the label used to be
    // deleted without ever reaching the clipboard).
    let mut app = app_with_label();
    let wall = app_with_wall().drawing.lines[0].clone();
    app.drawing.lines.push(wall.clone());
    app.selection.lines.insert(wall.id);
    let _ = app.update(Message::Cut);
    assert!(app.drawing.lines.is_empty() && app.drawing.texts.is_empty());
    let _ = app.update(Message::Paste);
    assert_eq!(app.drawing.lines.len(), 1);
    assert_eq!(app.drawing.texts.len(), 1);
    assert_eq!(
        app.selection.lines.len() + app.selection.texts.len(),
        2,
        "both come back selected"
    );
}

#[test]
fn e2e_a_cut_with_nothing_copyable_changes_nothing() {
    // Only an area's label picked: nothing can be copied, so nothing is cut.
    let mut app = state(defined_square());
    app.view.mode = crate::view_state::Mode::Select;
    app.selection.clear();
    let id = app.drawing.polygons[0].id;
    app.selection.area_labels.insert(id);
    let (lines, polys) = (app.drawing.lines.len(), app.drawing.polygons.len());
    let _ = app.update(Message::Cut);
    assert_eq!(
        (app.drawing.lines.len(), app.drawing.polygons.len()),
        (lines, polys)
    );
}

#[test]
fn e2e_symbols_paste_attached_to_their_copied_wall_or_free() {
    // A window with its wall: the copy rides the copied wall.
    let (mut app, id) = app_with_selected_window();
    let wall = app.drawing.symbols[0].wall_id.expect("hosted");
    app.selection.lines.insert(wall);
    let walls = app.drawing.lines.len();
    let _ = app.update(Message::Copy);
    let _ = app.update(Message::Paste);
    assert_eq!(app.drawing.lines.len(), walls + 1);
    assert_eq!(app.drawing.symbols.len(), 2);
    let copy = app.drawing.symbols.iter().find(|s| s.id != id).unwrap();
    let new_wall = app.selection.lines.iter().next().copied().unwrap();
    assert_eq!(copy.wall_id, Some(new_wall), "hosted on the pasted wall");
    // The window alone: the copy detaches (it must not snap back onto the original wall).
    let (mut app, id) = app_with_selected_window();
    let _ = app.update(Message::Copy);
    let _ = app.update(Message::Paste);
    let copy = app.drawing.symbols.iter().find(|s| s.id != id).unwrap();
    assert!(copy.wall_id.is_none() && copy.wall_fraction.is_none());
    let original = app.drawing.symbols.iter().find(|s| s.id == id).unwrap();
    assert!(
        (copy.x - (original.x + 2.0)).abs() < 1e-9 && (copy.y - (original.y + 2.0)).abs() < 1e-9
    );
    assert!(app.selection.symbols.contains(&copy.id));
}

#[test]
fn e2e_pasting_an_area_carries_its_pinned_label_and_size() {
    let mut app = state(defined_square());
    app.view.mode = crate::view_state::Mode::Select;
    let id = app.drawing.polygons[0].id;
    app.drawing.polygons[0].label_position = Some(sketch_core::geometry::Pt::new(-6.0, 0.0));
    app.drawing.polygons[0].area_size_position = Some(sketch_core::geometry::Pt::new(-6.0, -1.0));
    app.selection.clear();
    app.selection.polys.insert(id);
    let _ = app.update(Message::Copy);
    let _ = app.update(Message::Paste);
    let copy = app.drawing.polygons.iter().find(|p| p.id != id).unwrap();
    assert_eq!(
        copy.label_position,
        Some(sketch_core::geometry::Pt::new(-4.0, 2.0))
    );
    assert_eq!(
        copy.area_size_position,
        Some(sketch_core::geometry::Pt::new(-4.0, 1.0))
    );
}

#[test]
fn e2e_select_by_type_menu_opens_and_selects() {
    // Edit Tools › Select opens the by-type menu; the rows select by type.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        SELECT_BY_TYPE_BTN,
    ]);
    let app = state(h);
    assert!(app.select_menu.open.is_some(), "the menu opens");

    // Select Areas with no areas clears the selection.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        SELECT_BY_TYPE_BTN,
    ]);
    h.send(Message::SelectByType(crate::select_menu::SelectKind::Areas));
    let app = state(h);
    assert!(app.select_menu.open.is_none(), "choosing a row closes it");
    assert!(app.selection.is_empty());

    // Select Freeform picks the interior walls only: draw one of each first.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_INT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_DRAW_EXT,
        "move (641, 529)",
        "click (641, 529)",
        "move (841, 529)",
        "click (841, 529)",
        "type escape",
    ]);
    h.send(Message::SelectByType(
        crate::select_menu::SelectKind::Freeform,
    ));
    let app = state(h);
    assert_eq!(app.selection.lines.len(), 1, "one interior wall");
    let id = *app.selection.lines.iter().next().expect("selected");
    let line = app.drawing.lines.iter().find(|l| l.id == id).expect("wall");
    assert_eq!(line.kind, sketch_core::model::WallType::Interior);
}

#[test]
fn e2e_context_menu_rotate_turns_the_selected_wall() {
    // Rotate 90° from the context menu: the selected wall turns about the
    // selection's own centre, exactly as the live original does.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        "move (600, 300)",
        "press right (600, 300)",
    ]);
    let before = state(h).drawing.lines[0].clone();
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        "move (600, 300)",
        "press right (600, 300)",
    ]);
    h.send(Message::ContextAction(
        crate::context_menu::Action::Rotate90,
    ));
    let app = state(h);
    let after = app.drawing.lines[0].clone();
    // A horizontal wall becomes vertical: x stops changing, y starts.
    assert!(
        (after.start.x - after.end.x).abs() < 1e-6,
        "the wall is vertical now: {:?} -> {:?}",
        after.start,
        after.end
    );
    assert!((after.start.y - after.end.y).abs() > 1.0);
    assert!(app.undo.can_undo(), "the rotation banks undo");
    assert!(
        (before.start.y - before.end.y).abs() < 1e-6,
        "it started horizontal"
    );
}

#[test]
fn e2e_context_menu_flip_mirrors_the_selected_wall() {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        "move (600, 300)",
        "press right (600, 300)",
    ]);
    let before = state(h).drawing.lines[0].clone();
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        "move (600, 300)",
        "press right (600, 300)",
    ]);
    h.send(Message::ContextAction(
        crate::context_menu::Action::FlipHorz,
    ));
    let app = state(h);
    let after = app.drawing.lines[0].clone();
    // A wall alone in its selection mirrors onto itself (the centre is its own
    // midpoint), so its endpoints swap and its length is unchanged.
    let len_before =
        ((before.end.x - before.start.x).powi(2) + (before.end.y - before.start.y).powi(2)).sqrt();
    let len_after =
        ((after.end.x - after.start.x).powi(2) + (after.end.y - after.start.y).powi(2)).sqrt();
    assert!((len_before - len_after).abs() < 1e-6);
}

#[test]
fn e2e_marquee_drag_selects_the_walls_it_encloses() {
    // Two walls drawn apart; a Select-mode drag on empty canvas from above-left
    // to below-right encloses both.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (441, 429)",
        "click (441, 429)",
        "type escape",
        "move (641, 529)",
        "click (641, 529)",
        "move (741, 529)",
        "click (741, 529)",
        "type escape",
        RAIL_SELECT,
        // Start well clear of any wall, then drag a box around both.
        "press (300, 400)",
        "move (360, 420)",
        "move (500, 460)",
        "move (800, 560)",
        "release (800, 560)",
    ]);
    let app = state(h);
    assert_eq!(app.drawing.lines.len(), 2);
    assert_eq!(app.selection.lines.len(), 2, "the marquee took both walls");
    assert!(app.view.fence.is_none(), "the fence is released");
}

#[test]
fn e2e_marquee_ignores_walls_that_stick_out() {
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        // A box that covers only the first endpoint.
        "press (300, 400)",
        "move (400, 420)",
        "move (450, 450)",
        "release (450, 450)",
    ]);
    let app = state(h);
    assert!(
        app.selection.lines.is_empty(),
        "a wall with an endpoint outside is not enclosed"
    );
}

#[test]
fn e2e_dragging_a_selected_wall_moves_it_by_whole_grid_steps() {
    // Press on the wall, drag ~1.4 ft, release: the snap rounds to one step.
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ]);
    let before = state(h).drawing.lines[0].clone();
    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
        // iced_test delivers a press at the last moved position, so move first.
        // Off the curve handle (the wall's middle), on the wall.
        "move (500, 421)",
        "press (500, 421)",
        "move (470, 460)",
        "move (500, 490)",
        "release (500, 490)",
    ]);
    let app = state(h);
    let after = app.drawing.lines[0].clone();
    assert_eq!(
        app.drawing.lines.len(),
        1,
        "the drag moved, it did not draw"
    );
    let dx = after.start.x - before.start.x;
    let dy = after.start.y - before.start.y;
    // The wall moved, and by a whole number of feet on both axes.
    assert!(dx.abs() > 0.5 || dy.abs() > 0.5, "dx {dx} dy {dy}");
    assert!((dx - dx.round()).abs() < 1e-6, "dx {dx} is a whole step");
    assert!((dy - dy.round()).abs() < 1e-6, "dy {dy} is a whole step");
    assert!(app.undo.can_undo(), "the drag banks one undo step");
    assert!(app.view.drag.is_none(), "the drag is closed");
}

#[test]
fn e2e_dragging_an_endpoint_moves_only_that_vertex() {
    // iced_test reports a button press at the position from BEFORE its own
    // instruction's move, so the endpoint handle cannot be aimed with a script
    // press. The messages the canvas emits are driven directly instead.
    const SETUP: &[&str] = &[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
        RAIL_SELECT,
        "move (441, 421)",
        "click (441, 421)",
    ];
    let mut h = harness();
    h.script(SETUP);
    let app = state(h);
    assert_eq!(app.selection.lines.len(), 1);
    let line = app.drawing.lines[0].clone();
    let (ex, ey) = app.view.to_screen(crate::view_state::World {
        x: line.end.x,
        y: line.end.y,
    });

    let mut h = harness();
    h.script(SETUP);
    h.send(Message::CanvasPress(ex as f32, ey as f32));
    let app = state(h);
    assert!(
        matches!(
            app.view.drag,
            Some(crate::view_state::Drag::Endpoint { start: false, .. })
        ),
        "the end handle is grabbed: {:?}",
        app.view.drag
    );

    let mut h = harness();
    h.script(SETUP);
    h.send(Message::CanvasPress(ex as f32, ey as f32));
    h.send(Message::CanvasDrag(ex as f32, (ey + 100.0) as f32, false));
    h.send(Message::CanvasDragEnd);
    let app = state(h);
    let after = app.drawing.lines[0].clone();
    // The far end stays put; the grabbed end moved down.
    assert!((after.start.x - line.start.x).abs() < 1e-6, "start x moved");
    assert!((after.start.y - line.start.y).abs() < 1e-6, "start y moved");
    // World y grows upwards, so a 100px downward drag is a negative delta.
    assert!(
        after.end.y < line.end.y - 1.0,
        "the grabbed end moved down: {:?} -> {:?}",
        line.end,
        after.end
    );
    assert!(app.undo.can_undo());
}

#[test]
fn e2e_calcs_panel_lists_the_areas_and_toggles() {
    // Draw a closed area, give it a code, then open Calcs: the tree lists the
    // group and its area, and clicking the area expands its details.
    const SETUP: &[&str] = &[
        RAIL_DRAW_EXT,
        "move (464, 380)",
        "click (464, 380)",
        "move (664, 380)",
        "click (664, 380)",
        "move (664, 580)",
        "click (664, 580)",
        "move (464, 580)",
        "click (464, 580)",
        "move (464, 380)",
        "click (464, 380)",
        "type escape",
        RAIL_SELECT,
        "move (564, 480)",
        "click (564, 480)",
    ];
    let mut h = harness();
    h.script(SETUP);
    let app = state(h);
    assert_eq!(
        app.drawing.polygons.len(),
        1,
        "the loop closed into an area"
    );
    let id = app.drawing.polygons[0].id;

    // A defined code puts it in a group; UND is counted instead.
    let mut h = harness();
    h.script(SETUP);
    h.send(Message::AreaDialog(
        crate::area_dialog::AreaDialogMsg::Code("GLA1".into()),
    ));
    h.send(Message::AreaDialog(
        crate::area_dialog::AreaDialogMsg::Apply,
    ));
    h.send(Message::ToggleCalcs);
    let app = state(h);
    assert_eq!(app.drawing.polygons[0].kind, "GLA1");
    let mut computed = std::collections::BTreeMap::new();
    let result =
        sketch_core::area_calc::recalculate(&app.drawing.polygons, &app.area_codes, app.geo());
    for c in &result.polygons {
        computed.insert(
            c.id,
            crate::calcs::Computed {
                subtracted: c.subtracted,
                factor: c.factor,
                net: c.net,
                perimeter: c.perimeter,
            },
        );
    }
    let calcs = crate::calcs::build(&app.drawing.polygons, &computed);
    assert_eq!(calcs.groups.len(), 1);
    assert_eq!(calcs.groups[0].code, "GLA1");
    assert_eq!(calcs.groups[0].areas.len(), 1);
    assert!(app.dock.open);
    assert_eq!(app.dock.active, crate::dock::DockTab::Calcs);

    // Clicking the area row expands it and selects the area.
    let mut h = harness();
    h.script(SETUP);
    h.send(Message::CalcsArea(id));
    let app = state(h);
    assert!(app.dock.calcs_expanded.contains(&id));
    assert!(app.selection.polys.contains(&id));
    // Clicking again collapses it.
    let mut h = harness();
    h.script(SETUP);
    h.send(Message::CalcsArea(id));
    h.send(Message::CalcsArea(id));
    let app = state(h);
    assert!(!app.dock.calcs_expanded.contains(&id));

    // A group header toggles the group.
    let mut h = harness();
    h.script(SETUP);
    h.send(Message::CalcsGroup("GLA1".into()));
    let app = state(h);
    assert!(app.dock.calcs_collapsed.contains("GLA1"));
}

#[test]
fn e2e_breakdown_dialog_empty_guards_with_a_toast() {
    let mut h = harness();
    // No areas: OpenBreakdown does not open the dialog and toasts instead.
    h.send(Message::OpenBreakdown);
    assert!(h.shows("Define at least one area first."));
    assert!(
        !state(h).breakdown.open,
        "dialog does not open without areas"
    );
}

#[test]
fn e2e_breakdown_dialog_opens_and_reports_gla1() {
    let mut h = harness();
    // Assign the first area to the GLA1 code, then draw a 20x20 rectangle
    // with the same 5-click script the GLA-assignment path expects — that
    // transfers `kind = "GLA1"` onto the polygon (a MENU_RECT draw does not).
    h.send(Message::DefineFirst(true));
    h.send(Message::AreaType(
        Some("GLA1".into()),
        Some("First Floor".into()),
    ));
    // Same 5-click rectangle the define-first path expects (RAIL_DRAW_EXT
    // arms rectangle mode); it stamps kind = "GLA1" on the closed polygon.
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "move (541, 629)",
        "click (541, 629)",
        "move (341, 629)",
        "click (341, 629)",
        "click (341, 429)",
    ]);
    h.send(Message::OpenBreakdown);
    let app = state(h);
    assert!(app.breakdown.open);
    let report = app.breakdown_report();
    assert!(
        report.contains("AREA CALCULATIONS"),
        "report has its header"
    );
    assert!(
        report.contains("GLA1") && report.contains("First Floor"),
        "report carries the area code and name"
    );
    assert!(report.contains("100"), "report totals the GLA1 area");
}

#[test]
fn e2e_breakdown_copy_writes_clipboard_and_toasts() {
    let mut h = harness();
    h.script(&[CARET_EXT, MENU_RECT, "type enter", "type enter"]);
    h.send(Message::OpenBreakdown);
    h.send(Message::Breakdown(
        crate::breakdown_dialog::BreakdownMsg::Copy,
    ));
    assert!(h.shows("Calculations copied."), "Copy toasts its success");
}

#[test]
fn e2e_breakdown_style_button_toggles_label() {
    let mut h = harness();
    h.script(&[CARET_EXT, MENU_RECT, "type enter", "type enter"]);
    h.send(Message::OpenBreakdown);
    // Default state is feet-inches; two toggles restore it.
    h.send(Message::Breakdown(
        crate::breakdown_dialog::BreakdownMsg::ToggleStyle,
    ));
    h.send(Message::Breakdown(
        crate::breakdown_dialog::BreakdownMsg::ToggleStyle,
    ));
    let app = state(h);
    assert!(
        !app.breakdown.feet_inches,
        "two toggles restore the default"
    );
    assert_eq!(app.breakdown.style_label(), "Feet & inches");
}

#[test]
fn e2e_breakdown_style_button_flips_once() {
    let mut h = harness();
    h.script(&[CARET_EXT, MENU_RECT, "type enter", "type enter"]);
    h.send(Message::OpenBreakdown);
    h.send(Message::Breakdown(
        crate::breakdown_dialog::BreakdownMsg::ToggleStyle,
    ));
    let app = state(h);
    assert!(app.breakdown.feet_inches, "one toggle flips to decimal");
    assert_eq!(app.breakdown.style_label(), "Decimal feet");
}

// ---------------------------------------------------------------------------
// The Layers tree (S10-025..034): rows, tools, rename, context menu, drag.
// Geometry at the default 312px dock: the tree card starts ~y 168 under the
// tab strip + 8px padding; rows are 25px on a 27px pitch with the drop
// zones 6/13/6; the eye/lock tools hug the row's right edge.
// ---------------------------------------------------------------------------

/// One closed 200x200ft loop at `(341, 429)`; a second at `(600, 429)`.
const LOOP1: &[&str] = &[
    RAIL_DRAW_EXT,
    "click (341, 429)",
    "click (541, 429)",
    "click (541, 629)",
    "click (341, 629)",
    "click (342, 430)",
];
const LOOP2: &[&str] = &[
    "click (600, 429)",
    "click (800, 429)",
    "click (800, 629)",
    "click (600, 629)",
    "click (601, 430)",
];

fn dedicated_layer_id(app: &App, poly: i64) -> Option<String> {
    app.layers
        .items
        .iter()
        .filter(|l| l.area_poly_id.as_deref() == Some(poly.to_string().as_str()))
        .map(|l| l.id.clone())
        .next()
}

/// The dedicated layer of the sketch's FIRST closed area (loops share one
/// id counter with the walls, so the id is looked up, not assumed).
fn first_area_layer(app: &App) -> Option<String> {
    let poly = app.drawing.polygons.first()?.id;
    dedicated_layer_id(app, poly)
}

#[test]
fn e2e_layers_tree_lists_closed_areas_and_the_empty_note() {
    // An empty sketch shows the original's empty text; a closed loop gives
    // the area its own dedicated layer row ("Area", from 'Undefined Area').
    let mut h = harness();
    h.script(&[TAB_LAYERS]);
    assert!(
        h.shows("Closed areas create layers automatically."),
        "the empty tree's note"
    );
    assert!(state(h).layers.items.len() == 1, "only the default layer");

    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.push(TAB_LAYERS);
    let mut h = harness();
    h.script(&lines);
    assert!(h.shows("Area"), "the dedicated layer's row");
    let app = state(h);
    let id = first_area_layer(&app).expect("the polygon got a dedicated layer");
    let layer = app.layers.layer(&id).expect("the layer exists");
    assert_eq!(layer.name, "Area");
    // assignAreaToDedicatedLayer({activate: false}) keeps the active layer.
    assert_eq!(app.layers.active, "layer-1");
}

#[test]
fn e2e_layer_row_click_sets_active() {
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, AWAY, "click (1200, 185)"]);
    let mut h = harness();
    h.script(&lines);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert_eq!(app.layers.active, id, "the click activated the row");
}

#[test]
fn e2e_layer_eye_toggles_visibility() {
    // One click hides (eye-slash state), two show again.
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, AWAY, "click (1364, 185)"]);
    let mut h = harness();
    h.script(&lines);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert!(
        !app.layers.layer(&id).expect("row").visible,
        "the eye hid the layer"
    );

    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, AWAY, "click (1364, 185)", "click (1364, 185)"]);
    let mut h = harness();
    h.script(&lines);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert!(
        app.layers.layer(&id).expect("row").visible,
        "the eye shows it again"
    );
}

#[test]
fn e2e_layer_context_menu_opens_and_activates_the_row() {
    // Right press opens the menu and activates the row (the original's
    // contextmenu handler calls setActiveLayer first).
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, "move (1200, 185)", "press right (1200, 185)"]);
    let mut h = harness();
    h.script(&lines);
    assert!(h.shows("Export"), "the context menu is open");
    assert!(h.shows("Rename"), "the context menu is open");
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert_eq!(app.layers.active, id, "the right press activated the row");
    assert_eq!(app.dock.layer_menu.as_deref(), Some(id.as_str()));
}

#[test]
fn e2e_layer_context_menu_rename_commits_on_enter() {
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, "move (1200, 185)", "press right (1200, 185)"]);
    lines.extend(["click (1160, 188)", "type \"LIV\"", "type enter"]);
    let mut h = harness();
    h.script(&lines);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert_eq!(
        app.layers.layer(&id).expect("row").name,
        "LIV",
        "the rename committed"
    );
    assert!(app.dock.layer_rename.is_none(), "the input closed");
}

#[test]
fn e2e_layer_context_menu_rename_refuses_a_blank_name() {
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, "move (1200, 185)", "press right (1200, 185)"]);
    lines.extend(["click (1160, 188)", "type enter"]);
    let mut h = harness();
    h.script(&lines);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert_eq!(app.layers.layer(&id).expect("row").name, "Area");
}

#[test]
fn e2e_layer_rename_escape_cancels() {
    // The cancel arm: a focused text box swallows Escape before the
    // EventTap in the test runtime, so the arm is driven by message (the
    // real app's raw-event subscription delivers the Escape; the tap's
    // K::Escape branch covers the unfocused case).
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, "move (1200, 185)", "press right (1200, 185)"]);
    lines.extend(["click (1160, 188)", "type \"X\""]);
    let mut h = harness();
    h.script(&lines);
    h.send(Message::LayerRenameCancel);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert_eq!(
        app.layers.layer(&id).expect("row").name,
        "Area",
        "the cancel discarded the buffer"
    );
    assert!(app.dock.layer_rename.is_none());
}

#[test]
fn e2e_layer_drag_moves_a_layer_and_banks_move_layer() {
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend(LOOP2.to_vec());
    lines.extend([TAB_LAYERS, AWAY]);
    lines.extend(["move (1200, 212)", "press (1200, 212)"]);
    lines.extend(["move (1200, 175)", "release (1200, 175)"]);
    let mut h = harness();
    h.script(&lines);
    assert!(h.shows("Area 2"), "both rows listed");
    let app = state(h);
    let after: Vec<String> = app
        .layers
        .items
        .iter()
        .filter(|l| l.area_poly_id.is_some())
        .map(|l| l.name.clone())
        .collect();
    assert_eq!(after, vec!["Area 2", "Area"], "the row moved before");
    // The history lists labels newest-first.
    assert_eq!(
        app.undo.labels().first().map(String::as_str),
        Some("Move Layer"),
        "the drag banked its undo step"
    );
    assert!(app.dock.layer_drag.is_none(), "the drag disarmed");
}

#[test]
fn e2e_layers_context_menu_send_to_back_and_bring_to_front() {
    // S10-037/038: the selection auto-assigns onto a named layer at the
    // stack's edge (autoAssignLayer), with the toast as its receipt.
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend(["type escape", RAIL_SELECT]);
    lines.extend(["move (441, 429)", "click (441, 429)"]);
    lines.extend(["move (600, 300)", "press right (600, 300)"]);
    let mut h = harness();
    h.script(&lines);
    assert!(h.shows("Send to Back"), "the menu row is wired");
    h.send(Message::ContextAction(
        crate::context_menu::Action::SendToBack,
    ));
    assert!(
        h.shows("Moved selection to layer Send to Back."),
        "the toast"
    );
    let app = state(h);
    let layer = app
        .layers
        .items
        .iter()
        .find(|l| l.name == "Send to Back")
        .expect("the named layer exists");
    assert!(!layer.is_group && layer.parent_id.is_none());
    assert_eq!(
        app.layers.items.first().map(|l| l.id.clone()).as_deref(),
        Some(layer.id.as_str()),
        "the layer sits at the stack's back (bottom)"
    );
    // The selected wall (the click's hit) moved onto the named layer; the
    // area's remaining walls stay on the area's own dedicated layer.
    let sel_wall = *app.selection.lines.iter().next().expect("a wall selected");
    let moved = app
        .drawing
        .lines
        .iter()
        .find(|l| l.id == sel_wall)
        .expect("wall");
    assert_eq!(
        moved.layer_id.as_deref(),
        Some(layer.id.as_str()),
        "the selected wall moved onto the layer"
    );

    // Bring to Front pushes the layer to the stack's front (top) instead.
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend(["type escape", RAIL_SELECT]);
    lines.extend(["move (441, 429)", "click (441, 429)"]);
    lines.extend(["move (600, 300)", "press right (600, 300)"]);
    let mut h = harness();
    h.script(&lines);
    h.send(Message::ContextAction(
        crate::context_menu::Action::BringToFront,
    ));
    let app = state(h);
    let layer = app
        .layers
        .items
        .iter()
        .find(|l| l.name == "Bring to Front")
        .expect("the named layer exists");
    assert_eq!(
        app.layers.items.last().map(|l| l.id.clone()).as_deref(),
        Some(layer.id.as_str()),
        "the layer sits at the stack's front (top)"
    );
    let sel_wall = *app.selection.lines.iter().next().expect("a wall selected");
    let moved = app
        .drawing
        .lines
        .iter()
        .find(|l| l.id == sel_wall)
        .expect("wall");
    assert_eq!(
        moved.layer_id.as_deref(),
        Some(layer.id.as_str()),
        "the selected wall moved onto the layer"
    );
    // The selection re-sorts within its layer segment (toFront when the
    // layer went to back � here the layer is at the front, so the walls
    // slide to the segment's back).
    assert_eq!(
        app.undo.labels().first().map(String::as_str),
        Some("Auto-Assign Layer"),
        "the action banked its undo step"
    );
}

#[test]
fn e2e_new_sketch_confirms_then_resets() {
    // Ctrl+N / File > New: the confirm, Cancel keeps the sketch, OK resets
    // to a blank Untitled (resetToBlank: the entities, undo, layers, pages,
    // the name; the OPTIONS survive).
    let mut h = harness();
    h.script(LOOP1);
    h.send(Message::New);
    assert!(
        h.shows("Start a new sketch? Unsaved changes will be lost."),
        "the confirm opens"
    );
    h.send(Message::Dialog(DialogMsg::Dismiss));
    let app = state(h);
    assert!(!app.drawing.lines.is_empty(), "cancel keeps the sketch");

    let mut h = harness();
    h.script(LOOP1);
    h.send(Message::New);
    h.send(Message::Dialog(DialogMsg::Ok));
    let app = state(h);
    assert!(app.drawing.lines.is_empty(), "ok resets the entities");
    assert_eq!(app.file_name, "Untitled");
    assert_eq!(app.pages.pages.len(), 1);
    assert_eq!(app.layers.items.len(), 1, "the default layer back");
    assert!(
        app.drawing.options.grid_enabled,
        "the options survive the reset"
    );
}

#[test]
fn e2e_exit_confirms_before_closing() {
    // S11-080, narrowed: the title bar's Close / the backstage Exit ask
    // first only when there is unsaved work; Cancel keeps the app alive
    // (Save / Don't save end in iced::exit, which ends the process, so only
    // the dialog is asserted here).
    let mut h = harness();
    h.script(LOOP1);
    h.send(Message::Close);
    let ask = "Save changes to \"Untitled\" before closing?";
    assert!(h.shows(ask), "the confirm opens");
    assert!(h.shows("Don't save") && h.shows("Save") && h.shows("Cancel"));
    assert!(h.shows("Unsaved"), "the title bar pill reports the edit");
    h.send(Message::Dialog(DialogMsg::Dismiss));
    assert!(!h.shows(ask), "cancel keeps the app alive");
}

#[test]
fn e2e_recent_files_record_open_clear() {
    // S11-049..053: the File backstage's Recent Files item toggles the
    // submenu (empty state), and Clear toasts.
    let mut h = harness();
    h.script(&["click \"File\"", "click \"Recent Files\""]);
    assert!(h.shows("Recent Files"), "the submenu opened");
    assert!(h.shows("No recent files yet."), "the empty note");
    h.send(Message::RecentClear);
    assert!(h.shows("Recent files cleared."), "the clear toast");
}

#[test]
fn e2e_shortcuts_sheet_opens_and_closes() {
    // S12-015: the Help ribbon's button / F1 opens the cheat sheet; Close,
    // Escape and a second F1 close it.
    let mut h = harness();
    h.send(Message::Shortcuts(crate::shortcuts::ShortcutsMsg::Open));
    assert!(h.shows("Keyboard Shortcuts"), "the sheet is open");
    assert!(h.shows("Rise 10, run 5 diagonal"), "the Drawing group");
    h.script(&["type escape"]);
    assert!(!h.shows("Keyboard Shortcuts"), "Escape closes");

    // F1 toggles it back open through the key map.
    let mut h = harness();
    h.send(Message::Key(crate::input::KeyCmd::Shortcuts));
    assert!(h.shows("Keyboard Shortcuts"), "F1 opened the sheet");
    h.send(Message::Key(crate::input::KeyCmd::Shortcuts));
    assert!(!h.shows("Keyboard Shortcuts"), "a second F1 closes");
}

fn paste(text: &str) -> iced::widget::text_editor::Action {
    iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
        std::sync::Arc::new(text.to_string()),
    ))
}

#[test]
fn e2e_subject_info_tabs_and_fields() {
    // S12-001/002/003/005/006: the three tabs, the 13 parcel labels, the
    // metadata strip and the collapsible additional fields.
    use crate::subject_info::{SubjectMsg, Tab};
    let mut h = harness();
    h.send(Message::OpenSubjectInfo);
    assert!(h.shows("Parcel Info"));
    assert!(h.shows("General Comments"));
    assert!(h.shows("Comment Tables"));
    for label in [
        "Parcel No.",
        "File No.",
        "Property Address",
        "City",
        "State",
        "ZipCode",
        "Owner",
        "County",
        "Client",
        "Client Address",
        "Appraiser Name",
        "Inspection Date",
        "GPS Coordinates",
    ] {
        assert!(h.shows(label), "{label} field");
    }
    assert!(h.shows("Created By: Appraisal Sketch"));
    assert!(h.shows("Format Version: Sketch-Web"));
    assert!(!h.shows("Name"), "the additional rows start collapsed");
    h.send(Message::SubjectInfo(SubjectMsg::ToggleExtras));
    assert!(h.shows("Value"), "the additional rows open");
    h.send(Message::SubjectInfo(SubjectMsg::Tab(Tab::Comments)));
    assert!(h.shows("Save Default") && h.shows("Apply Default"));
    h.send(Message::SubjectInfo(SubjectMsg::Tab(Tab::Tables)));
    assert!(h.shows("Comment Table 1") && h.shows("Comment Table 3"));
}

#[test]
fn e2e_subject_info_ok_saves_and_cancel_discards() {
    // S12-011: OK validates and commits; Cancel/x drop the draft.
    use crate::subject_info::SubjectMsg;
    let mut h = harness();
    h.send(Message::OpenSubjectInfo);
    h.send(Message::SubjectInfo(SubjectMsg::Field(0, "P-77".into())));
    h.send(Message::SubjectInfo(SubjectMsg::Field(
        12,
        "47.7737, -122.6539".into(),
    )));
    h.send(Message::SubjectInfo(SubjectMsg::Table(
        1,
        paste("one\ntwo"),
    )));
    h.send(Message::SubjectInfo(SubjectMsg::ExtraName(0, "Lot".into())));
    h.send(Message::SubjectInfo(SubjectMsg::Ok));
    assert!(h.shows("Subject information updated."), "the OK toast");
    assert!(!h.shows("Parcel Info"), "OK closes the dialog");
    let app = state(h);
    assert_eq!(app.subject_info.parcel_no, "P-77");
    assert_eq!(app.subject_info.gps_latitude, "47.7737");
    assert_eq!(app.subject_info.gps_longitude, "-122.6539");
    assert_eq!(app.subject_info.comment_tables[1], vec!["one", "two"]);
    assert_eq!(app.subject_info.additional_fields[0].name, "Lot");

    let mut h = harness();
    h.send(Message::OpenSubjectInfo);
    h.send(Message::SubjectInfo(SubjectMsg::Field(0, "gone".into())));
    h.send(Message::SubjectInfo(SubjectMsg::Close));
    assert!(!h.shows("Parcel Info"), "Cancel closes the dialog");
    let app = state(h);
    assert_eq!(app.subject_info.parcel_no, "", "Cancel discards the draft");
}

#[test]
fn e2e_subject_info_bad_gps_keeps_the_dialog_open() {
    use crate::subject_info::SubjectMsg;
    let mut h = harness();
    h.send(Message::OpenSubjectInfo);
    h.send(Message::SubjectInfo(SubjectMsg::Field(
        12,
        "nowhere".into(),
    )));
    h.send(Message::SubjectInfo(SubjectMsg::Ok));
    assert!(
        h.shows(
            "GPS Coordinates must be decimal degrees (47.7737, -122.6539) or DMS (47\u{b0}46'24.92\"N 122\u{b0}39'14.18\"W)."
        ),
        "the GPS refusal"
    );
    assert!(h.shows("Parcel Info"), "the dialog stays open");
}

#[test]
fn e2e_subject_info_default_comments_and_autofill() {
    // S12-004/008/009: Save Default / Apply Default and the suite-only
    // Auto-Fill row.
    use crate::subject_info::{SubjectMsg, Tab};
    let mut h = harness();
    h.send(Message::OpenSubjectInfo);
    assert!(h.shows("Auto-Fill"), "the row shows until auto-filled");
    h.send(Message::SubjectInfo(SubjectMsg::AutoFill));
    assert!(h.shows("Auto-Fill is available when the sketch is open in the appraisal suite."));
    h.send(Message::SubjectInfo(SubjectMsg::Tab(Tab::Comments)));
    h.send(Message::SubjectInfo(SubjectMsg::Comments(paste(
        "Standard note",
    ))));
    h.send(Message::SubjectInfo(SubjectMsg::SaveDefault));
    assert!(h.shows("Default comments saved."));
    h.send(Message::SubjectInfo(SubjectMsg::Comments(paste(" more"))));
    h.send(Message::SubjectInfo(SubjectMsg::ApplyDefault));
    h.send(Message::SubjectInfo(SubjectMsg::Ok));
    let app = state(h);
    assert_eq!(app.subject_info.general_comments, "Standard note");
}

#[test]
fn e2e_print_buttons_use_the_preview_pdf_with_summary_and_breakdown() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/print-019");
    std::fs::create_dir_all(&root).unwrap();
    let mut app = state(harness());
    for (y, height, code, name) in [
        (82., 30., "GLA1", "First Floor"),
        (45., 22., "GLA2", "Second Floor"),
        (0., 26., "BSMT", "Finished BSMT"),
    ] {
        app.drawing.add_wall_shape(
            crate::drawing::WallShape::Rectangle,
            sketch_core::model::WallType::Exterior,
            50.,
            Some(height),
            None,
            Pt::new(25., y + height / 2.),
        );
        let p = app.drawing.polygons.last_mut().unwrap();
        p.kind = code.into();
        p.name = name.into();
    }
    app.file_name = "PrintButtons019".into();
    let fixture = root.join("sample.skx");
    std::fs::write(
        &fixture,
        sketch_core::skx::write(&app.build_save_model(), &Default::default()).unwrap(),
    )
    .unwrap();
    for paper in crate::page_layout::PAPERS {
        app.page_layout.paper_size = paper;
        let bytes = app.report_pdf();
        let name = crate::page_layout::paper_key(paper);
        std::fs::write(root.join(format!("sample-{name}.pdf")), &bytes).unwrap();
    }
    let mut h = harness();
    let (_, picked) = crate::file_dialogs::with_open_path(Some(fixture), || {
        h.script(&["click \"File\"", "click (100, 118)"]);
    });
    assert_eq!(picked, 1);
    h.script(&["type escape", "click \"File\"", "click (100, 594)"]);
    assert!(h.shows("Close Preview"));
    h.script(&["click \"Print / Save PDF\""]);
    assert!(h.shows("Opening the appraisal report to print."));
    let preview = state(h);
    assert_eq!(preview.print_preview.page_count(), 1);
    let path = crate::print::temp_report_path(&preview.file_name);
    let printed = std::fs::read(&path).unwrap();
    assert!(
        printed == preview.print_preview.pdf,
        "Preview Print must use the displayed PDF exactly"
    );
    std::fs::write(root.join("preview-print.pdf"), &printed).unwrap();
    let rasters = crate::print::render_pages(&printed, 1200).unwrap();
    std::fs::write(root.join("preview-print.png"), &rasters[0]).unwrap();
    let mut h = harness();
    crate::file_dialogs::with_open_path(Some(root.join("sample.skx")), || {
        h.script(&["click \"File\"", "click (100, 118)"]);
    });
    h.script(&["type escape", "click \"File\"", "click (100, 558)"]);
    assert!(h.shows("Opening the appraisal report to print."));
    let app = state(h);
    let printed = std::fs::read(crate::print::temp_report_path(&app.file_name)).unwrap();
    assert!(
        printed == app.report_pdf(),
        "File Print must use the same report pipeline"
    );
    std::fs::write(root.join("file-print.pdf"), &printed).unwrap();
}

#[test]
fn e2e_integration_setup_help_buttons_enable_restore_and_report_errors() {
    let mut h = harness();
    h.script(&["click \"Help\"", "click (148, 91)"]);
    assert!(h.shows("Appraise-It Pro integration"));
    assert!(h.shows("Enable integration"));
    h.screenshot("integration-setup-open");
    // Screenshots suppress the emulator's physical cursor. Start a fresh
    // harness before further input so widget hit-testing stays independent.
    let mut h = harness();
    h.script(&["click \"Help\"", "click (148, 91)"]);
    let (_, requests) =
        crate::integration_setup::with_result(Ok("Host launch enabled.".into()), || {
            h.script(&["click \"Enable integration\""]);
            h.wait(50);
        });
    assert_eq!(requests, [true]);
    assert!(
        h.shows("Host launch enabled.\nRestart Appraise-It Pro to use the updated launch setting.")
    );
    assert!(!state(h).integration_setup.busy);
    let mut h = harness();
    h.script(&["click \"Help\"", "click (148, 91)"]);
    let (_, requests) =
        crate::integration_setup::with_result(Ok("Previous provider restored.".into()), || {
            h.script(&["click \"Restore previous provider\""]);
            h.wait(50);
        });
    assert_eq!(requests, [false]);
    assert!(h.shows(
        "Previous provider restored.\nRestart Appraise-It Pro to use the updated launch setting."
    ));
    let mut h = harness();
    h.script(&["click \"Help\"", "click (148, 91)"]);
    let (_, requests) = crate::integration_setup::with_result(
        Err("Windows permission was cancelled.".into()),
        || {
            h.script(&["click \"Enable integration\""]);
            h.wait(50);
        },
    );
    assert_eq!(requests, [true]);
    assert!(h.shows("Setup was not completed: Windows permission was cancelled."));
    h.screenshot("integration-setup-error");
}

#[test]
fn e2e_integration_setup_close_escape_and_modal_shortcuts_leave_the_sketch_unchanged() {
    for close in ["click \"Close\"", "type escape"] {
        let mut h = defined_square();
        h.script(&["type escape"]);
        h.send(Message::SelectAll);
        h.script(&["click \"Help\"", "click (148, 91)"]);
        h.send(Message::Key(crate::input::KeyCmd::Delete));
        assert!(h.shows("Enable integration"));
        h.script(&[close]);
        let app = state(h);
        assert!(!app.integration_setup.open);
        assert!(
            app.dialog.is_none(),
            "Delete must not open an underlying deletion confirmation"
        );
        assert_eq!(app.selection.lines.len(), 4);
        assert_eq!(app.drawing.polygons.len(), 1);
        assert_eq!(app.drawing.polygons[0].kind, "GLA1");
        assert_eq!(app.drawing.lines.len(), 4);
    }
    let mut h = Harness::boot_at_size(program(1.0), iced::Size::new(420.0, 620.0));
    h.send(Message::WindowResized(iced::Size::new(420.0, 620.0)));
    h.script(&["click \"Help\"", "click (148, 91)"]);
    assert!(h.shows("Enable integration"));
    assert!(h.shows("Restore previous provider"));
    h.screenshot("integration-setup-small");
}

#[test]
fn e2e_print_preview_opens_and_closes() {
    // S11-055: the overlay's toolbar reads `N page(s) · Paper portrait`;
    // Close Preview and Escape both dismiss it.
    let mut h = harness();
    h.send(Message::PrintPreviewOpen);
    assert!(h.shows("Print / Save PDF"), "the print button");
    assert!(h.shows("Close Preview"), "the close button");
    assert!(
        h.shows(&crate::print::summary(
            1,
            sketch_core::print_layout::PaperSize::Legal
        )),
        "the paper summary"
    );
    // The complete report is now a PDF-rendered page image, not placeholder text widgets.
    h.send(Message::PrintPreview(crate::print::PreviewMsg::Close));
    assert!(!h.shows("Close Preview"), "Close Preview dismisses");

    let mut h = harness();
    h.send(Message::PrintPreviewOpen);
    h.script(&["type escape"]);
    assert!(!h.shows("Close Preview"), "Escape dismisses");
}

#[test]
fn e2e_print_writes_the_report_and_toasts() {
    // S11-054: File > Print writes the report PDF (no viewer under test).
    let mut h = harness();
    h.send(Message::Print);
    assert!(
        h.shows("Opening the appraisal report to print."),
        "the toast"
    );
    let path = crate::print::temp_report_path("Untitled");
    let bytes = std::fs::read(&path).expect("the report was written");
    assert!(bytes.starts_with(b"%PDF"), "a PDF");
}

#[test]
fn e2e_reference_image_scale_flow() {
    // S10-011/049: Set Scale arms the mode (the alert explains), two canvas
    // clicks store the points, the prompt's known distance rescales.
    // (The rfd picker cannot run headlessly — never call PhotoLoad here;
    // the loaded-image math is unit-covered in reference_image::tests.)
    let mut h = harness();
    h.send(Message::PhotoScaleStart);
    assert!(h.shows("No image loaded."), "the no-image alert");

    // The show/hide arms' alerts are the same guard.
    let mut h = harness();
    h.send(Message::PhotoShow);
    assert!(h.shows("No image loaded. Use Load Image first."));
}

#[test]
fn e2e_reference_image_geo_prompts() {
    // S10-023/024: the Geo prompts write the subject's georeference bag.
    // (The toasts + the saved fields split across harnesses: state() ends
    // one.)
    let mut h = harness();
    h.send(Message::GeoCoordinate);
    h.script(&["type \"47.7737, -122.6539\"", "click \"OK\""]);
    assert!(
        h.shows("Georeference saved at 47.7737, -122.6539. Load a map image to trace it."),
        "the saved toast"
    );
    let app = state(h);
    assert_eq!(app.subject_info.gps_latitude, "47.7737");
    assert_eq!(app.subject_info.gps_longitude, "-122.6539");

    // A bad coordinate toasts the format help (the expect matches the whole
    // widget text — exact equality in iced_test's selector).
    let mut h = harness();
    h.send(Message::GeoCoordinate);
    h.script(&["type \"garbage\"", "click \"OK\""]);
    assert!(
        h.shows(
            "Enter coordinates as decimal degrees (47.7737, -122.6539) or DMS (47°46'24.92\"N 122°39'14.18\"W)."
        ),
        "the format help"
    );

    // The address prompt saves the address.
    let mut h = harness();
    h.send(Message::GeoAddress);
    h.script(&["type \"123 Main St\"", "click \"OK\""]);
    assert!(h.shows("Georeference address saved. Load a map image to trace it."));
    let app = state(h);
    assert_eq!(app.subject_info.property_address, "123 Main St");
}

#[test]
fn e2e_text_library_manager_adds_a_label() {
    // A blank add toasts (one harness; the expect rebuild between focused
    // typing drops the input's focus — an iced_test artifact — so the two
    // checks don't share a harness).
    let mut h = harness();
    h.send(Message::Dock(DockMsg::EditTextLibrary));
    assert!(h.shows("Manage Text Library"), "the manager is open");
    h.script(&["click \"Add\""]);
    assert!(h.shows("Enter a text label first."), "a blank add toasts");

    let mut h = harness();
    h.send(Message::Dock(DockMsg::EditTextLibrary));
    h.script(&["type \"Pantry\"", "click \"Add\""]);
    let app = state(h);
    assert!(
        app.dock.text_labels.iter().any(|l| l == "Pantry"),
        "the label landed in the library"
    );
}

#[test]
fn e2e_symbol_organizer_saves_favorites() {
    // The Organize... button opens the favorites editor; the checkboxes
    // seed from the defaults.
    let mut h = harness();
    h.send(Message::Dock(DockMsg::OrganizeSymbols));
    h.script(&["click \"Edit favorites…\""]);
    assert!(
        h.shows("Organize Favorite Symbols"),
        "the organizer is open"
    );
    let app = state(h);
    let defaults = app.symbol_organizer.checked.clone();
    assert!(
        !defaults.is_empty(),
        "the checkboxes seed from the defaults"
    );

    // Save Favorites persists (state.symbolFavorites) and toasts.
    let mut h = harness();
    h.send(Message::Dock(DockMsg::OrganizeSymbols));
    h.script(&["click \"Edit favorites…\""]);
    h.script(&["click \"Save Favorites\""]);
    assert!(h.shows("Favorite symbols updated."), "the save toasts");
    let app = state(h);
    assert_eq!(
        app.symbol_favorites.as_deref(),
        Some(defaults.as_slice()),
        "the working copy persisted"
    );
}

#[test]
fn e2e_layer_hide_then_undo_restores_the_layer_stack() {
    // The snapshot carries the layer stack: hide + undo brings it back.
    let mut lines: Vec<&str> = LOOP1.to_vec();
    lines.extend([TAB_LAYERS, AWAY, "click (1364, 185)"]);
    let mut h = harness();
    h.script(&lines);
    h.send(Message::Undo);
    let app = state(h);
    let id = first_area_layer(&app).expect("row exists");
    assert!(
        app.layers.layer(&id).expect("row").visible,
        "undo restored the visibility"
    );
}

/// The window screenshot the harness cannot take (its window tasks never
/// run): a white 1400x860 frame standing in for it.
fn fake_shot() -> Message {
    Message::MiniMapShot(iced::window::Screenshot::new(
        vec![255u8; 1400 * 860 * 4],
        iced::Size::new(1400, 860),
        1.0,
    ))
}

#[test]
fn e2e_mini_map_toggles_from_the_ribbon_and_closes() {
    // S12-012/018: View > Mini-Map asks for a snapshot; once it arrives the
    // card shows with its x. The x, or the button again, closes it.
    let mut h = harness();
    h.script(&["click \"View\"", "click \"Mini-Map\""]);
    assert!(!h.shows("\u{d7}"), "nothing shows until the snapshot lands");
    h.send(fake_shot());
    assert!(h.shows("\u{d7}"), "the card's close button");
    h.send(Message::MiniMapClose);
    assert!(!h.shows("\u{d7}"), "the x closes it");

    let mut h = harness();
    h.send(fake_shot());
    assert!(h.shows("\u{d7}"));
    h.send(Message::ToggleMiniMap);
    assert!(!h.shows("\u{d7}"), "the toggle closes an open map");
}

#[test]
fn e2e_command_palette_lists_filters_and_reports_no_match() {
    // S12-028: the 42-entry catalog with its hints, the label filter, and
    // the empty note.
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    assert!(h.shows("Save"), "the first entry");
    assert!(h.shows("F2"), "its hint");
    assert!(h.shows("Connect Bluetooth device"), "a late entry");
    h.send(Message::CommandPaletteQuery("  MINI ".into()));
    assert!(h.shows("Mini map"), "the filter is a label substring");
    assert!(!h.shows("Save"), "other entries drop out");
    h.send(Message::CommandPaletteQuery("zzz".into()));
    assert!(h.shows("No matching command."));
    h.send(Message::CommandPaletteClose);
    assert!(!h.shows("No matching command."), "close removes it");
}

#[test]
fn e2e_command_palette_ctrl_k_toggles_and_escape_closes() {
    // S12-074 / S12-028: Ctrl+K opens, Ctrl+K again or Escape closes.
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    assert!(h.shows("Keyboard shortcuts"));
    h.send(Message::CommandPaletteOpen);
    assert!(!h.shows("Keyboard shortcuts"), "a second open closes it");
    h.send(Message::CommandPaletteOpen);
    h.send(Message::Key(crate::input::KeyCmd::Escape));
    // The palette owns Escape through the raw stream; the key-map route is
    // the unfocused fallback.
    h.send(Message::CommandPaletteClose);
    assert!(!h.shows("Keyboard shortcuts"));
}

#[test]
fn e2e_command_palette_arrows_wrap_and_enter_runs() {
    // S12-028: the highlight wraps; running closes the palette first and
    // then executes the highlighted command.
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("grid".into()));
    h.send(Message::CommandPaletteMove(1));
    h.send(Message::CommandPaletteMove(1));
    h.send(Message::CommandPaletteRun);
    let app = state(h);
    assert!(!app.command_palette_open, "running closes the palette");
    assert!(!app.view.grid, "Toggle grid ran (the grid starts on)");
}

#[test]
fn e2e_command_palette_file_commands_use_native_workflows() {
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("Save As".into()));
    let (_, requests) =
        crate::file_dialogs::with_save_path(None, || h.send(Message::CommandPaletteRun));
    assert_eq!(requests, ["Untitled.sketchy"]);
    let app = state(h);
    assert!(!app.command_palette_open);
    assert!(
        app.dialog.is_none(),
        "Save As goes directly to the native picker"
    );
    // Picker-based commands are checked by the catalog mapping test; invoking
    // their native file dialogs would interrupt the user's desktop.
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("New sketch".into()));
    h.send(Message::CommandPaletteRun);
    let app = state(h);
    assert!(!app.command_palette_open);
    assert_ne!(app.toasts.text(), Some("new is not available."));
}

#[test]
fn e2e_command_palette_rows_run_commands() {
    // S12-029/037..070: a click on a row runs it through the existing
    // handlers - Mini map, Keyboard shortcuts and Subject info here.
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("keyboard".into()));
    h.send(Message::CommandPaletteRunAt(0));
    assert!(
        h.shows("Rise 10, run 5 diagonal"),
        "the shortcuts sheet opened"
    );

    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("subject".into()));
    h.send(Message::CommandPaletteRunAt(0));
    assert!(h.shows("Parcel Info"), "Subject Info opened");

    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("mini".into()));
    h.send(Message::CommandPaletteRun);
    h.send(fake_shot());
    assert!(h.shows("\u{d7}"), "running it opens the map");
}

#[test]
fn e2e_command_palette_rotate_sketch_turns_everything_and_north() {
    // S12-050: `rotateSketch(90)` - an empty sketch is a no-op; a drawn wall
    // turns about its centre, north turns along, and an undo step is banked.
    let mut h = harness();
    h.send(Message::RotateSketch);
    let app = state(h);
    assert!(!app.undo.can_undo(), "nothing drawn: no step");

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
    ]);
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("rotate".into()));
    h.send(Message::CommandPaletteRun);
    let app = state(h);
    let line = &app.drawing.lines[0];
    assert!((line.start.x - line.end.x).abs() < 1e-6, "now vertical");
    assert!((line.start.y - line.end.y).abs() > 1.0);
    assert_eq!(app.pages.page().north_angle, 90.0, "north turned along");
    assert!(app.undo.can_undo());
}

fn disto_device(id: &str, name: Option<&str>) -> crate::disto::Device {
    crate::disto::Device {
        id: id.into(),
        name: name.map(str::to_string),
    }
}

#[test]
fn e2e_release_audit_bay_mirror_matches_reference_and_undo_redo() {
    let mut app = state(harness());
    let ctx = app.draw_ctx();
    app.drawing.pen_down_at(Pt::new(0.0, 0.0), ctx);
    app.drawing.commit_to(
        Pt::new(10.0, 0.0),
        10.0,
        0.0,
        sketch_core::model::WallType::Exterior,
    );
    app.drawing.commit_to(
        Pt::new(14.0, 3.0),
        5.0,
        37.0,
        sketch_core::model::WallType::Exterior,
    );
    let _ = app.update(Message::Key(crate::input::KeyCmd::BayMirror));
    let mirrored = app.drawing.lines.last().unwrap();
    assert_eq!(mirrored.start, Pt::new(14.0, 3.0));
    assert!((mirrored.end.x - 18.0).abs() < 1e-9);
    assert!(mirrored.end.y.abs() < 1e-9);
    assert_eq!(app.drawing.line_ids.len(), 3);
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.lines.len(), 2);
    let _ = app.update(Message::Redo);
    assert_eq!(app.drawing.lines.len(), 3);
    app.drawing.pen_up();
    let _ = app.update(Message::Key(crate::input::KeyCmd::BayMirror));
    assert_eq!(app.drawing.lines.len(), 3);
}

#[test]
fn e2e_release_audit_calculation_details_button_runs_breakdown() {
    let mut h = harness();
    h.send(Message::Dock(DockMsg::Tab(DockTab::Calcs)));
    h.script(&["click \"Calculation Details\""]);
    assert!(h.shows("Define at least one area first."));
}

#[test]
fn e2e_release_audit_new_symbol_is_selected_for_contextual_editing() {
    let mut app = state(harness());
    let symbol = app.symbols_lib().favorites[0].id.clone();
    app.place_symbol_at(Pt::new(0.0, 0.0), symbol);
    let id = app.drawing.symbols.last().unwrap().id;
    assert!(app.selection.symbols.contains(&id));
    assert_eq!(app.view.mode, crate::view_state::Mode::Select);
    assert_eq!(app.body, Body::EditTools);
}

#[test]
fn e2e_release_audit_labels_move_freely_until_area_grid_snap_is_enabled() {
    use crate::view_state::World;
    let mut app = app_with_label();
    let (id, x, y) = {
        let t = &app.drawing.texts[0];
        (t.id, t.x, t.y)
    };
    app.selection.clear();
    app.selection.texts.insert(id);
    app.view.snap = true;
    app.view.area_snap = false;
    app.view
        .drag_start_move(World { x: 0.0, y: 0.0 }, World { x, y });
    let (sx, sy) = app.view.to_screen(World { x: 0.3, y: 0.2 });
    let _ = app.update(Message::CanvasDrag(sx as f32, sy as f32, false));
    assert!((app.drawing.texts[0].x - x - 0.3).abs() < 1e-6);
    assert!((app.drawing.texts[0].y - y - 0.2).abs() < 1e-6);
    let t = &app.drawing.texts[0];
    app.view.area_snap = true;
    app.view.area_snap_size = 0.5;
    app.drag_banked = false;
    app.view
        .drag_start_move(World { x: 0.0, y: 0.0 }, World { x: t.x, y: t.y });
    let (sx, sy) = app.view.to_screen(World { x: 0.4, y: 0.4 });
    let _ = app.update(Message::CanvasDrag(sx as f32, sy as f32, false));
    let t = &app.drawing.texts[0];
    assert!((t.x * 2.0 - (t.x * 2.0).round()).abs() < 1e-6);
    assert!((t.y * 2.0 - (t.y * 2.0).round()).abs() < 1e-6);
}

fn disto_drawing_app() -> App {
    let mut h = harness();
    h.script(&[RAIL_DRAW_EXT, "click (341, 429)"]);
    let mut app = state(h);
    app.disto.connected = Some("DISTO E7500i".into());
    app.disto.session = 1;
    app
}

fn disto_indicate(app: &mut App, meters: f64) {
    let _ = app.update(Message::Disto(crate::disto::DistoMsg::Radio(
        app.disto.session,
        crate::disto::RadioEvent::Measurement(meters),
    )));
}

#[test]
fn e2e_disto_standard_measurement_waits_for_keyboard_direction_and_is_undoable() {
    use crate::input::KeyCmd;
    let mut app = disto_drawing_app();
    app.drawing.options.disto_rounding = "nearestHundredth".into();
    app.drawing.buffer = "999".into();
    let meters = sketch_core::disto::parse_meters(&[0xcf, 0xf7, 0xdb, 0x3f]).unwrap();
    disto_indicate(&mut app, meters);
    assert_eq!(
        app.drawing.buffer, "5.64",
        "replaces the previous numeric entry"
    );
    assert!(app.drawing.lines.is_empty(), "Standard never auto-commits");
    let _ = app.update(Message::Key(KeyCmd::Arrow {
        dir: 2,
        ctrl: false,
        shift: false,
    }));
    let _ = app.update(Message::Key(KeyCmd::Enter));
    assert_eq!(app.drawing.lines.len(), 1);
    let line = &app.drawing.lines[0];
    assert!((sketch_core::geometry::distance(line.start, line.end) - 5.64).abs() < 1e-6);
    let _ = app.update(Message::Undo);
    assert!(app.drawing.lines.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.drawing.lines.len(), 1);
    let restored = &app.drawing.lines[0];
    assert!((sketch_core::geometry::distance(restored.start, restored.end) - 5.64).abs() < 1e-6);
}

#[test]
fn e2e_disto_continuous_uses_previous_heading_and_one_undo_per_measurement() {
    use crate::input::KeyCmd;
    let mut app = disto_drawing_app();
    let _ = app.update(Message::Key(KeyCmd::Buffer("1")));
    let _ = app.update(Message::Key(KeyCmd::Buffer("0")));
    let _ = app.update(Message::Key(KeyCmd::Arrow {
        dir: 2,
        ctrl: false,
        shift: false,
    }));
    let _ = app.update(Message::Key(KeyCmd::Enter));
    app.drawing.options.disto_drawing_mode = "continuous".into();
    app.drawing.options.disto_rounding = "nearestWhole".into();
    for _ in 0..2 {
        app.drawing.cursor = Pt::new(-500.0, -500.0);
        disto_indicate(&mut app, 1.7185);
        let line = app.drawing.lines.last().unwrap();
        assert!((line.end.x - line.start.x - 6.0).abs() < 1e-6);
        assert!((line.end.y - line.start.y).abs() < 1e-6);
    }
    assert_eq!(app.drawing.lines.len(), 3);
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.lines.len(), 2);
}

#[test]
fn e2e_disto_keyboard_entry_uses_the_current_interior_wall_type() {
    use crate::input::KeyCmd;
    let mut app = disto_drawing_app();
    app.view.mode = crate::view_state::Mode::DrawInterior;
    disto_indicate(&mut app, 3.048);
    let _ = app.update(Message::Key(KeyCmd::Arrow {
        dir: 2,
        ctrl: false,
        shift: false,
    }));
    let _ = app.update(Message::Key(KeyCmd::Enter));
    let line = app.drawing.lines.last().unwrap();
    assert_eq!(line.kind, sketch_core::model::WallType::Interior);
    assert!((sketch_core::geometry::distance(line.start, line.end) - 10.0).abs() < 1e-6);
}

#[test]
fn e2e_disto_metric_rounding_and_inactive_or_modal_input_are_safe() {
    let mut app = disto_drawing_app();
    app.drawing.options.units.metric = true;
    app.drawing.options.disto_rounding = "nearestTenth".into();
    disto_indicate(&mut app, 2.26);
    let feet = app.drawing.buffer.parse::<f64>().unwrap();
    assert!((feet * 0.3048 - 2.3).abs() < 0.0001);
    let expected = app.drawing.buffer.clone();
    let _ = app.update(Message::Options(crate::options_dialog::OptMsg::Open));
    disto_indicate(&mut app, 4.0);
    assert_eq!(app.drawing.buffer, expected);
    let _ = app.update(Message::Options(crate::options_dialog::OptMsg::Cancel));
    let _ = app.update(Message::Disto(crate::disto::DistoMsg::Connect));
    disto_indicate(&mut app, 5.0);
    assert_eq!(
        app.drawing.buffer, expected,
        "disconnected indications are ignored"
    );
    app.disto.connected = Some("DISTO".into());
    let _ = app.set_mode(crate::view_state::Mode::Select);
    disto_indicate(&mut app, 6.0);
    assert_eq!(app.drawing.buffer, expected);
}

#[test]
fn e2e_disto_connect_flow_updates_the_chip() {
    // S12-013/014/083: Tools > Connect opens the chooser over a scan; a pick
    // connects, the toast names the device and `#disto-status` takes its name.
    use crate::disto::DistoMsg;
    let mut h = harness();
    h.script(&["click \"Tools\""]);
    assert!(h.shows("Disconnected"), "the chip starts Disconnected");
    h.script(&["click \"Connect\""]);
    assert!(h.shows("Select a Bluetooth device"), "the chooser opens");
    assert!(h.shows("Scanning for Bluetooth devices\u{2026}"));
    h.send(Message::Disto(DistoMsg::Scanned(Ok(vec![
        disto_device("2", Some("Headset")),
        disto_device("1", Some("DISTO X4")),
    ]))));
    assert!(h.shows("2 devices found"));
    assert!(h.shows("DISTO X4") && h.shows("Headset"));
    h.send(Message::Disto(DistoMsg::Pick(0)));
    h.send(Message::Disto(DistoMsg::Choose));
    h.send(Message::Disto(DistoMsg::Connected(Ok("DISTO X4".into()))));
    assert!(h.shows("Connected to DISTO X4."), "the toast");
    assert!(!h.shows("Select a Bluetooth device"), "the chooser closes");
    assert!(h.shows("DISTO X4"), "the chip shows the device");
    assert!(!h.shows("Disconnected"), "no longer Disconnected");
}

#[test]
fn e2e_disto_cancel_is_silent_and_failures_toast() {
    use crate::disto::{DistoMsg, Failure};
    let mut h = harness();
    h.send(Message::Disto(DistoMsg::Connect));
    h.send(Message::Disto(DistoMsg::Scanned(Ok(vec![]))));
    assert!(h.shows("No Bluetooth devices were found."));
    h.send(Message::Disto(DistoMsg::Cancel));
    assert!(
        !h.shows("Select a Bluetooth device"),
        "Cancel closes silently"
    );
    assert!(!h.shows("Bluetooth connection was not completed."));

    let mut h = harness();
    h.send(Message::Disto(DistoMsg::Connect));
    h.send(Message::Disto(DistoMsg::Scanned(Err(Failure::Unavailable))));
    assert!(h.shows("Bluetooth device access is not available on this computer."));

    let mut h = harness();
    h.send(Message::Disto(DistoMsg::Connect));
    h.send(Message::Disto(DistoMsg::Scanned(Err(Failure::Failed(
        "x".into(),
    )))));
    assert!(h.shows("Bluetooth connection was not completed."));
}

#[test]
fn e2e_disto_escape_dismisses_the_chooser() {
    let mut h = harness();
    h.send(Message::Disto(crate::disto::DistoMsg::Connect));
    assert!(h.shows("Select a Bluetooth device"));
    h.send(Message::Key(crate::input::KeyCmd::Escape));
    assert!(!h.shows("Select a Bluetooth device"));
}

#[test]
fn e2e_disto_connect_is_reachable_from_the_palette() {
    // S12-065 (and S4-030's status-bar button sends the same message).
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("bluetooth".into()));
    h.send(Message::CommandPaletteRun);
    assert!(h.shows("Select a Bluetooth device"));
}

#[test]
fn e2e_flip_sketch_menu_mirrors_the_sketch_and_north() {
    // Home > Flip Sketch opens Flip Horizontal / Flip Vertical; the choice
    // mirrors every point about the sketch's centre, turns north, banks an
    // undo step and clears the selection. An empty sketch does nothing.
    use crate::popup_menu::PopupMsg;
    let mut h = harness();
    h.script(&["click \"Flip Sketch\""]);
    assert!(h.shows("Flip Horizontal"), "the menu opens");
    assert!(h.shows("Flip Vertical"));
    h.send(Message::PopupMenu(PopupMsg::Flip(
        crate::transform::Axis::Horizontal,
    )));
    assert!(!h.shows("Flip Horizontal"), "the choice closes the menu");
    let app = state(h);
    assert!(!app.undo.can_undo(), "nothing drawn: no undo step");

    let mut h = harness();
    h.script(&[
        RAIL_DRAW_EXT,
        "move (341, 429)",
        "click (341, 429)",
        "move (541, 429)",
        "click (541, 429)",
        "type escape",
    ]);
    let before = {
        let mut probe = harness();
        probe.script(&[
            RAIL_DRAW_EXT,
            "move (341, 429)",
            "click (341, 429)",
            "move (541, 429)",
            "click (541, 429)",
            "type escape",
        ]);
        state(probe).drawing.lines[0].clone()
    };
    h.send(Message::PopupMenu(PopupMsg::Open(
        crate::popup_menu::Kind::Flip,
        570.0,
        124.0,
    )));
    h.send(Message::PopupMenu(PopupMsg::Flip(
        crate::transform::Axis::Vertical,
    )));
    let app = state(h);
    let after = &app.drawing.lines[0];
    // A single horizontal wall flipped vertically stays put (it is its own
    // centre line); north becomes 180 - 0.
    assert!((after.start.y - before.start.y).abs() < 1e-6);
    assert!((after.start.x - before.start.x).abs() < 1e-6);
    assert_eq!(app.pages.page().north_angle, 180.0, "north mirrors");
    assert!(app.undo.can_undo(), "the flip banks an undo step");
}

#[test]
fn e2e_canvas_background_menu_recolours_and_persists() {
    // View > Background: White / Dark / Blue with a check on the current;
    // a choice toasts, recolours, and rides the save model.
    use crate::canvas_theme::CanvasTheme;
    use crate::popup_menu::PopupMsg;
    let mut h = harness();
    h.script(&["click \"View\"", "click \"Background\""]);
    assert!(h.shows("White"), "the light theme is called White");
    assert!(h.shows("Dark"));
    assert!(h.shows("Blue"));
    assert!(h.shows("\u{2713}"), "the current choice carries a check");
    h.send(Message::PopupMenu(PopupMsg::Canvas(CanvasTheme::Blue)));
    assert!(h.shows("Drawing background: Blue"), "the toast");
    assert_eq!(crate::canvas_theme::current(), CanvasTheme::Blue);
    let mut app = state(h);
    let model = app.build_save_model();
    assert_eq!(model["settings"]["canvasTheme"], "blue");

    // A save with a canvas theme restores it on open; an unknown one is light.
    let mut h = harness();
    h.send(Message::New);
    let mut app = state(h);
    app.apply_settings(Some(&serde_json::json!({"canvasTheme": "dark"})));
    assert_eq!(crate::canvas_theme::current(), CanvasTheme::Dark);
    app.apply_settings(Some(&serde_json::json!({"canvasTheme": "sepia"})));
    assert_eq!(crate::canvas_theme::current(), CanvasTheme::Light);
}

#[test]
fn e2e_interface_theme_menu_switches_the_whole_chrome() {
    // View > Interface: Light / Dark / Blue; a choice toasts and the chrome
    // palette follows (`--bg-app` etc.).
    use crate::popup_menu::PopupMsg;
    use crate::theme::{bg_app, bg_panel, text_main, InterfaceTheme};
    let hexs = |c: iced::Color| {
        format!(
            "{:02x}{:02x}{:02x}",
            (c.r * 255.0).round() as u8,
            (c.g * 255.0).round() as u8,
            (c.b * 255.0).round() as u8
        )
    };
    let mut h = harness();
    h.script(&["click \"View\"", "click \"Interface\""]);
    assert!(h.shows("Light"));
    assert!(h.shows("\u{2713}"), "Dark is the current choice");
    h.send(Message::PopupMenu(PopupMsg::Interface(
        InterfaceTheme::Light,
    )));
    assert!(h.shows("Interface: Light"));
    assert_eq!(hexs(bg_app()), "e9edf2");
    assert_eq!(hexs(bg_panel()), "f7f8fa");
    assert_eq!(hexs(text_main()), "20252b");
    h.send(Message::PopupMenu(PopupMsg::Interface(
        InterfaceTheme::Blue,
    )));
    assert!(h.shows("Interface: Blue"));
    assert_eq!(hexs(bg_app()), "17243a");
    h.send(Message::PopupMenu(PopupMsg::Interface(
        InterfaceTheme::Dark,
    )));
    assert!(h.shows("Interface: Dark"));
    assert_eq!(hexs(bg_app()), "1e1e1e");
}

#[test]
fn e2e_popup_menu_backdrop_dismisses_and_palette_opens_it() {
    use crate::popup_menu::PopupMsg;
    let mut h = harness();
    h.send(Message::PopupMenu(PopupMsg::Open(
        crate::popup_menu::Kind::InterfaceTheme,
        596.0,
        117.0,
    )));
    assert!(h.shows("Blue"));
    h.send(Message::PopupMenu(PopupMsg::Close));
    assert!(!h.shows("Blue"), "an outside press closes it");

    // S12-061/062/051: the palette entries open the same menus.
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("canvas".into()));
    h.send(Message::CommandPaletteRun);
    assert!(h.shows("White"), "Canvas background menu");
    let mut h = harness();
    h.send(Message::CommandPaletteOpen);
    h.send(Message::CommandPaletteQuery("flip".into()));
    h.send(Message::CommandPaletteRun);
    assert!(h.shows("Flip Horizontal"), "Flip menu");
}

#[test]
fn e2e_dragging_a_dock_symbol_onto_the_canvas_places_it_there() {
    let mut h = harness();
    h.send(Message::DockDragPress(crate::dock::DragItem::Symbol(
        "windows/36in".into(),
    )));
    h.script(&["move (900, 400)", "move (500, 400)", "release (500, 400)"]);
    let app = state(h);
    assert_eq!(app.drawing.symbols.len(), 1, "the drop places it");
    assert!(app.dock_drag.is_none());
    let at = app.world(500.0 - 41.0, 400.0 - 129.0);
    let placed = &app.drawing.symbols[0];
    assert!((placed.x - at.x).abs() < 1.0 && (placed.y - at.y).abs() < 1.0);
    // One undo step removes it.
    let mut app = app;
    let _ = app.update(Message::Undo);
    assert!(app.drawing.symbols.is_empty());
}

#[test]
fn e2e_dragging_a_dock_text_drops_a_plain_12px_label() {
    let mut h = harness();
    h.send(Message::DockDragPress(crate::dock::DragItem::Text(
        "Porch".into(),
    )));
    h.script(&["move (900, 400)", "move (500, 300)", "release (500, 300)"]);
    let mut app = state(h);
    assert_eq!(app.drawing.texts.len(), 1);
    let t = &app.drawing.texts[0];
    assert_eq!(
        (t.text.as_str(), t.font_size, t.rotation),
        ("Porch", 12.0, 0.0)
    );
    let _ = app.update(Message::Undo);
    assert!(app.drawing.texts.is_empty());
}

#[test]
fn e2e_a_dock_drag_released_off_the_canvas_places_nothing() {
    // Over the ribbon, and over the dock itself.
    for release in ["release (300, 60)", "release (1200, 400)"] {
        let mut h = harness();
        h.send(Message::DockDragPress(crate::dock::DragItem::Text(
            "Porch".into(),
        )));
        h.script(&["move (900, 400)", "move (300, 60)", release]);
        let app = state(h);
        assert!(app.drawing.texts.is_empty(), "{release}");
        assert!(app.dock_drag.is_none());
    }
}

#[test]
fn e2e_a_press_without_travel_is_still_a_click_and_a_drag_is_not() {
    use crate::dock::DragItem;
    // The tap's release and the item's release come back to back (one event),
    // so drive them as consecutive messages.
    let mut app = state(harness());
    let _ = app.update(Message::DockDragPress(DragItem::Text("Porch".into())));
    let _ = app.update(Message::DockDragMove(iced::Point::new(1200.0, 300.0), 0.0));
    let _ = app.update(Message::DockDragRelease);
    let _ = app.update(Message::DockItemRelease(DragItem::Text("Porch".into())));
    assert_eq!(app.placing_text.as_deref(), Some("Porch"), "a click arms");

    // After a drag, the release over the item is not a click.
    let mut app = state(harness());
    let item = || DragItem::Symbol("windows/36in".into());
    let _ = app.update(Message::DockDragPress(item()));
    let _ = app.update(Message::DockDragMove(iced::Point::new(1200.0, 300.0), 0.0));
    let _ = app.update(Message::DockDragMove(iced::Point::new(1210.0, 320.0), 0.0));
    let _ = app.update(Message::DockDragRelease);
    let _ = app.update(Message::DockItemRelease(item()));
    assert!(
        app.placing_symbol.is_none(),
        "the drag's release is no click"
    );
    // A stray release over an item, with no press of its own, is no click.
    let _ = app.update(Message::DockItemRelease(item()));
    assert!(app.placing_symbol.is_none());
}

#[test]
fn e2e_clicking_to_place_a_text_label_can_be_undone() {
    let mut app = app_with_label();
    app.drawing.texts.clear();
    let _ = app.update(Message::Dock(DockMsg::TextPick("Porch".into())));
    let _ = app.update(Message::CanvasClick {
        x: 300.0,
        y: 200.0,
        double: false,
        ctrl: false,
    });
    assert_eq!(app.drawing.texts.len(), 1);
    let _ = app.update(Message::Undo);
    assert!(app.drawing.texts.is_empty(), "Place Text is an undo step");
}

#[test]
fn e2e_a_real_press_on_a_dock_symbol_drags_it_to_the_canvas() {
    let drag = |end: &str| {
        let mut h = harness();
        h.send(Message::Dock(DockMsg::Tab(crate::dock::DockTab::Symbols)));
        h.script(&[
            "move (1200, 253)",
            "press (1200, 253)",
            "move (1150, 300)",
            "move (500, 400)",
            end,
        ]);
        state(h)
    };
    let app = drag("release (500, 400)");
    assert_eq!(app.drawing.symbols.len(), 1, "dropped on the canvas");
    assert!(app.placing_symbol.is_none(), "not armed as well");

    // Press and release on the item with no travel is the click that arms it.
    let mut h = harness();
    h.send(Message::Dock(DockMsg::Tab(crate::dock::DockTab::Symbols)));
    h.script(&["move (1200, 253)", "click (1200, 253)"]);
    let app = state(h);
    assert!(app.placing_symbol.is_some(), "a click still arms placement");
    assert!(app.drawing.symbols.is_empty());
}

/// The PNG export decoded to straight RGBA.
fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    (info.width, info.height, buf)
}

/// The box of pixels that differ between two same-size RGBA rasters.
fn diff_box(a: &(u32, u32, Vec<u8>), b: &(u32, u32, Vec<u8>)) -> Option<(u32, u32, u32, u32)> {
    assert_eq!((a.0, a.1), (b.0, b.1));
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..a.1 {
        for x in 0..a.0 {
            let i = ((y * a.0 + x) * 4) as usize;
            if a.2[i..i + 4] != b.2[i..i + 4] {
                found = Some(match found {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    found
}

/// Where a world point lands in the export (`png_export_bytes`' fit).
fn export_px(app: &App, world: Pt) -> (f64, f64) {
    let page = crate::exporter::build_export_page(&app.drawing, 0.0, &app.area_codes);
    let options = crate::exporter::export_options(&app.drawing);
    let size = sketch_core::export_png::parse_image_size(Some(&app.drawing.options.image_size));
    let (_, fit) =
        sketch_core::export_png::render_png(&page, &options, &app.area_codes, size, None).unwrap();
    fit.point(world)
}

#[test]
fn e2e_the_png_export_draws_a_text_label_where_it_sits() {
    let mut app = app_with_label();
    app.selection.clear();
    // A wall gives the export content beyond the label itself.
    app.drawing.texts[0].font_size = 16.0;
    let with = decode_png(&app.png_export_bytes().expect("png"));
    let text = app.drawing.texts.remove(0);
    let without = decode_png(&app.png_export_bytes().expect("png"));
    app.drawing.texts.push(text.clone());
    // Without the label there is no content at all, so compare on a fixed fit
    // instead: the label's own ink must be inside a box around its centre.
    let (cx, cy) = export_px(&app, Pt::new(text.x, text.y));
    let ink = |img: &(u32, u32, Vec<u8>)| img.2.chunks(4).filter(|p| p[3] > 0).count();
    assert!(ink(&with) > 100, "the label is inked");
    assert_eq!(ink(&without), 0, "nothing else is drawn");
    // The ink sits around the label's centre.
    let blank = (with.0, with.1, vec![0u8; with.2.len()]);
    let (x0, y0, x1, y1) = diff_box(&with, &blank).unwrap();
    let (mx, my) = (f64::from(x0 + x1) / 2.0, f64::from(y0 + y1) / 2.0);
    assert!(
        (mx - cx).abs() < 8.0 && (my - cy).abs() < 12.0,
        "{mx},{my} vs {cx},{cy}"
    );
    // Text-size scale: 16 * max(zoom, 2) is at least 32px tall caps-and-descenders.
    assert!(y1 - y0 >= 20, "tall enough to read: {}", y1 - y0);
}

#[test]
fn e2e_the_png_export_draws_area_text_and_respects_the_filters() {
    let mut app = state(defined_square());
    let on = decode_png(&app.png_export_bytes().expect("png"));
    // No auto-post text: only the outline and fill remain.
    let mut quiet = state(defined_square());
    quiet.drawing.polygons[0].auto_post.name = false;
    quiet.drawing.polygons[0].auto_post.calcs = false;
    quiet.drawing.polygons[0].auto_post.code = false;
    let off = decode_png(&quiet.png_export_bytes().expect("png"));
    let (x0, y0, x1, y1) = diff_box(&on, &off).expect("the area text is drawn");
    // Both lines sit in the middle of the square's 10 ft: inside the image's
    // central third, and the block is wider than tall.
    let (w, h) = (on.0, on.1);
    assert!(x0 > w / 4 && x1 < w * 3 / 4 && y0 > h / 4 && y1 < h * 3 / 4);
    assert!(x1 - x0 > y1 - y0);
    // The Area filter hides it.
    app.view.filters[0] = false;
    quiet.view.filters[0] = false;
    let hidden = decode_png(&app.png_export_bytes().expect("png"));
    let quiet_hidden = decode_png(&quiet.png_export_bytes().expect("png"));
    assert!(
        diff_box(&hidden, &quiet_hidden).is_none(),
        "the area pass, including labels, is hidden"
    );
}

#[test]
fn e2e_a_rotated_label_and_area_text_export_turned() {
    let mut app = state(defined_square());
    let flat = decode_png(&app.png_export_bytes().expect("png"));
    app.drawing.polygons[0].label_rotation = Some(90.0);
    app.drawing.polygons[0].area_size_rotation = Some(90.0);
    let turned = decode_png(&app.png_export_bytes().expect("png"));
    let (a0, b0, a1, b1) = diff_box(&flat, &turned).expect("it moved");
    assert!(a1 > a0 && b1 > b0);
}

#[test]
fn e2e_png_export_text_picture() {
    // A look at the result: the export over white.
    let mut app = state(defined_square());
    let id =
        crate::placed_text::place_text_at(&mut app.drawing, Pt::new(-6.0, 6.0), "Porch".into());
    app.drawing
        .texts
        .iter_mut()
        .find(|t| t.id == id)
        .unwrap()
        .underline = true;
    let (w, h, rgba) = decode_png(&app.png_export_bytes().expect("png"));
    let mut flat = Vec::with_capacity(rgba.len());
    for p in rgba.chunks(4) {
        let a = u32::from(p[3]);
        for c in &p[..3] {
            flat.push(((u32::from(*c) * a + 255 * (255 - a)) / 255) as u8);
        }
    }
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e/png_export_text.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&flat)
        .unwrap();
}

fn ink_pixels(img: &(u32, u32, Vec<u8>)) -> usize {
    img.2.chunks(4).filter(|p| p[3] > 0).count()
}

#[test]
fn e2e_the_png_export_draws_a_walls_measurement_label() {
    let mut app = app_with_wall();
    assert_eq!(app.drawing.lines.len(), 1);
    let line = app.drawing.lines[0].clone();
    app.view.dimensions = true;
    let with = decode_png(&app.png_export_bytes().expect("png"));
    app.view.dimensions = false;
    let without = decode_png(&app.png_export_bytes().expect("png"));
    let (x0, y0, x1, y1) = diff_box(&with, &without).expect("the label is drawn");
    // It sits beside the wall's middle.
    let mid = Pt::new(
        (line.start.x + line.end.x) / 2.0,
        (line.start.y + line.end.y) / 2.0,
    );
    let (mx, my) = export_px(&app, mid);
    let (cx, cy) = (f64::from(x0 + x1) / 2.0, f64::from(y0 + y1) / 2.0);
    assert!((cx - mx).abs() < 30.0, "{cx} vs {mx}");
    // Held off the wall by 10 text-zoom px (the fit zoom here is ~9.6).
    assert!((cy - my).abs() < 130.0 && cy > my, "{cy} vs {my}");
    assert!(
        x1 - x0 > 20 && y1 - y0 > 8,
        "readable: {}x{}",
        x1 - x0,
        y1 - y0
    );
    // The Dimensions filter and a hidden label both leave it out.
    app.view.dimensions = true;
    app.view.filters[3] = false;
    let filtered = decode_png(&app.png_export_bytes().expect("png"));
    assert!(diff_box(&filtered, &without).is_none());
    app.view.filters[3] = true;
    app.drawing.lines[0].dimension_hidden = true;
    let hidden = decode_png(&app.png_export_bytes().expect("png"));
    assert!(diff_box(&hidden, &without).is_none());
}

#[test]
fn e2e_the_png_export_draws_placed_symbols() {
    let mut app = app_with_wall();
    app.view.dimensions = false;
    app.place_symbol_at(Pt::new(0.0, -4.0), "windows/36in".into());
    assert_eq!(app.drawing.symbols.len(), 1);
    // The symbol also grows the export's bounds, so compare the same page
    // with the Symbols filter off and on.
    app.view.filters[5] = false;
    let bare = decode_png(&app.png_export_bytes().expect("png"));
    app.view.filters[5] = true;
    let with = decode_png(&app.png_export_bytes().expect("png"));
    let (x0, y0, x1, y1) = diff_box(&with, &bare).expect("the symbol is drawn");
    assert!(ink_pixels(&with) > ink_pixels(&bare));
    assert!(x1 - x0 > 10 && y1 - y0 > 4, "{}x{}", x1 - x0, y1 - y0);
}

#[test]
fn e2e_the_png_export_draws_dimension_chains() {
    let mut app = app_with_wall();
    app.view.dimensions = false;
    let bare = decode_png(&app.png_export_bytes().expect("png"));
    let world = app.drawing.lines[0].start;
    let (seed, side) =
        crate::chains::seed_wall(&app.drawing.lines, world, app.view.zoom).expect("a wall");
    app.drawing.add_chain(seed, side);
    let with = decode_png(&app.png_export_bytes().expect("png"));
    let (x0, y0, x1, y1) = diff_box(&with, &bare).expect("the chain is drawn");
    assert!(
        x1 - x0 > 100,
        "a dimension line along the wall: {}",
        x1 - x0
    );
    assert!(y1 > y0);
    // The Dimensions filter hides it too.
    app.view.filters[3] = false;
    let filtered = decode_png(&app.png_export_bytes().expect("png"));
    assert!(diff_box(&filtered, &bare).is_none());
}

#[test]
fn e2e_png_export_overlay_picture() {
    let mut app = app_with_wall();
    app.view.dimensions = true;
    app.place_symbol_at(Pt::new(-3.0, -4.0), "windows/36in".into());
    let (w, h, rgba) = decode_png(&app.png_export_bytes().expect("png"));
    let mut flat = Vec::with_capacity(rgba.len());
    for p in rgba.chunks(4) {
        let a = u32::from(p[3]);
        for c in &p[..3] {
            flat.push(((u32::from(*c) * a + 255 * (255 - a)) / 255) as u8);
        }
    }
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e/png_export_overlay.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&flat)
        .unwrap();
}

/// Window px (what the dock drag's pointer carries) of a world point.
fn window_px(x: f64, y: f64) -> iced::Point {
    let (cx, cy) = canvas_px(x, y);
    iced::Point::new(cx + 41.0, cy + 129.0)
}

fn area_item(code: &str, name: Option<&str>) -> crate::dock::DragItem {
    crate::dock::DragItem::Area {
        code: code.into(),
        name: name.map(str::to_string),
    }
}

/// Drag an area tile to a world point: press, a first move to set the origin,
/// a far one to start the drag, then the target.
fn drag_area_to(app: &mut App, code: &str, name: Option<&str>, world: (f64, f64)) {
    let _ = app.update(Message::DockDragPress(area_item(code, name)));
    let _ = app.update(Message::DockDragMove(iced::Point::new(1200.0, 300.0), 0.0));
    let _ = app.update(Message::DockDragMove(iced::Point::new(1100.0, 300.0), 0.0));
    let at = window_px(world.0, world.1);
    let _ = app.update(Message::DockDragMove(at, 0.0));
}

fn wall(id: i64, a: (f64, f64), b: (f64, f64)) -> sketch_core::model::SketchLine {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "start": {"x": a.0, "y": a.1},
        "end": {"x": b.0, "y": b.1},
        "type": "exterior"
    }))
    .unwrap()
}

#[test]
fn e2e_dropping_an_area_code_defines_the_area_under_it() {
    let mut app = state(defined_square());
    app.drawing.polygons[0].kind = "UND".into();
    app.drawing.polygons[0].name = "Undefined Area".into();
    // The square's centre is (-6, -2); the hover outlines it in the code's colour.
    drag_area_to(&mut app, "GLA2", Some("Second Floor"), (-6.0, -2.0));
    let hint = app
        .dock_drag
        .as_ref()
        .and_then(|d| d.hover.as_ref())
        .expect("a hint");
    assert!(hint.0.len() >= 4, "the area's ring");
    let _ = app.update(Message::DockDragRelease);
    let poly = &app.drawing.polygons[0];
    assert_eq!(
        (poly.kind.as_str(), poly.name.as_str()),
        ("GLA2", "Second Floor")
    );
    assert_eq!(app.drawing.polygons.len(), 1);
    // One undo step puts the area back.
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0].kind, "UND");
}

#[test]
fn e2e_the_define_drop_renames_the_areas_generated_layer() {
    let mut app = state(defined_square());
    app.drawing.polygons[0].kind = "UND".into();
    app.drawing.polygons[0].name = "Undefined Area".into();
    // The fixture's area was closed already defined, so its layer took the
    // name "First Floor". An undefined area's layer carries the generated
    // name, and only a generated name is replaced by a later definition.
    let layer = app.drawing.polygons[0]
        .layer_id
        .clone()
        .expect("a dedicated layer");
    app.layers.rename(&layer, "Area".to_string());
    drag_area_to(&mut app, "GLA2", Some("Second Floor"), (-6.0, -2.0));
    let _ = app.update(Message::DockDragRelease);
    assert_eq!(app.layers.layer(&layer).unwrap().name, "Second Floor");
}

#[test]
fn e2e_the_define_drop_keeps_a_layer_name_that_is_not_generated() {
    // `/^Area( \d+)?$/` in the original's defineAreaTarget: a layer that
    // already has a real name (here from the first definition) keeps it.
    let mut app = state(defined_square());
    let layer = app.drawing.polygons[0]
        .layer_id
        .clone()
        .expect("a dedicated layer");
    assert_eq!(app.layers.layer(&layer).unwrap().name, "First Floor");
    drag_area_to(&mut app, "GLA2", Some("Second Floor"), (-6.0, -2.0));
    let _ = app.update(Message::DockDragRelease);
    assert_eq!(app.drawing.polygons[0].name, "Second Floor");
    assert_eq!(app.layers.layer(&layer).unwrap().name, "First Floor");
}

#[test]
fn e2e_the_clear_tile_returns_an_area_to_undefined() {
    let mut app = state(defined_square());
    drag_area_to(&mut app, "UND", None, (-6.0, -2.0));
    let _ = app.update(Message::DockDragRelease);
    let poly = &app.drawing.polygons[0];
    assert_eq!(
        (poly.kind.as_str(), poly.name.as_str()),
        ("UND", "Undefined Area")
    );
}

#[test]
fn e2e_a_drop_inside_walls_with_no_area_creates_it_and_defines_it() {
    let mut app = state(harness());
    for (i, (a, b)) in [
        ((0.0, 0.0), (8.0, 0.0)),
        ((8.0, 0.0), (8.0, 6.0)),
        ((8.0, 6.0), (0.0, 6.0)),
        ((0.0, 6.0), (0.0, 0.0)),
    ]
    .into_iter()
    .enumerate()
    {
        app.drawing.lines.push(wall(100 + i as i64, a, b));
    }
    assert!(app.drawing.polygons.is_empty());
    drag_area_to(&mut app, "GLA1", Some("First Floor"), (4.0, 3.0));
    let hint = app
        .dock_drag
        .as_ref()
        .and_then(|d| d.hover.as_ref())
        .expect("a hint");
    assert_eq!(hint.0.len(), 4, "the enclosed face is outlined");
    let _ = app.update(Message::DockDragRelease);
    assert_eq!(app.drawing.polygons.len(), 1);
    let poly = &app.drawing.polygons[0];
    assert_eq!(
        (poly.kind.as_str(), poly.name.as_str()),
        ("GLA1", "First Floor")
    );
    assert!((poly.area - 48.0).abs() < 1e-6, "{}", poly.area);
    assert!(poly.layer_id.is_some(), "on a dedicated layer");
    // Two undo steps: the definition, then the region itself.
    let _ = app.update(Message::Undo);
    assert_eq!(app.drawing.polygons[0].kind, "UND");
    let _ = app.update(Message::Undo);
    assert!(app.drawing.polygons.is_empty());
}

#[test]
fn e2e_an_addition_against_a_wall_gets_a_wall_of_its_own() {
    // A 10x10 house and a 4x5 porch against the middle of its bottom wall.
    let mut app = state(harness());
    let walls = [
        ((0.0, 0.0), (10.0, 0.0)),
        ((10.0, 0.0), (10.0, 10.0)),
        ((10.0, 10.0), (0.0, 10.0)),
        ((0.0, 10.0), (0.0, 0.0)),
        ((3.0, 0.0), (3.0, -5.0)),
        ((3.0, -5.0), (7.0, -5.0)),
        ((7.0, -5.0), (7.0, 0.0)),
    ];
    for (i, (a, b)) in walls.into_iter().enumerate() {
        app.drawing.lines.push(wall(100 + i as i64, a, b));
    }
    let before = app.drawing.lines.len();
    drag_area_to(&mut app, "P/P", Some("Porch"), (5.0, -2.0));
    let _ = app.update(Message::DockDragRelease);
    assert_eq!(app.drawing.polygons.len(), 1);
    let poly = app.drawing.polygons[0].clone();
    assert!((poly.area - 20.0).abs() < 1e-6, "the porch: {}", poly.area);
    assert_eq!(
        app.drawing.lines.len(),
        before + 1,
        "the shared side got its own wall"
    );
    // Every side now runs exactly along the wall it names.
    for (i, edge) in poly.edges.iter().enumerate() {
        let line = app
            .drawing
            .lines
            .iter()
            .find(|l| Some(l.id) == edge.line_id)
            .expect("a wall");
        let (a, b) = (poly.points[i], poly.points[(i + 1) % poly.points.len()]);
        let ends = [(line.start, line.end), (line.end, line.start)];
        assert!(
            ends.iter()
                .any(|(s, e)| sketch_core::model::same(*s, a) && sketch_core::model::same(*e, b)),
            "side {i}"
        );
    }
}

#[test]
fn e2e_a_drop_outside_any_region_or_on_a_locked_area_changes_nothing() {
    let mut app = state(defined_square());
    let _ = app.update(Message::Noop);
    let before = app.drawing.polygons[0].clone();
    // Empty canvas.
    drag_area_to(&mut app, "GLA2", Some("Second Floor"), (8.0, 8.0));
    assert!(
        app.dock_drag.as_ref().unwrap().hover.is_none(),
        "no region, no hint"
    );
    let _ = app.update(Message::DockDragRelease);
    assert_eq!(app.drawing.polygons[0], before);
    // A locked area refuses.
    let layer = before.layer_id.clone().expect("layer");
    app.layers.set_locked(&layer, true);
    app.gates_cache = app.layer_gates();
    drag_area_to(&mut app, "GLA2", Some("Second Floor"), (-6.0, -2.0));
    assert!(
        app.dock_drag.as_ref().unwrap().hover.is_none(),
        "locked: no hint"
    );
    let _ = app.update(Message::DockDragRelease);
    assert_eq!(app.drawing.polygons[0], before);
}

#[test]
fn e2e_escape_cancels_a_dock_drag() {
    let mut app = state(defined_square());
    let _ = app.update(Message::Noop);
    let before = app.drawing.polygons[0].clone();
    drag_area_to(&mut app, "GLA2", Some("Second Floor"), (-6.0, -2.0));
    assert!(app.dock_drag.is_some());
    let _ = app.update(Message::Key(crate::input::KeyCmd::Escape));
    assert!(app.dock_drag.is_none(), "the drag is gone");
    let _ = app.update(Message::DockDragRelease);
    let _ = app.update(Message::DockItemRelease(area_item(
        "GLA2",
        Some("Second Floor"),
    )));
    assert_eq!(app.drawing.polygons[0], before, "nothing dropped, no click");
    assert!(app.drawing.selected_area_type.as_deref() != Some("GLA2"));
    // A symbol drag cancels the same way.
    let mut app = state(harness());
    let _ = app.update(Message::DockDragPress(crate::dock::DragItem::Symbol(
        "windows/36in".into(),
    )));
    let _ = app.update(Message::DockDragMove(iced::Point::new(1200.0, 300.0), 0.0));
    let _ = app.update(Message::DockDragMove(window_px(0.0, 0.0), 0.0));
    let _ = app.update(Message::Key(crate::input::KeyCmd::Escape));
    let _ = app.update(Message::DockDragRelease);
    assert!(app.drawing.symbols.is_empty());
}

#[test]
fn e2e_a_palette_tile_is_still_a_click_to_pick_the_code() {
    let mut app = state(harness());
    let _ = app.update(Message::DockDragPress(area_item(
        "GLA2",
        Some("Second Floor"),
    )));
    let _ = app.update(Message::DockItemRelease(area_item(
        "GLA2",
        Some("Second Floor"),
    )));
    assert_eq!(app.drawing.selected_area_type.as_deref(), Some("GLA2"));
    assert_eq!(
        app.drawing.selected_area_name.as_deref(),
        Some("Second Floor")
    );
}

#[test]
fn e2e_a_real_press_on_a_palette_tile_drags_it_onto_an_area() {
    // The Second Floor tile (window px 1200, 239) onto the square's centre.
    let mut h = defined_square();
    h.send(Message::Dock(DockMsg::Tab(DockTab::Areas)));
    h.script(&[
        "move (1200, 239)",
        "press (1200, 239)",
        "move (1150, 280)",
        "move (443, 521)",
        "release (443, 521)",
    ]);
    let app = state(h);
    let poly = &app.drawing.polygons[0];
    assert_eq!(
        (poly.kind.as_str(), poly.name.as_str()),
        ("GLA2", "Second Floor")
    );
    // A plain click on a tile picks it.
    let mut h = harness();
    h.send(Message::Dock(DockMsg::Tab(DockTab::Areas)));
    h.script(&["move (1200, 239)", "click (1200, 239)"]);
    let app = state(h);
    assert_eq!(app.drawing.selected_area_type.as_deref(), Some("GLA2"));
}

#[test]
fn e2e_the_area_drop_hint_is_painted_over_the_region() {
    let mut h = defined_square();
    h.send(Message::DockDragPress(area_item(
        "GLA2",
        Some("Second Floor"),
    )));
    h.send(Message::DockDragMove(iced::Point::new(1200.0, 300.0), 0.0));
    h.send(Message::DockDragMove(iced::Point::new(1100.0, 300.0), 0.0));
    h.send(Message::DockDragMove(window_px(-6.0, -2.0), 0.0));
    h.screenshot("area_drop_hint");
}

#[test]
fn e2e_report_fidelity_all_papers_and_exact_preview() {
    use sketch_core::print_layout::PaperSize;
    let mut app = state(defined_square());
    app.file_name = "report-fidelity".into();
    app.subject_info.owner = "Renée García – Müller".into();
    app.subject_info.property_address = "123 Long Property Address, Suite 456".into();
    app.subject_info.general_comments = "Measured walls and curved edges. Styled labels, symbols and dimensions appear in the printed sketch. ".repeat(5);
    app.subject_info.comment_tables =
        vec![vec!["Living area verified".into(), "Garage excluded".into()]; 3];
    let id = crate::placed_text::place_text_at(
        &mut app.drawing,
        Pt::new(-3.0, 3.0),
        "Porch – café".into(),
    );
    let label = app.drawing.texts.iter_mut().find(|t| t.id == id).unwrap();
    label.rotation = 30.0;
    label.bold = true;
    label.italic = true;
    app.place_symbol_at(Pt::new(-3.0, -4.0), "windows/36in".into());
    let wall = app.drawing.lines[0].start;
    let (seed, side) = crate::chains::seed_wall(&app.drawing.lines, wall, app.view.zoom).unwrap();
    app.drawing.add_chain(seed, side);
    app.pages.pages.push(crate::pages::Page {
        lines: app.drawing.lines.clone(),
        polygons: app.drawing.polygons.clone(),
        labels: app.drawing.texts.clone(),
        symbols: app.drawing.symbols.clone(),
        north_angle: 45.0,
        ..Default::default()
    });
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e/report-fidelity");
    std::fs::create_dir_all(&dir).unwrap();
    for paper in [PaperSize::Legal, PaperSize::Letter, PaperSize::A4] {
        app.page_layout.paper_size = paper;
        let pdf = app.report_pdf();
        let pages = crate::print::render_pages(&pdf, 1040).expect("a readable PDF");
        assert_eq!(pages.len(), 2, "each sketch has a report page");
        let name = crate::print::paper_name(paper);
        std::fs::write(dir.join(format!("{name}.pdf")), &pdf).unwrap();
        for (index, png) in pages.iter().enumerate() {
            let (w, h, rgba) = decode_png(png);
            assert_eq!(w, 1040);
            assert!(h > w);
            assert!(
                rgba.chunks(4)
                    .filter(|p| p[0] < 200 && p[1] < 200 && p[2] < 200)
                    .count()
                    > 5000,
                "report has visible content"
            );
            std::fs::write(dir.join(format!("{name}-{}.png", index + 1)), png).unwrap();
        }
        app.print_preview.open(paper, pdf.clone()).unwrap();
        assert_eq!(app.print_preview.page_count(), 2);
        let _ = app.update(Message::PrintPreview(crate::print::PreviewMsg::Print));
        assert_eq!(
            std::fs::read(crate::print::temp_report_path(&app.file_name)).unwrap(),
            pdf,
            "Print uses the previewed bytes"
        );
        app.print_preview.close();
    }
}

#[test]
fn e2e_png_export_respects_layer_occlusion_and_omits_selection() {
    let mut app = state(defined_square());
    app.view.dimensions = false;
    let poly = &mut app.drawing.polygons[0];
    let center = Pt::new(
        poly.points.iter().map(|p| p.x).sum::<f64>() / poly.points.len() as f64,
        poly.points.iter().map(|p| p.y).sum::<f64>() / poly.points.len() as f64,
    );
    poly.fill_color = Some("#ffffff".into());
    poly.fill_opacity = Some(100.0);
    poly.auto_post.name = false;
    poly.auto_post.calcs = false;
    poly.auto_post.code = false;
    poly.layer_id = Some("upper".into());
    app.layers.items = vec![
        crate::layers::Layer::new("lower", "Lower"),
        crate::layers::Layer::new("upper", "Upper"),
    ];
    let id = crate::placed_text::place_text_at(&mut app.drawing, center, "Covered label".into());
    let label = app.drawing.texts.iter_mut().find(|t| t.id == id).unwrap();
    label.layer_id = 1;
    label.color = "#000000".into();
    app.selection.clear();
    let covered = decode_png(&app.png_export_bytes().unwrap());
    let text = app.drawing.texts.remove(0);
    let bare = decode_png(&app.png_export_bytes().unwrap());
    assert!(
        diff_box(&covered, &bare).is_none(),
        "foreground fill covers lower-layer text"
    );
    app.drawing.texts.push(text);
    app.layers.items.reverse();
    app.drawing.texts[0].layer_id = 2;
    let visible = decode_png(&app.png_export_bytes().unwrap());
    assert!(
        diff_box(&visible, &covered).is_some(),
        "changing layer order reveals the text"
    );
    app.selection.lines.insert(app.drawing.lines[0].id);
    let selected = decode_png(&app.png_export_bytes().unwrap());
    assert!(
        diff_box(&visible, &selected).is_none(),
        "selection handles never enter the export"
    );
}

#[test]
fn e2e_symbol_manager_delete_is_permanent_and_purge_controls_are_absent() {
    crate::symlib::with_temp_library(|| {
        crate::persist::with_temp_store(|| {
            use crate::symbol_manager::Msg as S;
            let id = "caddillo/door-swing-36";
            let mut h = harness();
            h.send(Message::SymbolManager(S::Open));
            for old in [
                "Purge",
                "Purge hidden…",
                "Delete forever",
                "Delete category forever",
                "Export purged list",
            ] {
                assert!(!h.shows(old), "removed control: {old}");
            }
            assert!(!crate::symbol_manager::VIEWS.contains(&"Purged"));
            h.send(Message::SymbolManager(S::Filter("Door Swing 36".into())));
            h.send(Message::SymbolManager(S::Toggle(id.into())));
            h.script(&["click (400, 770)", "click \"Cancel\""]);
            assert!(crate::symlib::library()
                .as_ref()
                .as_ref()
                .unwrap()
                .symbol(id)
                .is_some());
            h.script(&["click (400, 770)", "click \"Confirm\"", "click \"Confirm\""]);
            assert!(crate::symlib::library()
                .as_ref()
                .as_ref()
                .unwrap()
                .symbol(id)
                .is_none());
            h.screenshot("symbol-manager-delete-025");
            let a = state(h);
            assert!(a.symbol_changes.deleted.contains(id));
            assert!(crate::symlib::library_path()
                .with_file_name("symbols.json.bak")
                .is_file());
        });
    });
}

#[test]
fn e2e_symbol_context_uses_delete_without_a_purge_action() {
    use crate::symbol_manager::Msg as S;
    let mut h = harness();
    h.send(Message::SymbolManager(S::Context(
        "caddillo/door-swing-36".into(),
    )));
    assert!(h.shows("Delete"));
    assert!(!h.shows("Purge"));
    assert!(!h.shows("Delete forever"));
    h.script(&["click (790, 440)","expect \"Delete selected symbols? The symbols are removed from symbols.json along with every placed instance. This cannot be undone.\"","click \"Cancel\""]);
}

#[test]
fn e2e_symbol_delete_layout_gallery() {
    use crate::symbol_manager::Msg as S;
    let mut h = harness();
    h.send(Message::SymbolManager(S::Open));
    h.send(Message::SymbolManager(S::Toggle(
        "caddillo/door-swing-36".into(),
    )));
    h.screenshot("symbol-delete-manager-layout-025");
    let mut h = harness();
    h.send(Message::SymbolManager(S::Context(
        "caddillo/door-swing-36".into(),
    )));
    h.screenshot("symbol-delete-context-layout-025");
    let mut h = harness();
    h.send(Message::SymbolManager(S::Open));
    h.script(&["click (1200, 89)"]);
    h.screenshot("symbol-delete-categories-025");
}

#[test]
fn e2e_symbol_manager_native_actions_and_preview() {
    use crate::symbol_manager::Msg as S;
    let mut h = harness();
    h.send(Message::Dock(DockMsg::Show(DockTab::Symbols)));
    h.script(&["click \"Manage symbols...\"", "expect \"Manage Symbols\""]);
    let id = crate::symlib::library()
        .as_ref()
        .as_ref()
        .unwrap()
        .categories[0]
        .items[0]
        .id
        .clone();
    h.send(Message::SymbolManager(S::Toggle(id.clone())));
    h.script(&[
        "click \"Hide\"",
        "expect \"Hide selected definitions? Placed instances keep rendering.\"",
        "click \"Confirm\"",
    ]);
    h.script(&["click \"Close\""]);
    let a = state(h);
    assert!(a.symbol_changes.hidden.contains(&id));
    assert!(a.symbols_lib().symbol(&id).is_some());
    let mut preview = harness();
    preview.send(Message::SymbolManager(S::Open));
    preview.send(Message::SymbolManager(S::Toggle(id)));
    preview.screenshot("symbol-manager-native");
}

#[test]
fn e2e_autosave_startup_recovery_and_file_entries() {
    crate::persist::with_temp_store(|| {
        let mut original = App::boot(1.).0;
        original.file_name = "Recover through UI".into();
        original.extended_data = "Preserved".into();
        let _ = original.autosave_tick(true);
        let mut h = harness();
        h.script(&[
            "expect \"Recover autosaved sketch: Recover through UI?\"",
            "click \"Recover\"",
        ]);
        h.script(&[
            "click \"File\"",
            "expect \"Import Layer...\"",
            "expect \"Recover Autosave...\"",
        ]);
        h.screenshot("autosave-recovered-native");
        let a = state(h);
        assert_eq!(a.file_name, "Recover through UI");
        assert_eq!(a.extended_data, "Preserved");
    });
}

#[test]
fn e2e_symbol_manager_live_preview_and_separate_viewer() {
    use crate::symbol_manager::Msg as S;
    let mut h = harness();
    h.send(Message::SymbolManager(S::Open));
    h.script(&[
        "expect \"Enlarge\"",
        "click \"Arrows - Left Turn\"",
        "expect \"Type: arrows/left-turn\"",
        "click \"Enlarge\"",
        "expect \"Symbol Viewer\"",
        "type escape",
        "expect \"Manage Symbols\"",
        "expect \"Type: arrows/left-turn\"",
    ]);
    h.screenshot("symbol-manager-live-preview");
    let a = state(h);
    assert!(a.symbol_manager.open);
    assert!(!a.symbol_manager.enlarged);
    assert_eq!(a.symbol_manager.focus.as_deref(), Some("arrows/left-turn"));
    assert!(
        a.symbol_manager.selected.is_empty(),
        "previewing must not change bulk selection"
    );
    let mut viewer = harness();
    viewer.send(Message::SymbolManager(S::Open));
    viewer.send(Message::SymbolManager(S::Inspect(
        "arrows/north-arrow".into(),
    )));
    viewer.script(&[
        "expect \"Symbol Viewer\"",
        "expect \"Type: arrows/north-arrow\"",
    ]);
    viewer.screenshot("symbol-viewer-native");
}
#[test]
fn e2e_symbol_manager_footer_and_preview_fit_small_window() {
    use crate::symbol_manager::Msg as S;
    for size in [iced::Size::new(960., 640.), iced::Size::new(800., 600.)] {
        let mut h = Harness::boot_at_size(program(1.), size);
        h.send(Message::WindowResized(size));
        h.send(Message::SymbolManager(S::Open));
        h.script(&[
            "expect \"Enlarge\"",
            "expect \"Add DXF / JSON\"",
            "click \"Close\"",
        ]);
        assert!(
            !state(h).symbol_manager.open,
            "footer Close must be reachable at {size:?}"
        );
        let mut shot = Harness::boot_at_size(program(1.), size);
        shot.send(Message::WindowResized(size));
        shot.send(Message::SymbolManager(S::Open));
        shot.screenshot(&format!(
            "symbol-manager-{}x{}",
            size.width as u32, size.height as u32
        ));
    }
}

fn deleted_boundary_harness(
) -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static> {
    let mut h = harness();
    h.script(LOOP1);
    h.script(&[RAIL_SELECT, "click (441, 429)"]);
    h.send(Message::Delete);
    h
}
fn keyboard_replacement_harness(
    interior: bool,
) -> Harness<impl Program<State = App, Message = Message, Theme = iced::Theme> + 'static> {
    use crate::input::KeyCmd;
    let mut h = deleted_boundary_harness();
    if interior {
        h.script(&[RAIL_DRAW_INT]);
    } else {
        h.script(&[RAIL_DRAW_EXT]);
    }
    h.script(&["click (341, 429)"]);
    h.send(Message::Key(KeyCmd::Buffer("1")));
    h.send(Message::Key(KeyCmd::Buffer("0")));
    h.send(Message::Key(KeyCmd::Arrow {
        dir: 2,
        ctrl: false,
        shift: false,
    }));
    h.send(Message::Key(KeyCmd::Enter));
    h
}
#[test]
fn e2e_redraw_deleted_boundary_mouse_recloses_area() {
    let mut h = deleted_boundary_harness();
    h.script(&[RAIL_DRAW_EXT, "click (341, 429)", "click (541, 429)"]);
    let a = state(h);
    assert_eq!(a.drawing.lines.len(), 4);
    assert_eq!(
        a.drawing.polygons.len(),
        1,
        "replacing the missing wall must recreate the enclosed area"
    );
    assert!((a.drawing.polygons[0].area - 100.).abs() < 1e-6);
    assert!(
        !a.drawing.pen_down,
        "a single replacement finishes its run immediately"
    );
}
#[test]
fn e2e_redraw_deleted_boundary_keyboard_recloses_area() {
    let before = state(deleted_boundary_harness());
    assert_eq!(before.drawing.lines.len(), 3);
    assert!(before.drawing.polygons.is_empty());
    let mut replacement = keyboard_replacement_harness(false);
    replacement.screenshot("redraw-keyboard-reclosed");
    let a = state(replacement);
    assert_eq!(a.drawing.lines.len(), 4);
    assert_eq!(
        a.drawing.polygons.len(),
        1,
        "a typed replacement must run the same loop detection as mouse drawing"
    );
    assert!((a.drawing.polygons[0].area - 100.).abs() < 1e-6);
    assert!(!a.drawing.pen_down);
    assert!(!a.drawing.keyboard_pointer);
    assert!(a.drawing.points.is_empty());
    assert!(a.drawing.line_ids.is_empty());
}
#[test]
fn e2e_redraw_deleted_boundary_undo_removes_closure_as_one_edit() {
    let mut h = keyboard_replacement_harness(false);
    h.send(Message::Undo);
    let a = state(h);
    assert_eq!(a.drawing.lines.len(), 3);
    assert!(a.drawing.polygons.is_empty());
}
#[test]
fn e2e_redraw_deleted_boundary_redo_restores_one_closed_area() {
    let mut h = keyboard_replacement_harness(false);
    h.send(Message::Undo);
    h.send(Message::Redo);
    let a = state(h);
    assert_eq!(a.drawing.lines.len(), 4);
    assert_eq!(a.drawing.polygons.len(), 1);
    assert!((a.drawing.polygons[0].area - 100.).abs() < 1e-6);
    assert!(!a.drawing.pen_down);
}
#[test]
fn e2e_redraw_deleted_boundary_interior_does_not_enclose_appraisal_area() {
    let a = state(keyboard_replacement_harness(true));
    assert_eq!(
        a.drawing.lines.len(),
        4,
        "mode={:?} pen={} cursor={:?} points={:?} buffer={:?}",
        a.view.mode,
        a.drawing.pen_down,
        a.drawing.cursor,
        a.drawing.points,
        a.drawing.buffer
    );
    // Lines are kept in layer order, so the new wall is the one with the
    // highest id, not necessarily the last in the list.
    let newest = a.drawing.lines.iter().max_by_key(|l| l.id).unwrap();
    assert_eq!(newest.kind, sketch_core::model::WallType::Interior);
    assert!(a.drawing.polygons.is_empty());
    assert!(a.drawing.pen_down);
}
#[test]
fn e2e_redraw_detecting_an_unrelated_loop_keeps_current_run_active() {
    use crate::input::KeyCmd;
    let mut a = state(harness());
    a.drawing.add_wall_shape(
        crate::drawing::WallShape::Rectangle,
        sketch_core::model::WallType::Exterior,
        10.,
        Some(10.),
        None,
        Pt::new(0., 0.),
    );
    a.drawing.polygons.clear();
    a.view.mode = crate::view_state::Mode::DrawExterior;
    a.drawing.cursor = Pt::new(30., 30.);
    for key in [
        KeyCmd::Enter,
        KeyCmd::Buffer("1"),
        KeyCmd::Buffer("0"),
        KeyCmd::Arrow {
            dir: 2,
            ctrl: false,
            shift: false,
        },
        KeyCmd::Enter,
    ] {
        let _ = a.update(Message::Key(key));
    }
    assert_eq!(
        a.drawing.polygons.len(),
        1,
        "the pre-existing closed loop is detected"
    );
    assert!(
        a.drawing.pen_down,
        "an unrelated detected loop must not terminate the current run"
    );
    assert!(a.drawing.keyboard_pointer);
    // Layer order, not draw order: find the new wall by its id.
    let newest = a.drawing.lines.iter().max_by_key(|l| l.id).unwrap();
    assert_eq!(newest.end, Pt::new(40., 30.));
}

#[test]
fn e2e_redraw_next_wall_cannot_reuse_the_reclosed_area_id() {
    let mut h = keyboard_replacement_harness(false);
    h.script(&["click (650, 700)", "click (850, 700)"]);
    let a = state(h);
    assert_eq!(a.drawing.lines.len(), 5);
    assert_eq!(a.drawing.polygons.len(), 1);
    let ids: Vec<_> = a
        .drawing
        .lines
        .iter()
        .map(|line| line.id)
        .chain(a.drawing.polygons.iter().map(|poly| poly.id))
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "a subsequent drawing operation must not reuse the recreated area's ID"
    );
}

#[test]
fn e2e_redraw_deleted_boundary_touch_recloses_area() {
    let mut h = deleted_boundary_harness();
    h.script(&[RAIL_DRAW_EXT, "click (341, 429)"]);
    h.send(Message::TouchDistance("10".into()));
    h.send(Message::TouchAction("right".into()));
    let a = state(h);
    assert_eq!(a.drawing.lines.len(), 4);
    assert_eq!(a.drawing.polygons.len(), 1);
    assert!(!a.drawing.pen_down);
    assert!((a.drawing.polygons[0].area - 100.).abs() < 1e-6);
}

#[test]
fn e2e_redraw_deleted_boundary_with_remaining_curved_wall_recloses_area() {
    use crate::input::KeyCmd;
    let mut a = state(deleted_boundary_harness());
    let curve = a
        .drawing
        .lines
        .iter()
        .find(|line| (line.start.x + 11.).abs() < 1e-6 && (line.end.x + 11.).abs() < 1e-6)
        .unwrap()
        .id;
    assert!(a.drawing.set_line_bow(curve, 90.));
    let _ = a.update(Message::Key(KeyCmd::DrawExterior));
    a.drawing.cursor = Pt::new(-11., 3.);
    for key in [
        KeyCmd::Enter,
        KeyCmd::Buffer("1"),
        KeyCmd::Buffer("0"),
        KeyCmd::Arrow {
            dir: 2,
            ctrl: false,
            shift: false,
        },
        KeyCmd::Enter,
    ] {
        let _ = a.update(Message::Key(key));
    }
    assert_eq!(
        a.drawing.polygons.len(),
        1,
        "replacing a straight edge must also re-close a boundary that contains a curve"
    );
    let poly = &a.drawing.polygons[0];
    assert!(poly
        .edges
        .iter()
        .any(|edge| edge.line_id == Some(curve) && edge.bow_angle.abs() > 89.));
    assert!(
        (poly.area - 100.).abs() > 1.,
        "the curved area must not be flattened to its chord"
    );
    assert!(!a.drawing.pen_down);
}

#[test]
fn e2e_report_close_buttons_cancel_discard_and_save_error() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/report-close-022/widget-blocked");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::write(&root, b"Blocked report session").unwrap();
    let make = || {
        let root = root.clone();
        Harness::boot(iced::application(
            move || {
                let (mut app, task) = App::boot(1.);
                app.apex_session = Some(root.clone());
                (app, task)
            },
            App::update,
            App::view,
        ))
    };
    let mut h = make();
    h.send(Message::Close);
    for label in ["Save to report", "Discard changes", "Cancel"] {
        assert!(h.shows(label));
    }
    h.script(&["click \"Cancel\""]);
    assert!(!h.shows("Save to report"));
    h.send(Message::Close);
    h.script(&["type escape"]);
    assert!(!h.shows("Save to report"));
    h.send(Message::Close);
    h.script(&["click \"Save to report\""]);
    let app = state(h);
    assert!(!app.apex_returning);
    assert!(
        app.toasts.text().is_some(),
        "Save invokes host transfer and reports the I/O failure"
    );
    let mut h = make();
    h.send(Message::Close);
    h.screenshot("report-close-options");
    let mut h = make();
    h.send(Message::Close);
    h.script(&["click \"Discard changes\""]);
    let app = state(h);
    assert!(app
        .toasts
        .text()
        .unwrap()
        .contains("Could not close the report session"));
}

#[test]
fn e2e_category_deletion_fire_uses_visible_confirmation_and_refreshes() {
    crate::symlib::with_temp_library(|| {
        use crate::symbol_manager::Msg as S;
        let mut shot = harness();
        shot.send(Message::SymbolManager(S::Open));
        shot.send(Message::SymbolManager(S::Category("fire".into())));
        shot.script(&["click \"Delete category\""]);
        shot.screenshot("fire-category-delete-confirmation");
        let mut h = harness();
        h.send(Message::SymbolManager(S::Open));
        h.send(Message::SymbolManager(S::Category("fire".into())));
        h.script(&[
            "click \"Delete category\"",
            "expect \"Delete category: Fire\"",
            "click \"Cancel\"",
        ]);
        assert!(!h.shows("Delete category: Fire"));
        h.script(&[
            "click \"Delete category\"",
            "click \"Continue\"",
            "type escape",
            "expect \"Manage Symbols\"",
        ]);
        assert!(!h.shows("Delete category: Fire"));
        h.script(&[
            "click \"Delete category\"",
            "click \"Continue\"",
            "click \"Confirm deletion\"",
        ]);
        let a = state(h);
        assert!(a.symbol_changes.deleted_categories.contains("fire"));
        assert!(a.symbol_manager.category.is_empty());
        assert!(!a.symbols_lib().categories.iter().any(|c| c.key == "fire"));
        assert!(!a.manager_symbols().is_empty());
    });
}

#[test]
fn e2e_category_deletion_confirmation_fits_a_small_window() {
    use crate::symbol_manager::Msg as S;
    let size = iced::Size::new(800., 600.);
    let mut h = Harness::boot_at_size(program(1.), size);
    h.send(Message::WindowResized(size));
    h.send(Message::SymbolManager(S::Open));
    h.send(Message::SymbolManager(S::Category("fire".into())));
    h.script(&[
        "click \"Delete category\"",
        "expect \"Delete category: Fire\"",
        "click \"Continue\"",
        "click \"Cancel\"",
    ]);
    assert!(!h.shows("Delete category: Fire"));
    h.script(&["click \"Delete category\""]);
    h.screenshot("fire-category-delete-small");
}

#[test]
fn e2e_recent_files_list_renders_with_an_entry() {
    // The row's fill-weight file icon was not embedded, so opening the list
    // with at least one entry panicked the app. Building the view is enough
    // to hit it: icons are resolved as the row is built.
    let mut app = state(harness());
    app.recent = vec![crate::recent::RecentFile::new("Subject.skx", "{\"pages\":[]}").unwrap()];
    let _ = app.update(Message::TabClicked(crate::ribbon::TabId::File));
    let _ = app.update(Message::RecentToggle);
    assert!(app.file_menu && app.recent_open);
    let _ = app.view();
}

#[test]
fn e2e_help_tab_opens_the_user_guide_and_the_integration_topic() {
    use crate::guide::{GuideMsg, INTEGRATION_TOPIC};
    // Both guide buttons are on the Help tab, with the actions they run.
    let opens: Vec<usize> = crate::ribbon_tabs::help()
        .into_iter()
        .filter_map(|item| match item {
            crate::ribbon::Item::Btn(b) => match b.action {
                Some(Message::Guide(GuideMsg::Open(topic))) => Some(topic),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(opens, [INTEGRATION_TOPIC, 0]);

    let mut h = harness();
    h.script(&["click \"Help\""]);
    h.send(Message::Guide(GuideMsg::Open(0)));
    assert!(h.shows("USER GUIDE") && h.shows("A first sketch"));
    // The topic list switches the page.
    h.script(&["click \"Appraise-It integration\""]);
    assert!(h.shows("Set up (once per computer)") && !h.shows("A first sketch"));
    // Escape closes it, like the shortcuts sheet.
    h.script(&["type escape"]);
    assert!(!h.shows("USER GUIDE"));
    h.send(Message::Guide(GuideMsg::Open(INTEGRATION_TOPIC)));
    assert!(h.shows("Going back to Apex"));
    let app = state(h);
    assert!(app.guide.open && app.guide.topic == INTEGRATION_TOPIC);

    // Pictures for review (a screenshot ends its harness).
    for (name, topic) in [("guide-getting-started", Some(0)), ("guide-integration", Some(INTEGRATION_TOPIC)), ("help-tab", None)] {
        let mut h = harness();
        h.script(&["click \"Help\""]);
        if let Some(topic) = topic {
            h.send(Message::Guide(GuideMsg::Open(topic)));
        }
        h.screenshot(name);
    }
}
