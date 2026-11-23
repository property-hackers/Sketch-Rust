//! Core widget kit for the Next shell (`docs/ui-redesign/01-design-spec.md`, `02-architecture.md`).
//!
//! Every widget reads colour, spacing, radii, typography, and density metrics
//! from `ui::tokens` and `ui::fonts`, draws icons in their meaning tint at
//! rest (`crate::icons::tinted`) and in one state colour when active or
//! disabled, and avoids `theme::at` fixed coordinates.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::widget::{
    button, column, container, mouse_area, row, rule, text, text_input, tooltip as iced_tooltip,
    Space,
};
use iced::{
    mouse, touch, window, Alignment, Background, Border, Color, Element, Event, Length, Padding,
    Point, Rectangle, Size,
};

use crate::icons;
use crate::ui::fonts::{BARLOW, BARLOW_MEDIUM, BARLOW_SEMIBOLD, BSC_MEDIUM, BSC_SEMIBOLD};
use crate::ui::tokens::{
    self, Density, FOCUS_RING_WIDTH, RADIUS_CONTROL, RADIUS_DIALOG, RADIUS_SHEET, RADIUS_TILE,
    RADIUS_TRAY, SPACE_1, SPACE_2, SPACE_3, SPACE_4, TEXT_11, TEXT_12, TEXT_13, TEXT_14,
};
use crate::Message;

const TOOLTIP_DELAY: Duration = Duration::from_millis(450);
pub const LONG_PRESS_DELAY: Duration = Duration::from_millis(500);
pub const LONG_PRESS_SLOP_PX: f32 = 8.0;

/// Wrap a control so agents can target it as `#id` (iced 0.14 `Button` has no `.id()`).
pub fn with_id<'a>(
    id: impl Into<widget::Id>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content).id(id).into()
}

// ---------------------------------------------------------------------------
// Shared visual helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct SurfaceSpec {
    bg: Color,
    hover_bg: Color,
    pressed_bg: Color,
    disabled_bg: Color,
    fg: Color,
    disabled_fg: Color,
    border: Option<Color>,
    radius: f32,
}

fn apply_surface_style(spec: SurfaceSpec, focused: bool, status: button::Status) -> button::Style {
    let (bg, fg, is_disabled) = match status {
        button::Status::Active => (spec.bg, spec.fg, false),
        button::Status::Hovered => (spec.hover_bg, spec.fg, false),
        button::Status::Pressed => (spec.pressed_bg, spec.fg, false),
        button::Status::Disabled => (spec.disabled_bg, spec.disabled_fg, true),
    };
    let border = if focused && !is_disabled {
        Border {
            color: tokens::accent(),
            width: FOCUS_RING_WIDTH,
            radius: spec.radius.into(),
        }
    } else if let Some(b) = spec.border {
        Border {
            color: b,
            width: 1.0,
            radius: spec.radius.into(),
        }
    } else {
        Border {
            radius: spec.radius.into(),
            ..Border::default()
        }
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border,
        ..button::Style::default()
    }
}

