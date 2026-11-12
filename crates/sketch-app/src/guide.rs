//! The in-app User Guide (Help > User Guide / Integration Guide).
//!
//! Not in the original web app: a topic list on the left, the chosen topic's
//! text on the right, in the same card/backdrop dress as the Keyboard
//! Shortcuts sheet. The text is static and lives in this file so it ships
//! inside the exe and always matches the build.

use iced::widget::{button, column, container, mouse_area, opaque, row, scrollable, text, Space};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::*;
use crate::Message;

/// One piece of a topic's body.
pub enum Block {
    /// A section heading inside the topic.
    Heading(&'static str),
    /// A paragraph.
    Para(&'static str),
    /// Numbered steps, done in order.
    Steps(&'static [&'static str]),
    /// Unordered points.
    Points(&'static [&'static str]),
    /// A caution or limit, set apart from the body text.
    Note(&'static str),
}

pub struct Topic {
    pub title: &'static str,
    pub blocks: &'static [Block],
}

/// The index of the Appraise-It topic, opened directly by Help >
/// Integration Guide.
pub const INTEGRATION_TOPIC: usize = 6;

use Block::{Heading, Note, Para, Points, Steps};

pub const TOPICS: &[Topic] = &[
    Topic {
        title: "Getting started",
        blocks: &[
            Para("Sketch draws floor plans to scale and calculates their areas. A sketch is made of walls, the areas they enclose, and symbols and text placed on top."),
            Heading("The window"),
            Points(&[
                "Title bar: Save, Undo, Redo and the command search, with a pill that tells you where the file stands (Unsaved, Saved or Final, with the version number). The caret opens New, Open, Save As and Options.",
                "Tabs: File opens the file menu. Home, View, Tools, Reference Image and Help each show a ribbon of commands. Edit Tools appears while something is selected.",
                "Left rail: the tools - Select, drawing walls, Measure and the rest. Hover a tool to see its name.",
                "Right dock: panels for defining areas, calculations, symbols, text and layers.",
                "Status bar: the current page, zoom and live totals.",
            ]),
            Heading("A first sketch"),
            Steps(&[
                "Press E to draw exterior walls.",
                "Click a starting point, then type a length and press an arrow key for the direction - for example 40 then Right arrow draws 40 feet to the right.",
                "Carry on around the building. Press A to close the shape back to the start.",
                "Choose what the area is (First Floor, Garage and so on) in the Define Area panel (F4).",
                "Open the Calculations panel (F6) to see the totals.",
                "Save with Ctrl + S.",
            ]),
            Para("Help > Keyboard Shortcuts (F1) lists every key."),
        ],
    },
    Topic {
        title: "Drawing walls",
        blocks: &[
            Para("Press E for exterior walls or I for interior walls. Walls can be drawn with the mouse, the keyboard, or both."),
            Heading("With the mouse"),
            Points(&[
                "Click to set each corner. The wall preview follows the pointer and snaps to existing corners.",
                "Dashed guide lines appear when the pointer lines up with another corner.",
                "Press Esc to stop drawing.",
            ]),
            Heading("With the keyboard"),
            Points(&[
                "Type a length, then press an arrow key: 12 then Up arrow draws 12 feet up.",
                "10x5 draws a diagonal that rises 10 and runs 5.",
                "10<45 draws 10 feet at 45 degrees.",
                "10l or 10r turns left or right from the last wall (90 degrees unless you give an angle).",
                "Tab types a decimal point; Backspace erases the last character typed.",
                "A closes the area back to its starting point. J jumps the cursor to the nearest corner.",
            ]),
            Heading("Changing walls afterwards"),
            Points(&[
                "Press P for the Select tool, then click a wall or area. The Edit Tools tab shows what can be changed: length, line style, thickness and color.",
                "Drag a corner to move it. Insert adds a corner in the middle of a wall.",
                "Arrow keys nudge the selection; Delete removes it; Ctrl + Z undoes.",
            ]),
        ],
    },
    Topic {
        title: "Areas and calculations",
        blocks: &[
            Para("A closed run of exterior walls becomes an area. Each area has an area code - such as First Floor, Second Floor, Garage or Porch - that decides how it counts in the totals."),
            Heading("Defining an area"),
            Steps(&[
                "Open the Define Area panel (F4).",
                "Pick the area code, either before you draw or afterwards by dragging the code onto a drawn area.",
                "The area takes the code's name, color and fill, and joins the calculations.",
            ]),
            Heading("Reading the numbers"),
            Points(&[
                "The Calculations panel (F6) lists each area with its size and perimeter, grouped by code, with the living-area total.",
                "Tools > Area Breakdown shows how each area was calculated, piece by piece.",
                "Tools > Area Codes edits the list of codes: names, colors, and whether a code counts toward living area.",
            ]),
            Heading("Subtracting one area from another"),
            Para("Select an area and switch on Edit Tools > Auto Subtract to take it out of the area around it - for example an open-to-below space inside a second floor."),
            Note("An unclosed run of walls is not an area and is left out of the totals. Close it with A, or by clicking back on its first corner."),
        ],
    },
    Topic {
        title: "Symbols and text",
        blocks: &[
            Heading("Symbols"),
            Points(&[
                "Press S to open the symbol library. Pick a category, or search by name.",
                "Click a symbol, then click the drawing to place it. You can also drag it from the panel onto the drawing.",
                "Doors and windows snap onto a wall and cut an opening in it. For a door, the side of the wall you click on is the side it swings to.",
                "Select a placed symbol to move, rotate or resize it with its handles. Resizing a door or window resizes its opening too.",
                "Manage symbols chooses your Favorites and renames, hides, moves and deletes library symbols.",
            ]),
            Heading("Text"),
            Points(&[
                "Press T to open the text library, pick a label and click the drawing to place it.",
                "Select a label to change its wording, font, size and color on the Edit Tools tab.",
                "Drag the handle above a selected label to rotate it.",
            ]),
        ],
    },
    Topic {
        title: "Pages, printing and export",
        blocks: &[
            Heading("Pages"),
            Para("A sketch can have several pages, for example one per floor. Use the page buttons on the View tab or the status bar to add, delete and move between pages."),
            Heading("Printing"),
            Points(&[
                "File > Print Preview shows the pages as they will print, with the area summary and breakdown tables beneath the drawing.",
                "File > Page Layout sets the paper size and what is printed.",
                "File > Print sends the previewed pages to the printer.",
            ]),
            Heading("Export and import"),
            Points(&[
                "File > Export as PDF, PNG, SVG or DXF writes the drawing in that format.",
                "File > Email opens a message to support in your mail program.",
                "File > Import CAD brings in a DXF or DWG drawing.",
                "The Reference Image tab loads a picture - a scan or an old sketch - behind the drawing so it can be traced. Set Scale matches it to real measurements.",
            ]),
        ],
    },
    Topic {
        title: "Saving and recovery",
        blocks: &[
            Points(&[
                "Ctrl + S saves to the current file. The first save asks where to put it; Save As (F12) saves a copy under a new name and carries on in that copy.",
                "Sketches are saved as .sketchy files. A .sketchy file keeps every save as a version, so earlier states of the sketch are never lost (see Version history below).",
                "The pill in the title bar reads Unsaved while there are changes that are not in the file, and Saved when there are none. A .sketchy file adds its version number: Saved · v3. Marked as final it reads Final · v3, a file opened read-only reads Read-only · v3, and an old version on screen reads Viewing v1. Closing with unsaved changes asks whether to save them.",
                "File > Recent Files reopens a sketch you worked on lately.",
                "Drop a sketch file on the window to open it. Each window holds one sketch; opening a sketch that is already open brings its window forward.",
            ]),
            Heading("Version history"),
            Points(&[
                "File > Version History shows a panel listing every version of the sketch, newest first, with its date and note.",
                "Open shows an old version read-only, under a banner. Nothing can be edited there; Back to latest returns to the current sketch.",
                "Restore this version (on the banner) brings the old version back as a new latest version. Nothing is deleted, so you can go back again.",
                "Save a copy writes any version out as a separate .sketchy file and leaves the original alone.",
                "Mark as Final records the current sketch as a signed-off version with an optional note. A final version is never changed; saving more work adds a new version after it.",
                "Saving without making changes adds no new version.",
            ]),
            Heading("Autosave"),
            Para("While you work, Sketch keeps a recovery copy of the open sketch. If Sketch closes unexpectedly, the next start offers to restore it; File > Recover Autosave does the same at any time. Autosave is switched on, and its interval set, in Options."),
            Note("The recovery copy is a safety net, not a save. It is removed once you save or deliberately discard your changes."),
        ],
    },
    Topic {
        title: "Appraise-It integration",
        blocks: &[
            Para("Sketch can stand in for Apex as the sketch program Appraise-It Pro opens. You start it from the report, draw, and the sketch, its measurements and the sketch images go back into the report."),
            Heading("Set up (once per computer)"),
            Steps(&[
                "Put the whole Sketch folder where it will stay. The report opens Sketch from that exact location, so do not run setup from a download or temporary folder.",
                "Start Sketch from that folder and open Help > Appraise-It Integration.",
                "Check that the path shown is the copy of Sketch you want the report to open.",
                "Click Enable integration. If Windows asks for administrator permission, allow it.",
                "Wait for the panel to report success, then close it.",
                "Close and restart Appraise-It Pro.",
            ]),
            Para("Setup changes the shared Apex launch setting on this computer. It does not change Appraise-It Pro's own files."),
            Heading("Sketching from a report"),
            Steps(&[
                "In Appraise-It Pro, open the report and click its usual Apex sketch button. Sketch opens with that report's sketch, or an empty one.",
                "Draw and define the areas as usual.",
                "Close Sketch. It asks what to do with your work:",
            ]),
            Points(&[
                "Save to report - sends the sketch, the area measurements and the sketch images to the report, then closes once the report confirms it received them.",
                "Discard changes - closes without sending. The report keeps the sketch it had before.",
                "Cancel - returns to the drawing.",
            ]),
            Para("For a UAD 3.6 report, Appraise-It Pro then shows its own Transfer sketches window. Finish that window, or the images are not stored in the report."),
            Heading("After updating or moving Sketch"),
            Para("The report opens the copy of Sketch that was registered. If you install a new version in a different folder, or move the folder, open Help > Appraise-It Integration in the new copy and click Enable integration again, then restart Appraise-It Pro."),
            Heading("Going back to Apex"),
            Para("Open Help > Appraise-It Integration and click Restore previous provider, then restart Appraise-It Pro. The report's sketch button opens the program it used before."),
            Heading("If something goes wrong"),
            Points(&[
                "The report still opens Apex: restart Appraise-It Pro, then check that the integration panel says it is enabled for this copy of Sketch.",
                "Setup was not completed: the Windows permission prompt was probably declined. Click Enable integration again and allow it.",
                "Sketch stays open after Save to report: the report did not confirm the transfer. Your drawing is still there - check the report is still open, then save to the report again.",
                "The report shows no sketch images (UAD): the Transfer sketches window in Appraise-It Pro was not completed.",
            ]),
            Note("This is a development preview. It has been checked with Appraise-It Pro legacy and UAD 3.6 reports; other host programs have not been validated. Keep a backup of important reports until you trust the round trip."),
        ],
    },
];

/// The guide overlay's state.
#[derive(Debug, Clone, Default)]
pub struct Guide {
    pub open: bool,
    pub topic: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideMsg {
    /// Open at a topic (the Help tab's two buttons).
    Open(usize),
    /// Switch topic from the list.
    Topic(usize),
    Close,
}

impl Guide {
    pub fn update(&mut self, msg: GuideMsg) {
        match msg {
            GuideMsg::Open(topic) => {
                self.open = true;
                self.topic = topic.min(TOPICS.len() - 1);
            }
            GuideMsg::Topic(topic) => self.topic = topic.min(TOPICS.len() - 1),
            GuideMsg::Close => self.open = false,
        }
    }
}

const BODY: f32 = 12.5;

fn para(s: &'static str, color: Color) -> Element<'static, Message> {
    text(s)
        .font(SEGOE)
        .size(BODY)
        .line_height(iced::widget::text::LineHeight::Relative(1.45))
        .color(color)
        .width(Length::Fill)
        .into()
}

/// A list row: a fixed-width marker, then the wrapped text.
fn item(marker: String, s: &'static str) -> Element<'static, Message> {
    row![
        text(marker)
            .font(SEGOE_SEMIBOLD)
            .size(BODY)
            .line_height(iced::widget::text::LineHeight::Relative(1.45))
            .color(muted())
            .width(22),
        para(s, text_main()),
    ]
    .into()
}

fn block(b: &'static Block) -> Element<'static, Message> {
    match b {
        Heading(s) => container(text(*s).font(SEGOE_SEMIBOLD).size(13.5).color(text_main()))
            .padding(iced::Padding {
                top: 8.0,
                ..iced::Padding::ZERO
            })
            .into(),
        Para(s) => para(s, text_main()),
        Steps(steps) => column(
            steps
                .iter()
                .enumerate()
                .map(|(i, s)| item(format!("{}.", i + 1), s)),
        )
        .spacing(5)
        .into(),
        Points(points) => column(points.iter().map(|s| item("\u{2022}".into(), s)))
            .spacing(5)
            .into(),
        Note(s) => container(para(s, text_main()))
            .padding([8, 10])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(bg_app())),
                border: Border {
                    color: border(),
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            })
            .into(),
    }
}

/// The guide's layer, or None when closed. `size` is the window's, so the
/// card fits a small window.
pub fn layer(guide: &Guide, size: iced::Size) -> Option<Element<'static, Message>> {
    if !guide.open {
        return None;
    }
    let current = guide.topic.min(TOPICS.len() - 1);
    let topic = &TOPICS[current];

    let mut list = column![text("USER GUIDE")
        .font(SEGOE_SEMIBOLD)
        .size(11.0)
        .color(muted())]
    .spacing(2)
    .width(190);
    list = list.push(Space::new().height(4));
    for (i, t) in TOPICS.iter().enumerate() {
        let active = i == current;
        list = list.push(
            button(
                container(text(t.title).font(SEGOE).size(12.5).color(if active {
                    WHITE
                } else {
                    text_main()
                }))
                .padding([6, 10])
                .width(Length::Fill),
            )
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::Guide(GuideMsg::Topic(i)))
            .style(move |_, status| button::Style {
                background: if active {
                    Some(Background::Color(accent()))
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    Some(Background::Color(bg_hover()))
                } else {
                    None
                },
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        );
    }

    let mut body = column![text(topic.title)
        .font(SEGOE_SEMIBOLD)
        .size(18.0)
        .color(text_main())]
    .spacing(9)
    .padding(iced::Padding {
        right: 16.0,
        ..iced::Padding::ZERO
    });
    for b in topic.blocks {
        body = body.push(block(b));
    }

    let close = button(container(text("Close").font(SEGOE).size(12.0).color(WHITE)).padding([5, 12]))
        .padding(0)
        .on_press(Message::Guide(GuideMsg::Close))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(accent())),
            ..button::Style::default()
        });

