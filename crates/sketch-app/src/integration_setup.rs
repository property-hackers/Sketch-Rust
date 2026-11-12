//! Desktop host registration UI. Setup uses the same EXE and backend as the CLI.
use iced::widget::{button, column, container, mouse_area, opaque, row, scrollable, text};
use iced::{Background, Border, Color, Element, Length, Task};

use crate::{theme::*, Message};

#[derive(Debug, Clone)]
pub enum Msg {
    Open,
    Close,
    Enable,
    Restore,
    Finished(Result<String, String>),
}

#[derive(Default)]
pub struct Setup {
    pub open: bool,
    pub busy: bool,
    pub target: String,
    pub status: String,
    pub result: Option<Result<String, String>>,
}

impl Setup {
    pub fn update(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::Open => {
                self.open = true;
                self.target = std::env::current_exe()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|e| e.to_string());
                self.refresh_status();
            }
            Msg::Close => {
                if !self.busy {
                    self.open = false;
                }
            }
            Msg::Enable | Msg::Restore if !self.busy => {
                self.busy = true;
                self.result = None;
                return register(matches!(msg, Msg::Enable));
            }
            Msg::Finished(result) => {
                self.busy = false;
                self.result = Some(result);
                self.refresh_status();
            }
            _ => {}
        }
        Task::none()
    }

    fn refresh_status(&mut self) {
        #[cfg(windows)]
        {
            self.status = match crate::apex_registration::launch_path() {
                Ok(Some(path)) if path.to_string_lossy() == self.target => {
                    "Enabled for this copy of Sketch.".into()
                }
                Ok(Some(path)) => format!("Current host target: {}", path.display()),
                Ok(None) => "No Apex-compatible host launch is registered.".into(),
                Err(e) => format!("Could not read the host registration: {e}"),
            };
            if !cfg!(apex_component) {
                self.status.push_str("\nThis build does not include Apex compatibility.");
            }
        }
        #[cfg(not(windows))]
        {
            self.status = "Desktop integration setup requires Windows.".into();
        }
    }
}

fn register(enable: bool) -> Task<Message> {
    #[cfg(test)]
    let job = {
        // Never change the user's actual host registration from a GUI test.
        let result = TEST_REGISTRATION.with(|s| {
            let mut s = s.borrow_mut();
            s.0.push(enable);
            s.1.take()
                .unwrap_or_else(|| Err("No setup result staged for this test.".into()))
        });
        move || result
    };
    #[cfg(not(test))]
    let job = move || {
        #[cfg(windows)]
        {
            crate::apex_registration::configure(enable)
        }
        #[cfg(not(windows))]
        {
            let _ = enable;
            Err("Desktop integration setup requires Windows.".into())
        }
    };
    // Iced uses its thread-pool executor, not a Tokio runtime. Keep UAC and
    // filesystem work on a dedicated thread and return its result asynchronously.
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(job());
    });
    Task::perform(
        async move {
            receiver.await.unwrap_or_else(|_| {
                Err("Integration setup stopped before returning a result.".into())
            })
        },
        |result| Message::IntegrationSetup(Msg::Finished(result)),
    )
}

#[cfg(test)]
type TestRegistration = (Vec<bool>, Option<Result<String, String>>);
#[cfg(test)]
thread_local! { static TEST_REGISTRATION: std::cell::RefCell<TestRegistration> = const { std::cell::RefCell::new((Vec::new(), None)) }; }
#[cfg(test)]
pub(super) fn with_result<R>(
    result: Result<String, String>,
    run: impl FnOnce() -> R,
) -> (R, Vec<bool>) {
    struct Restore(TestRegistration);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_REGISTRATION.with(|s| {
                s.replace(std::mem::take(&mut self.0));
            });
        }
    }
    let _restore = Restore(TEST_REGISTRATION.with(|s| s.replace((Vec::new(), Some(result)))));
    let value = run();
    (value, TEST_REGISTRATION.with(|s| s.borrow().0.clone()))
}