/// Wraps any element in a 2px `tokens::accent()` keyboard-focus outline when `focused` is true.
#[allow(dead_code)]
pub fn focus_ring<'a>(
    focused: bool,
    radius: f32,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !focused {
        return content.into();
    }
    container(content)
        .padding(1)
        .style(move |_| container::Style {
            border: Border {
                color: tokens::accent(),
                width: FOCUS_RING_WIDTH,
                radius: radius.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// Thin 1px horizontal divider using `tokens::rule()`.
pub fn divider<'a>() -> Element<'a, Message> {
    rule::horizontal(1)
        .style(|_| rule::Style {
            color: tokens::rule(),
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}

/// Vertical 1px bar divider with an explicit height (avoids `Length::Fill` inside scrollables).
pub fn v_divider<'a>(height: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(height))
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::rule())),
            ..container::Style::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 1. text_button
// ---------------------------------------------------------------------------

/// Secondary / ghost text button (`on_press = None` renders disabled).
pub fn text_button<'a>(
    label: impl Into<String>,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    text_button_with(label, on_press, density, false, false)
}

/// Secondary / link text button with explicit `active` / `focused` flags.
pub fn text_button_with<'a>(
    label: impl Into<String>,
    on_press: Option<Message>,
    density: Density,
    active: bool,
    focused: bool,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let label_color = if disabled {
        tokens::disabled()
    } else if active {
        tokens::link()
    } else {
        tokens::text()
    };
    let bg = if active && !disabled {
        tokens::accent_soft()
    } else {
        tokens::panel()
    };
    let spec = SurfaceSpec {
        bg,
        hover_bg: tokens::hover(),
        pressed_bg: tokens::pressed(),
        disabled_bg: tokens::panel(),
        fg: label_color,
        disabled_fg: tokens::disabled(),
        border: Some(tokens::rule()),
        radius: RADIUS_CONTROL,
    };
    let mut btn = button(
        container(
            text(label.into())
                .font(BARLOW_MEDIUM)
                .size(TEXT_13)
                .color(label_color),
        )
        .align_y(Alignment::Center)
        .align_x(Alignment::Center)
        .height(Length::Fixed(density.min_target())),
    )
    .padding([0, 12])
    .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

// ---------------------------------------------------------------------------
// 2. primary_button
// ---------------------------------------------------------------------------

/// High-emphasis accent button (`on_press = None` renders disabled).
pub fn primary_button<'a>(
    label: impl Into<String>,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    primary_button_with(label, on_press, density, false)
}

/// Accent primary button with explicit keyboard `focused` state.
pub fn primary_button_with<'a>(
    label: impl Into<String>,
    on_press: Option<Message>,
    density: Density,
    focused: bool,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let fg = if disabled {
        tokens::disabled()
    } else {
        tokens::on_accent()
    };
    let accent = tokens::accent();
    let hover_bg = crate::theme::over(Color::WHITE, 0.12, accent);
    let pressed_bg = crate::theme::over(Color::BLACK, 0.18, accent);
    let spec = SurfaceSpec {
        bg: accent,
        hover_bg,
        pressed_bg,
        disabled_bg: tokens::hover(),
        fg,
        disabled_fg: tokens::disabled(),
        border: None,
        radius: RADIUS_CONTROL,
    };
    let mut btn = button(
        container(
            text(label.into())
                .font(BARLOW_SEMIBOLD)
                .size(TEXT_13)
                .color(fg),
        )
        .align_y(Alignment::Center)
        .align_x(Alignment::Center)
        .height(Length::Fixed(density.min_target())),
    )
    .padding([0, 14])
    .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

// ---------------------------------------------------------------------------
// 3. icon_button
// ---------------------------------------------------------------------------

/// Compact square monochrome icon button (`active` highlights with `accent_soft`).
pub fn icon_button<'a>(
    icon_name: &str,
    active: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    icon_button_with(icon_name, active, false, on_press, density)
}

