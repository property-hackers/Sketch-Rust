//! Timed, atomic recovery snapshots, independent of manual Save.
use crate::{persist, App, Message};
use iced::{Subscription, Task};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Debug)]
pub struct State {
    pub last: Instant,
    pub hash: String,
    pub path: Option<PathBuf>,
    pub recovered: Option<PathBuf>,
    pub recovery: Option<(PathBuf, serde_json::Value)>,
    /// The bound `.sketchy` store's path and document id, read once per path for the
    /// snapshot's `recoverySource`.
    pub source: Option<(PathBuf, String)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            last: Instant::now(),
            hash: String::new(),
            path: None,
            recovered: None,
            recovery: None,
            source: None,
        }
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
            .map_err(|e| std::io::Error::new(e.kind(), format!("Flush snapshot: {e}")))?;
    }
    // std rename replaces the destination atomically on Windows and Unix.
    std::fs::rename(&tmp, path)
        .map_err(|e| std::io::Error::new(e.kind(), format!("Replace snapshot: {e}")))
}
fn signature(model: &serde_json::Value) -> String {
    let mut v = model.clone();
    if let Some(m) = v.as_object_mut() {
        m.remove("savedAt");
    }
    sketch_core::skx::sha256_hex(v.to_string().as_bytes())
}
/// `signature` for the unsaved-changes check: the save model also carries
/// interface state (which dock tab is open, the canvas theme) that rides
/// along in the file but is not the user's work, so flipping it must not
/// read as an edit.
fn content_signature(model: &serde_json::Value) -> String {
    let mut v = model.clone();
    if let Some(settings) = v.get_mut("settings").and_then(|s| s.as_object_mut()) {
        settings.remove("sidebar");
        settings.remove("canvasTheme");
    }
    signature(&v)
}
pub fn latest() -> Option<(PathBuf, serde_json::Value)> {
    if !persist::enabled() {
        return None;
    }
    let dir = persist::path("autosave")?;
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "skx"))
        .collect();
    paths.sort_by_key(|e| {
        std::cmp::Reverse(
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH),
        )
    });
    for e in paths {
        if let Ok(bytes) = std::fs::read(e.path()) {
            if let Ok(v) = sketch_core::skx::read(&bytes) {
                if v.model
                    .get("pages")
                    .and_then(|v| v.as_array())
                    .is_some_and(|p| !p.is_empty())
                {
                    return Some((e.path(), v.model));
                }
            }
        }
    }
    None
}
pub fn subscription(enabled: bool) -> Subscription<Message> {
    if !enabled || !persist::enabled() {
        return Subscription::none();
    }
    Subscription::run(ticks).map(|_| Message::AutoSaveTick)
}
fn ticks() -> impl iced::futures::Stream<Item = ()> {
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        while !tx.is_closed() {
            std::thread::sleep(Duration::from_secs(15));
            if tx.unbounded_send(()).is_err() {
                break;
            }
        }
    });
    rx
}
impl App {
    pub(crate) fn autosave_baseline(&mut self) {
        let model = self.build_save_model();
        self.autosave.hash = signature(&model);
        self.autosave.last = Instant::now();
        // Every baseline is a "this is what is on disk" moment (new, open,
        // save), so the unsaved-changes tracker restarts from it too.
        self.saved_hash = content_signature(&model);
        self.dirty = false;
        self.dirty_revision = self.undo.revision();
    }
    /// Re-reads whether the document differs from what was last saved or
    /// opened. Exact (a hash of the save model), so an edit that is undone
    /// reads as clean again.
    pub(crate) fn refresh_dirty(&mut self) {
        let started = Instant::now();
        let model = self.build_save_model();
        self.dirty = content_signature(&model) != self.saved_hash;
        self.dirty_revision = self.undo.revision();
        self.dirty_checked = Instant::now();
        self.dirty_cost = started.elapsed();
    }
    /// `refresh_dirty`, but only when an edit went through the undo stack
    /// or the last look is stale. The stale interval grows with the cost of
    /// hashing, so a large sketch is not hashed on every message.
    pub(crate) fn refresh_dirty_if_due(&mut self) {
        let stale = (self.dirty_cost * 20).max(Duration::from_millis(750));
        if self.undo.revision() != self.dirty_revision || self.dirty_checked.elapsed() >= stale {
            self.refresh_dirty();
        }
    }
    pub(crate) fn autosave_boot(&mut self) {
        self.autosave_baseline();
        self.autosave.recovery = latest();
        if self.autosave.recovery.is_some() {
            self.autosave_prompt = true;
        }
    }
    pub(crate) fn autosave_tick(&mut self, force: bool) -> Task<Message> {
        if !persist::enabled()
            || !self.drawing.options.auto_save_enabled
            || self.viewing_revision.is_some()
        {
            return Task::none();
        }
        let minutes = self.drawing.options.auto_save_interval;
        let minutes = if minutes.is_finite() {
            minutes.clamp(1., 120.)
        } else {
            5.
        };
        if !force && self.autosave.last.elapsed().as_secs_f64() < minutes * 60. {
            return Task::none();
        }
        self.autosave.last = Instant::now();
        let model = self.build_save_model();
        let hash = signature(&model);
        if hash == self.autosave.hash {
            return Task::none();
        }
        let Some(dir) = persist::path("autosave") else {
            return Task::none();
        };
        let path = self.autosave.path.clone().unwrap_or_else(|| {
            dir.join(format!(
                "recovery-{}-{}.skx",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ))
        });
        let mut snapshot = model;
        if let Some(source) = self.recovery_source() {
            snapshot["recoverySource"] = source;
        }
        let result = sketch_core::skx::write(&snapshot, &Default::default())
            .map_err(|e| e.to_string())
            .and_then(|bytes| atomic_write(&path, &bytes).map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                self.autosave.path = Some(path);
                self.autosave.hash = hash;
                Task::none()
            }
            Err(e) => self.toasts.show(format!("Autosave failed: {e}")),
        }
    }
    /// Where the open document's `.sketchy` store is, for the recovery snapshot:
    /// `{path, documentId}`, so recovery can bind the document back to it.
    fn recovery_source(&mut self) -> Option<serde_json::Value> {
        let file = self
            .current_file
            .as_ref()
            .filter(|f| f.format == crate::SketchFileFormat::Sketchy)?;
        if self.autosave.source.as_ref().map(|(path, _)| path) != Some(&file.path) {
            let store = sketch_core::sketchy::store::Store::open(&file.path).ok()?;
            self.autosave.source = Some((file.path.clone(), store.document_id().to_string()));
        }
        let (path, id) = self.autosave.source.as_ref()?;
        Some(serde_json::json!({ "path": path.to_string_lossy(), "documentId": id }))
    }
    /// Binds a recovered document back to the store its snapshot came from, when that
    /// store is still there and still the same document. Then Save appends to it.
    fn rebind_recovered_source(&mut self, source: &serde_json::Value) -> Option<()> {
        let path = PathBuf::from(source.get("path")?.as_str()?);
        let store = sketch_core::sketchy::store::Store::open(&path).ok()?;
        if store.document_id() != source.get("documentId")?.as_str()? {
            return None;
        }
        self.current_file = Some(crate::CurrentFile {
            path,
            format: crate::SketchFileFormat::Sketchy,
            read_only: store.is_read_only(),
            confirmed_legacy: false,
        });
        self.refresh_versions();
        Some(())
    }
    pub(crate) fn autosave_clean(&mut self) {
        if let Some(path) = self.autosave.path.take() {
            let _ = std::fs::remove_file(path);
        }
        if let Some(path) = self.autosave.recovered.take() {
            let _ = std::fs::remove_file(&path);
            if self
                .autosave
                .recovery
                .as_ref()
                .is_some_and(|(p, _)| p == &path)
            {
                self.autosave.recovery = None;
            }
        }
        self.autosave_baseline();
    }
    pub(crate) fn recover_autosave(&mut self, accept: bool) -> Task<Message> {
        self.autosave_prompt = false;
        if accept {
            if let Some((path, mut model)) = self.autosave.recovery.clone() {
                let source = model.as_object_mut().and_then(|m| m.remove("recoverySource"));
                let task = self.apply_model(&model, "Recovered sketch");
                if let Some(source) = source {
                    let _ = self.rebind_recovered_source(&source);
                }
                self.autosave.recovered = Some(path);
                self.autosave_baseline();
                // Recovered work exists only in the recovery file until it
                // is saved by hand.
                self.saved_hash.clear();
                self.dirty = true;
                return task;
            }
        }
        Task::none()
    }
    pub(crate) fn recovery_layer(&self) -> iced::Element<'_, Message> {
        use iced::widget::{button, column, container, opaque, row, text};
        use iced::Length;
        if !self.autosave_prompt {
            return iced::widget::Space::new().into();
        }
        let title = self
            .autosave
            .recovery
            .as_ref()
            .and_then(|(_, m)| m.get("fileName"))
            .and_then(|v| v.as_str())
            .unwrap_or("sketch");
        let card = container(
            column![
                text(format!("Recover autosaved sketch: {title}?")),
                text("Recovery stays available until you recover and manually save it.").size(12),
                row![
                    button("Recover").on_press(Message::RecoverAutosave(true)),
                    button("Later").on_press(Message::RecoverAutosave(false))
                ]
                .spacing(12)
            ]
            .spacing(16)
            .padding(24),
        )
        .style(|_| container::Style {
            background: Some(crate::theme::bg_panel().into()),
            text_color: Some(crate::theme::text_main()),
            ..Default::default()
        });
        opaque(
            container(opaque(card))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Color::from_rgba(0., 0., 0., 0.5).into()),
                    ..Default::default()
                }),
        )
    }
}