pub fn layer(setup: &Setup, size: iced::Size) -> Option<Element<'_, Message>> {
    if !setup.open {
        return None;
    }
    let action = |label: &'static str, msg: Msg| {
        let b = button(text(label).font(SEGOE).size(13)).padding([8, 12]);
        if setup.busy {
            b
        } else {
            b.on_press(Message::IntegrationSetup(msg))
        }
    };
    let mut content = column![
        text("Appraise-It Pro integration").font(SEGOE).size(20).color(text_main()),
        text("Use Appraise-It Pro's existing Apex button to open Sketch and transfer measurements and sketch images back to the report.").font(SEGOE).size(13),
        text("Keep this copy of Sketch in its permanent folder before enabling integration:").font(SEGOE).size(12).color(muted()),
        text(&setup.target).font(SEGOE).size(12).wrapping(iced::widget::text::Wrapping::Glyph),
        text(&setup.status).font(SEGOE).size(12).wrapping(iced::widget::text::Wrapping::Glyph),
        text("Setup updates the shared Apex launch registration. It does not change Appraise-It Pro's program files. Windows may ask for administrator permission.").font(SEGOE).size(12).color(muted()),
    ].spacing(12);
    if setup.busy {
        content = content.push(
            text("Applying setup. Complete the Windows administrator prompt if it appears.")
                .font(SEGOE)
                .size(13),
        );
    }
    if let Some(result) = &setup.result {
        let (value, color) = match result {
            Ok(value) => (
                format!("{value}\nRestart Appraise-It Pro to use the updated launch setting."),
                text_main(),
            ),
            Err(value) => (format!("Setup was not completed: {value}"), hex(0xfc8181)),
        };
        content = content.push(text(value).font(SEGOE).size(13).color(color));
    }
    // An Apex-less build cannot enable integration but can still restore.
    let can_enable = !cfg!(all(windows, not(apex_component)));
    let actions: Element<'_, Message> = if size.width < 650.0 {
        let mut col = column![].spacing(8);
        if can_enable {
            col = col.push(action("Enable integration", Msg::Enable));
        }
        col.push(action("Restore previous provider", Msg::Restore))
            .push(action("Close", Msg::Close))
            .into()
    } else {
        let mut r = row![].spacing(8);
        if can_enable {
            r = r.push(action("Enable integration", Msg::Enable));
        }
        r.push(action("Restore previous provider", Msg::Restore))
            .push(action("Close", Msg::Close))
            .into()
    };
    content = content.push(actions).width(Length::Fill);
    let card = container(
        scrollable(content)
            .width(Length::Fill)
            .height(Length::Shrink),
    )
    .padding(20)
    .width(620.0_f32.min((size.width - 32.0).max(300.0)))
    .max_height((size.height - 32.0).max(160.0))
    .style(|_| container::Style {
        background: Some(Background::Color(bg_panel())),
        text_color: Some(text_main()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    });
    let backdrop = container(
        container(opaque(mouse_area(card)))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.46).into()),
        ..Default::default()
    });
    Some(opaque(
        mouse_area(backdrop).on_press(Message::IntegrationSetup(Msg::Close)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integration_setup_pending_job_blocks_duplicates_and_dismissal_until_result() {
        let mut setup = Setup {
            open: true,
            ..Default::default()
        };
        let (_, requests) = with_result(Ok("done".into()), || {
            let _ = setup.update(Msg::Enable);
            assert!(setup.busy);
            let _ = setup.update(Msg::Restore);
            let _ = setup.update(Msg::Enable);
            let _ = setup.update(Msg::Close);
            assert!(setup.open);
        });
        assert_eq!(requests, [true]);
        let _ = setup.update(Msg::Finished(Err("Permission cancelled".into())));
        assert!(!setup.busy);
        assert_eq!(setup.result, Some(Err("Permission cancelled".into())));
        let _ = setup.update(Msg::Close);
        assert!(!setup.open);
    }
}