/// A toggle drawn as one icon that is bright while `lit` and greyed out while
/// not, with no highlight box: the eye and the lock of a layer row. It always
/// has the same size, so the controls around it never move.
pub fn lit_icon_button<'a>(
    icon_name: &str,
    lit: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let fg = if lit {
        tokens::text()
    } else {
        tokens::disabled()
    };
    let size = density.min_target();
    let spec = SurfaceSpec {
        bg: Color::TRANSPARENT,
        hover_bg: tokens::hover(),
        pressed_bg: tokens::pressed(),
        disabled_bg: Color::TRANSPARENT,
        fg,
        disabled_fg: tokens::disabled(),
        border: None,
        radius: RADIUS_CONTROL,
    };
    let mut btn = button(
        container(icons::mono(icon_name, density.icon_size(), fg))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .style(move |_, status| apply_surface_style(spec, false, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

/// Monochrome icon button with `active` and keyboard `focused` states.
pub fn icon_button_with<'a>(
    icon_name: &str,
    active: bool,
    focused: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let fg = if disabled {
        tokens::disabled()
    } else if active {
        tokens::accent()
    } else {
        tokens::text()
    };
    let bg = if active && !disabled {
        tokens::accent_soft()
    } else {
        Color::TRANSPARENT
    };
    let size = density.min_target();
    let icon_px = density.icon_size();
    let spec = SurfaceSpec {
        bg,
        hover_bg: tokens::hover(),
        pressed_bg: tokens::pressed(),
        disabled_bg: Color::TRANSPARENT,
        fg,
        disabled_fg: tokens::disabled(),
        border: if active && !disabled {
            Some(tokens::accent())
        } else {
            None
        },
        radius: RADIUS_CONTROL,
    };
    let mut btn = button(
        container(if active || disabled {
            icons::mono(icon_name, icon_px, fg)
        } else {
            icons::tinted(icon_name, false, icon_px, fg)
        })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

// ---------------------------------------------------------------------------
// 4. field
// ---------------------------------------------------------------------------

/// Parameters for `kit::field`.
pub struct FieldProps<'s, 'a> {
    pub label: &'s str,
    pub value: &'s str,
    pub placeholder: &'s str,
    pub on_input: Option<Box<dyn Fn(String) -> Message + 'a>>,
    pub on_submit: Option<Message>,
    pub focused: bool,
}

/// Labelled text input field styled with `tokens::input_bg()`, `RADIUS_CONTROL`, and focus ring.
pub fn field<'a>(props: FieldProps<'_, 'a>, density: Density) -> Element<'a, Message> {
    let disabled = props.on_input.is_none();
    let mut input = text_input(props.placeholder, props.value)
        .font(BARLOW)
        .size(TEXT_13)
        .padding([((density.field_h() - 18.0).max(4.0)) * 0.5, SPACE_2 + 2.0])
        .width(Length::Fill)
        .style(move |_, status| {
            let is_focused = props.focused || matches!(status, text_input::Status::Focused { .. });
            let border_color = match status {
                text_input::Status::Disabled => tokens::rule(),
                _ if is_focused => tokens::accent(),
                text_input::Status::Hovered => tokens::muted(),
                _ => tokens::rule(),
            };
            text_input::Style {
                background: Background::Color(if disabled {
                    tokens::panel()
                } else {
                    tokens::input_bg()
                }),
                border: Border {
                    color: border_color,
                    width: if is_focused && !disabled {
                        FOCUS_RING_WIDTH
                    } else {
                        1.0
                    },
                    radius: RADIUS_CONTROL.into(),
                },
                icon: tokens::muted(),
                placeholder: tokens::muted(),
                value: if disabled {
                    tokens::disabled()
                } else {
                    tokens::text()
                },
                selection: tokens::accent_soft(),
            }
        });

    if let Some(on_input) = props.on_input {
        input = input.on_input(on_input);
    }
    if let Some(on_submit) = props.on_submit {
        input = input.on_submit(on_submit);
    }

    if props.label.is_empty() {
        return input.into();
    }

    column![
        text(props.label.to_owned())
            .font(BARLOW_MEDIUM)
            .size(TEXT_12)
            .color(if disabled {
                tokens::disabled()
            } else {
                tokens::muted()
            }),
        input,
    ]
    .spacing(SPACE_1)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// 5. stepper
// ---------------------------------------------------------------------------

/// Numeric `- / value / +` stepper control using `BSC_MEDIUM` tabular readout.
pub fn stepper<'a>(
    label: impl Into<String>,
    value_text: impl Into<String>,
    on_dec: Option<Message>,
    on_inc: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let label = label.into();
    let disabled = on_dec.is_none() && on_inc.is_none();
    let btn_size = density.field_h();
    let step_btn = |symbol: &'static str, msg: Option<Message>| {
        let fg = if msg.is_some() {
            tokens::text()
        } else {
            tokens::disabled()
        };
        let spec = SurfaceSpec {
            bg: tokens::field(),
            hover_bg: tokens::hover(),
            pressed_bg: tokens::pressed(),
            disabled_bg: tokens::panel(),
            fg,
            disabled_fg: tokens::disabled(),
            border: None,
            radius: 0.0,
        };
        let mut b = button(
            container(text(symbol).font(BARLOW_SEMIBOLD).size(TEXT_14).color(fg))
                .width(Length::Fixed(btn_size))
                .height(Length::Fixed(btn_size))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .style(move |_, status| apply_surface_style(spec, false, status));
        if let Some(m) = msg {
            b = b.on_press(m);
        }
        b
    };

    let strip = container(
        row![
            step_btn("-", on_dec),
            v_divider(btn_size),
            container(
                text(value_text.into())
                    .font(BSC_MEDIUM)
                    .size(TEXT_13)
                    .color(if disabled {
                        tokens::disabled()
                    } else {
                        tokens::text()
                    }),
            )
            .padding([0, 10])
            .height(Length::Fixed(btn_size))
            .align_y(Alignment::Center)
            .align_x(Alignment::Center),
            v_divider(btn_size),
            step_btn("+", on_inc),
        ]
        .height(Length::Fixed(btn_size))
        .align_y(Alignment::Center),
    )
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::input_bg())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    });

    if label.is_empty() {
        strip.into()
    } else {
        row![
            text(label)
                .font(BARLOW_MEDIUM)
                .size(TEXT_12)
                .color(tokens::muted()),
            Space::new().width(Length::Fill),
            strip,
        ]
        .align_y(Alignment::Center)
        .spacing(SPACE_2)
        .into()
    }
}

// ---------------------------------------------------------------------------
// 6. segmented
// ---------------------------------------------------------------------------

/// A segment option in `kit::segmented`.
pub struct Segment<'a> {
    pub label: &'a str,
    pub selected: bool,
    pub focused: bool,
    pub on_press: Option<Message>,
}

