//! Native sketch-save picker, isolated so widget tests can choose a disposable
//! destination without opening a Windows dialog on the user's desktop.

use std::path::PathBuf;

pub fn open_sketch() -> Option<PathBuf> {
    #[cfg(not(test))]
    {
        rfd::FileDialog::new()
            .add_filter("Sketch", crate::SKETCH_OPEN_EXTENSIONS)
            .pick_file()
    }
    #[cfg(test)]
    TEST_PICKER.with(|state| {
        let mut state = state.borrow_mut();
        state.open_requests += 1;
        state.open_destination.take()
    })
}

pub fn save_sketch(default_name: &str) -> Option<PathBuf> {
    #[cfg(not(test))]
    {
        rfd::FileDialog::new()
            .add_filter("Sketchy sketch", &["sketchy"])
            .add_filter("Legacy sketch", &["skx", "json"])
            .set_file_name(default_name)
            .save_file()
    }
    #[cfg(test)]
    TEST_PICKER.with(|state| {
        let mut state = state.borrow_mut();
        state.requests.push(default_name.to_owned());
        state.destination.take()
    })
}

#[cfg(test)]
#[derive(Default)]
struct TestPicker {
    destination: Option<PathBuf>,
    requests: Vec<String>,
    open_destination: Option<PathBuf>,
    open_requests: usize,
}

#[cfg(test)]
thread_local! {
    static TEST_PICKER: std::cell::RefCell<TestPicker> = Default::default();
}

#[cfg(test)]
pub fn with_save_path<R>(
    destination: Option<PathBuf>,
    run: impl FnOnce() -> R,
) -> (R, Vec<String>) {
    let (result, picker) = with_picker(
        TestPicker {
            destination,
            ..Default::default()
        },
        run,
    );
    (result, picker.requests)
}

#[cfg(test)]
pub fn with_open_path<R>(destination: Option<PathBuf>, run: impl FnOnce() -> R) -> (R, usize) {
    let (result, picker) = with_picker(
        TestPicker {
            open_destination: destination,
            ..Default::default()
        },
        run,
    );
    (result, picker.open_requests)
}

#[cfg(test)]
fn with_picker<R>(picker: TestPicker, run: impl FnOnce() -> R) -> (R, TestPicker) {
    struct Restore(Option<TestPicker>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_PICKER.with(|state| *state.borrow_mut() = self.0.take().unwrap());
        }
    }
    let _restore = Restore(Some(TEST_PICKER.with(|state| state.replace(picker))));
    let result = run();
    let picker = TEST_PICKER.with(|state| state.take());
    (result, picker)
}