    let reading = column![
        scrollable(body).width(Length::Fill).height(Length::Fill),
        row![Space::new().width(Length::Fill), close],
    ]
    .spacing(10);

    let divider = container(Space::new().width(1).height(Length::Fill)).style(|_| container::Style {
        background: Some(Background::Color(border())),
        ..container::Style::default()
    });

    let card = container(row![list, divider, reading].spacing(14).padding(14))
        .width(860.0_f32.min((size.width - 32.0).max(320.0)))
        .height(620.0_f32.min((size.height - 48.0).max(240.0)))
        .style(|_| container::Style {
            background: Some(Background::Color(bg_panel())),
            border: Border {
                color: border(),
                width: 1.0,
                radius: 5.0.into(),
            },
            ..container::Style::default()
        });

    // The card swallows its own presses; the backdrop closes.
    let centred = container(opaque(card))
        .center_x(Length::Fill)
        .center_y(Length::Fill);
    let backdrop = container(centred)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.46).into()),
            ..container::Style::default()
        });
    Some(opaque(
        mouse_area(backdrop).on_press(Message::Guide(GuideMsg::Close)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_integration_shortcut_points_at_the_integration_topic() {
        assert_eq!(TOPICS[INTEGRATION_TOPIC].title, "Appraise-It integration");
    }

    #[test]
    fn every_topic_has_a_title_and_non_empty_text() {
        for topic in TOPICS {
            assert!(!topic.title.is_empty());
            assert!(!topic.blocks.is_empty(), "{}", topic.title);
            for b in topic.blocks {
                match b {
                    Heading(s) | Para(s) | Note(s) => assert!(!s.trim().is_empty()),
                    Steps(items) | Points(items) => {
                        assert!(!items.is_empty());
                        assert!(items.iter().all(|s| !s.trim().is_empty()));
                    }
                }
            }
        }
    }

    #[test]
    fn saving_topic_describes_sketchy_versions() {
        let topic = TOPICS.iter().find(|t| t.title == "Saving and recovery").expect("topic");
        let text: String = topic
            .blocks
            .iter()
            .flat_map(|b| match b {
                Heading(s) | Para(s) | Note(s) => vec![*s],
                Steps(items) | Points(items) => items.to_vec(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [".sketchy", "Version History", "read-only", "Restore this version", "Save a copy", "Mark as Final", "Final · v3", "Read-only · v3"] {
            assert!(text.contains(needle), "guide lacks {needle:?}");
        }
    }

    #[test]
    fn open_picks_the_topic_and_out_of_range_is_clamped() {
        let mut guide = Guide::default();
        guide.update(GuideMsg::Open(INTEGRATION_TOPIC));
        assert!(guide.open);
        assert_eq!(guide.topic, INTEGRATION_TOPIC);
        guide.update(GuideMsg::Topic(999));
        assert_eq!(guide.topic, TOPICS.len() - 1);
        guide.update(GuideMsg::Close);
        assert!(!guide.open);
    }
}