/// Mutually exclusive segmented button group.
pub fn segmented<'a>(segments: Vec<Segment<'a>>, density: Density) -> Element<'a, Message> {
    let mut r = row![].spacing(2).align_y(Alignment::Center);
    for seg in segments {
        let disabled = seg.on_press.is_none();
        let fg = if disabled {
            tokens::disabled()
        } else if seg.selected {
            tokens::on_accent()
        } else {
            tokens::text()
        };
        let bg = if seg.selected && !disabled {
            tokens::accent()
        } else {
            tokens::field()
        };
        let spec = SurfaceSpec {
            bg,
            hover_bg: if seg.selected {
                crate::theme::over(Color::WHITE, 0.10, tokens::accent())
            } else {
                tokens::hover()
            },
            pressed_bg: tokens::pressed(),
            disabled_bg: tokens::panel(),
            fg,
            disabled_fg: tokens::disabled(),
            border: None,
            radius: RADIUS_CONTROL - 2.0,
        };
        let mut btn = button(
            container(
                text(seg.label)
                    .font(if seg.selected {
                        BARLOW_SEMIBOLD
                    } else {
                        BARLOW_MEDIUM
                    })
                    .size(TEXT_12)
                    .color(fg),
            )
            .height(Length::Fixed((density.field_h() - 4.0).max(24.0)))
            .padding([0, 10])
            .align_y(Alignment::Center)
            .align_x(Alignment::Center),
        )
        .padding(0)
        .style(move |_, status| apply_surface_style(spec, seg.focused, status));
        if let Some(m) = seg.on_press {
            btn = btn.on_press(m);
        }
        r = r.push(btn);
    }

    container(r)
        .padding(2)
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

// ---------------------------------------------------------------------------
// 7. chip
// ---------------------------------------------------------------------------

/// Compact toggle pill / filter chip.
pub fn chip<'a>(
    label: impl Into<String>,
    selected: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    chip_with(label, selected, false, on_press, density)
}

/// Toggle pill / filter chip with keyboard `focused` state.
pub fn chip_with<'a>(
    label: impl Into<String>,
    selected: bool,
    focused: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let fg = if disabled {
        tokens::disabled()
    } else if selected {
        tokens::accent()
    } else {
        tokens::text()
    };
    let spec = SurfaceSpec {
        bg: if selected && !disabled {
            tokens::accent_soft()
        } else {
            tokens::field()
        },
        hover_bg: tokens::hover(),
        pressed_bg: tokens::pressed(),
        disabled_bg: tokens::panel(),
        fg,
        disabled_fg: tokens::disabled(),
        border: Some(if selected && !disabled {
            tokens::accent()
        } else {
            tokens::rule()
        }),
        radius: 999.0,
    };
    let h = if matches!(density, Density::Touch) {
        36.0
    } else {
        26.0
    };
    let mut btn = button(
        container(
            text(label.into())
                .font(if selected {
                    BARLOW_SEMIBOLD
                } else {
                    BARLOW_MEDIUM
                })
                .size(TEXT_12)
                .color(fg),
        )
        .height(Length::Fixed(h))
        .padding([0, 10])
        .align_y(Alignment::Center)
        .align_x(Alignment::Center),
    )
    .padding(0)
    .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

// ---------------------------------------------------------------------------
// 8. list_row
// ---------------------------------------------------------------------------

/// Parameters for `kit::list_row` (ledger / browser / menu lists).
pub struct ListRowProps<'a> {
    pub swatch: Option<Color>,
    pub title: &'a str,
    pub value: Option<&'a str>,
    pub selected: bool,
    pub flagged: bool,
    pub focused: bool,
    pub on_press: Option<Message>,
}

/// Owned-string variant of [`ListRowProps`] when row text is computed locally.
pub struct OwnedListRowProps {
    pub swatch: Option<Color>,
    pub title: String,
    pub value: Option<String>,
    pub selected: bool,
    pub flagged: bool,
    pub focused: bool,
    pub on_press: Option<Message>,
}

/// Selectable ledger / browser row adhering strictly to `Density::row_h()`.
pub fn list_row<'a>(props: ListRowProps<'a>, density: Density) -> Element<'a, Message> {
    list_row_with(
        props.swatch,
        props.title,
        props.value,
        props.selected,
        props.flagged,
        props.focused,
        props.on_press,
        density,
    )
}

/// Selectable ledger / browser row taking owned strings.
pub fn list_row_owned<'a>(props: OwnedListRowProps, density: Density) -> Element<'a, Message> {
    list_row_with(
        props.swatch,
        props.title,
        props.value,
        props.selected,
        props.flagged,
        props.focused,
        props.on_press,
        density,
    )
}

#[allow(clippy::too_many_arguments)]
fn list_row_with<'a>(
    swatch: Option<Color>,
    title: impl iced::widget::text::IntoFragment<'a>,
    value: Option<impl iced::widget::text::IntoFragment<'a>>,
    selected: bool,
    flagged: bool,
    focused: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let title_color = if disabled {
        tokens::disabled()
    } else if flagged {
        tokens::flag()
    } else {
        tokens::text()
    };
    let value_color = if disabled {
        tokens::disabled()
    } else if flagged {
        tokens::flag()
    } else {
        tokens::muted()
    };

    let mut content = row![].spacing(SPACE_2).align_y(Alignment::Center);
    if let Some(c) = swatch {
        content = content.push(
            container(Space::new())
                .width(Length::Fixed(10.0))
                .height(Length::Fixed(10.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(c)),
                    border: Border {
                        color: tokens::rule(),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..container::Style::default()
                }),
        );
    }

    content = content.push(
        text(title)
            .font(if selected {
                BARLOW_SEMIBOLD
            } else {
                BARLOW_MEDIUM
            })
            .size(TEXT_13)
            .color(title_color)
            .width(Length::Fill),
    );

    if let Some(val) = value {
        content = content.push(text(val).font(BSC_MEDIUM).size(TEXT_13).color(value_color));
    }

    let spec = SurfaceSpec {
        bg: if selected && !disabled {
            tokens::accent_soft()
        } else {
            Color::TRANSPARENT
        },
        hover_bg: tokens::hover(),
        pressed_bg: tokens::pressed(),
        disabled_bg: Color::TRANSPARENT,
        fg: title_color,
        disabled_fg: tokens::disabled(),
        border: if selected && !disabled {
            Some(tokens::accent())
        } else {
            None
        },
        radius: RADIUS_CONTROL,
    };

    let mut btn = button(
        container(content)
            .height(Length::Fixed(density.row_h()))
            .padding([0, 8])
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(0)
    .width(Length::Fill)
    .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

// ---------------------------------------------------------------------------
// 9. group_header
// ---------------------------------------------------------------------------

/// Uppercase / bold section header with optional right-aligned action link or count.
pub fn group_header<'a>(
    title: impl Into<String>,
    trailing: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut r = row![
        text(title.into())
            .font(BSC_SEMIBOLD)
            .size(TEXT_11)
            .color(tokens::muted()),
        Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if let Some(extra) = trailing {
        r = r.push(extra);
    }

    container(r).padding([4, 2]).width(Length::Fill).into()
}

// ---------------------------------------------------------------------------
// 10. swatch
// ---------------------------------------------------------------------------

/// Clickable colour swatch square with active/focused ring.
pub fn swatch<'a>(
    color: Color,
    selected: bool,
    focused: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let size = if matches!(density, Density::Touch) {
        36.0
    } else {
        24.0
    };
    let spec = SurfaceSpec {
        bg: color,
        hover_bg: color,
        pressed_bg: color,
        disabled_bg: tokens::hover(),
        fg: tokens::text(),
        disabled_fg: tokens::disabled(),
        border: Some(if selected {
            tokens::text()
        } else {
            tokens::rule()
        }),
        radius: RADIUS_CONTROL,
    };
    let inner: Element<'a, Message> = if selected {
        container(icons::mono("check", 12.0, contrast_ink(color)))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    } else {
        Space::new()
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .into()
    };
    let mut btn = button(inner)
        .padding(0)
        .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

fn contrast_ink(bg: Color) -> Color {
    let lum = 0.2126 * bg.r + 0.7152 * bg.g + 0.0722 * bg.b;
    if lum > 0.52 {
        Color::from_rgb(0.12, 0.14, 0.17)
    } else {
        Color::WHITE
    }
}

// ---------------------------------------------------------------------------
// 11. tile
// ---------------------------------------------------------------------------

/// Vertical icon + short label tile used in the floating tool tray (`density.tray_button()`).
pub fn tile<'a>(
    icon_name: &str,
    label: impl Into<String>,
    active: bool,
    focused: bool,
    on_press: Option<Message>,
    density: Density,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    let fg = if disabled {
        tokens::disabled()
    } else if active {
        tokens::on_accent()
    } else {
        tokens::text()
    };
    let (w, h) = density.tray_button();
    let spec = SurfaceSpec {
        bg: if active && !disabled {
            tokens::accent()
        } else {
            tokens::bar()
        },
        hover_bg: if active && !disabled {
            crate::theme::over(Color::WHITE, 0.10, tokens::accent())
        } else {
            tokens::hover()
        },
        pressed_bg: tokens::pressed(),
        disabled_bg: tokens::bar(),
        fg,
        disabled_fg: tokens::disabled(),
        border: None,
        radius: RADIUS_TILE,
    };

    let body = column![
        if active || disabled {
            icons::mono(icon_name, density.icon_size(), fg)
        } else {
            icons::tinted(icon_name, false, density.icon_size(), fg)
        },
        text(label.into()).font(BSC_MEDIUM).size(TEXT_11).color(fg),
    ]
    .spacing(3)
    .align_x(Alignment::Center);

    let mut btn = button(
        container(body)
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .style(move |_, status| apply_surface_style(spec, focused, status));
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

// ---------------------------------------------------------------------------
// 12. menu
// ---------------------------------------------------------------------------

/// An entry in `kit::menu`.
#[allow(clippy::large_enum_variant)]
pub enum MenuEntry<'a> {
    Item {
        icon: Option<&'a str>,
        label: &'a str,
        shortcut: Option<&'a str>,
        checked: bool,
        focused: bool,
        on_press: Option<Message>,
    },
    Separator,
}

/// Floating context / overflow command menu with keyboard shortcuts and checkmarks.
pub fn menu<'a>(entries: Vec<MenuEntry<'a>>, width: f32, density: Density) -> Element<'a, Message> {
    let mut col = column![].spacing(2).width(Length::Fixed(width));
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                col = col.push(container(divider()).padding([4, 6]));
            }
            MenuEntry::Item {
                icon,
                label,
                shortcut,
                checked,
                focused,
                on_press,
            } => {
                let disabled = on_press.is_none();
                let fg = if disabled {
                    tokens::disabled()
                } else {
                    tokens::text()
                };
                let mut r = row![].spacing(SPACE_2).align_y(Alignment::Center);
                if checked {
                    r = r.push(icons::mono("check", 14.0, tokens::accent()));
                } else if let Some(name) = icon {
                    r = r.push(if disabled {
                        icons::mono(name, 14.0, fg)
                    } else {
                        icons::tinted(name, false, 14.0, fg)
                    });
                } else {
                    r = r.push(Space::new().width(Length::Fixed(14.0)));
                }
                r = r.push(
                    text(label)
                        .font(BARLOW_MEDIUM)
                        .size(TEXT_13)
                        .color(fg)
                        .width(Length::Fill),
                );
                if let Some(key) = shortcut {
                    r = r.push(text(key).font(BSC_MEDIUM).size(TEXT_12).color(if disabled {
                        tokens::disabled()
                    } else {
                        tokens::muted()
                    }));
                }

                let spec = SurfaceSpec {
                    bg: Color::TRANSPARENT,
                    hover_bg: tokens::hover(),
                    pressed_bg: tokens::pressed(),
                    disabled_bg: Color::TRANSPARENT,
                    fg,
                    disabled_fg: tokens::disabled(),
                    border: None,
                    radius: RADIUS_CONTROL,
                };
                let mut btn = button(
                    container(r)
                        .height(Length::Fixed(density.row_h().min(36.0)))
                        .padding([0, 8])
                        .align_y(Alignment::Center)
                        .width(Length::Fill),
                )
                .padding(0)
                .width(Length::Fill)
                .style(move |_, status| apply_surface_style(spec, focused, status));
                if let Some(msg) = on_press {
                    btn = btn.on_press(msg);
                }
                col = col.push(btn);
            }
        }
    }

    container(col)
        .padding(SPACE_1 + 2.0)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::bar())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_DIALOG.into(),
            },
            shadow: tokens::floating_shadow(),
            ..container::Style::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 13. popover
// ---------------------------------------------------------------------------

/// Elevated floating card used for inspector popovers, palette menus, and Pad shell.
pub fn popover<'a>(content: impl Into<Element<'a, Message>>, radius: f32) -> Element<'a, Message> {
    container(content)
        .padding(SPACE_3)
        .style(move |_| container::Style {
            background: Some(Background::Color(tokens::bar())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: radius.into(),
            },
            shadow: tokens::floating_shadow(),
            ..container::Style::default()
        })
        .into()
}

/// Floating tray card with `RADIUS_TRAY` and shadow.
pub fn tray_surface<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(SPACE_1 + 2.0)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::bar())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_TRAY.into(),
            },
            shadow: tokens::floating_shadow(),
            ..container::Style::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 14. sheet_handle
// ---------------------------------------------------------------------------

/// Compact bottom-sheet / slide-over drag pill (`on_press` cycles Peek/Half/Expanded or dismisses).
pub fn sheet_handle<'a>(
    title: impl Into<String>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let title = title.into();
    let pill = container(Space::new())
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(4.0))
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::muted())),
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let mut col = column![container(pill)
        .width(Length::Fill)
        .align_x(Alignment::Center)]
    .spacing(SPACE_1)
    .width(Length::Fill);

    if !title.is_empty() {
        col = col.push(
            container(
                text(title)
                    .font(BARLOW_SEMIBOLD)
                    .size(TEXT_13)
                    .color(tokens::text()),
            )
            .padding([0, 4])
            .width(Length::Fill),
        );
    }

    let box_el = container(col)
        .padding(Padding {
            top: 8.0,
            bottom: 6.0,
            left: SPACE_3,
            right: SPACE_3,
        })
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::panel())),
            border: Border {
                radius: iced::border::Radius {
                    top_left: RADIUS_SHEET,
                    top_right: RADIUS_SHEET,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
                ..Border::default()
            },
            ..container::Style::default()
        });

    if let Some(msg) = on_press {
        mouse_area(box_el)
            .on_press(msg)
            .interaction(iced::mouse::Interaction::Pointer)
            .into()
    } else {
        box_el.into()
    }
}

// ---------------------------------------------------------------------------
// 15. tooltip
// ---------------------------------------------------------------------------

/// Hover tooltip styled with Next tokens (`bar()`, `rule()`, `text()`).
pub fn tooltip<'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
    position: iced_tooltip::Position,
) -> Element<'a, Message> {
    let content = content.into();
    let label = label.into();
    if label.is_empty() {
        return content;
    }
    let body = container(
        text(label.clone())
            .font(BARLOW_MEDIUM)
            .size(TEXT_12)
            .color(tokens::text()),
    )
    .padding([4, 8])
    .style(|_| container::Style {
        background: Some(Background::Color(tokens::bar())),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
        shadow: tokens::floating_shadow(),
        ..container::Style::default()
    });

    let tip_widget = iced_tooltip(content, body, position)
        .delay(TOOLTIP_DELAY)
        .gap(SPACE_1)
        .padding(0);
    long_press(tip_widget, Message::Ui(crate::ui::Msg::TouchTooltip(label)))
}

// ---------------------------------------------------------------------------
// 16. long_press & context_press (touch press-and-hold, 500 ms / <= 8 px)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct HoldState {
    finger: Option<touch::Finger>,
    origin: Point,
    started: Option<Instant>,
    max_travel: f32,
    fired: bool,
}

struct LongPressArea<'a> {
    child: Element<'a, Message>,
    on_hold: Message,
}

impl Widget<Message, iced::Theme, iced::Renderer> for LongPressArea<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<HoldState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(HoldState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let st = tree.state.downcast_mut::<HoldState>();
        match event {
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                let over = cursor.is_over(layout.bounds())
                    || (cursor == mouse::Cursor::Unavailable
                        && layout.bounds().contains(*position));
                if over {
                    let now = Instant::now();
                    *st = HoldState {
                        finger: Some(*id),
                        origin: *position,
                        started: Some(now),
                        max_travel: 0.0,
                        fired: false,
                    };
                    shell.request_redraw_at(now + LONG_PRESS_DELAY);
                }
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                if st.finger == Some(*id) {
                    let dist = st.origin.distance(*position);
                    st.max_travel = st.max_travel.max(dist);
                }
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let (Some(fid), Some(started)) = (st.finger, st.started) {
                    if !st.fired && st.max_travel <= LONG_PRESS_SLOP_PX {
                        if now.duration_since(started) >= LONG_PRESS_DELAY {
                            st.fired = true;
                            let lost = Event::Touch(touch::Event::FingerLost {
                                id: fid,
                                position: Point::ORIGIN,
                            });
                            self.child.as_widget_mut().update(
                                &mut tree.children[0],
                                &lost,
                                layout,
                                cursor,
                                renderer,
                                clipboard,
                                shell,
                                viewport,
                            );
                            shell.publish(self.on_hold.clone());
                            return;
                        }
                        shell.request_redraw_at(started + LONG_PRESS_DELAY);
                    }
                }
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            Event::Touch(touch::Event::FingerLifted { id, .. }) => {
                if st.finger == Some(*id) {
                    let was_fired = st.fired;
                    *st = HoldState::default();
                    if was_fired {
                        shell.capture_event();
                        return;
                    }
                }
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            Event::Touch(touch::Event::FingerLost { id, .. }) => {
                if st.finger == Some(*id) {
                    *st = HoldState::default();
                }
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            _ => {
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Trigger `on_hold` on a 500 ms touch press within 8 px travel without activating tap.
pub fn long_press<'a>(
    child: impl Into<Element<'a, Message>>,
    on_hold: Message,
) -> Element<'a, Message> {
    Element::new(LongPressArea {
        child: child.into(),
        on_hold,
    })
}

/// Attach both desktop right-press and 500 ms touch press-and-hold to `child`.
pub fn context_press<'a>(
    child: impl Into<Element<'a, Message>>,
    msg: Message,
) -> Element<'a, Message> {
    long_press(mouse_area(child).on_right_press(msg.clone()), msg)
}

struct IsolateOperate<'a> {
    child: Element<'a, Message>,
}

impl Widget<Message, iced::Theme, iced::Renderer> for IsolateOperate<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.child.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        // Isolates Finder's viewport/translation stack so inner Scrollables
        // do not clip selector bounds of sibling Stack layers.
        operation.traverse(&mut |op| {
            self.child
                .as_widget_mut()
                .operate(&mut tree.children[0], layout, renderer, op);
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Wraps a subtree so `widget::Operation` viewport mutations stay local to `child`.
pub fn isolate_operate<'a>(child: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(IsolateOperate {
        child: child.into(),
    })
}

/// Container card helper for P1 gallery sections & panels (`SPACE_4` padding, `RADIUS_DIALOG`).
pub fn panel_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(SPACE_4)
        .style(|_| container::Style {
            background: Some(Background::Color(tokens::panel())),
            border: Border {
                color: tokens::rule(),
                width: 1.0,
                radius: RADIUS_DIALOG.into(),
            },
            ..container::Style::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Token styles for stock iced controls (pick lists and sliders)
// ---------------------------------------------------------------------------

/// Pick list field drawn from the tokens instead of iced's default theme.
pub fn pick_list_style(
    _theme: &iced::Theme,
    _status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    iced::widget::pick_list::Style {
        text_color: tokens::text(),
        placeholder_color: tokens::muted(),
        handle_color: tokens::muted(),
        background: Background::Color(tokens::input_bg()),
        border: Border {
            color: tokens::rule(),
            width: 1.0,
            radius: RADIUS_CONTROL.into(),
        },
    }
}

/// The drop-down list of a pick list, drawn from the tokens.
pub fn pick_menu_style(_theme: &iced::Theme) -> iced::overlay::menu::Style {
    iced::overlay::menu::Style {
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
    }
}

/// Slider with an accent-filled rail and a round accent handle.
pub fn slider_style(
    _theme: &iced::Theme,
    _status: iced::widget::slider::Status,
) -> iced::widget::slider::Style {
    use iced::widget::slider::{Handle, HandleShape, Rail, Style};
    Style {
        rail: Rail {
            backgrounds: (
                Background::Color(tokens::accent()),
                Background::Color(tokens::rule()),
            ),
            width: 4.0,
            border: Border {
                radius: 2.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: Handle {
            shape: HandleShape::Circle { radius: 7.0 },
            background: Background::Color(tokens::accent()),
            border_width: 2.0,
            border_color: tokens::panel(),
        },
    }
}
