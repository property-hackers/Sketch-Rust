//! Version history for `.sketchy` stores: the dock's Versions panel, the
//! read-only "viewing an old version" state with its banner, and the actions
//! that go with them (open, restore, save a copy, mark as final).
//!
//! Nothing here deletes or rewrites a revision: restoring commits the old
//! snapshot as a new draft on top of the head, and a copy is a new store.

use iced::widget::{column, container, row, text, Space};
use iced::{alignment, Border, Element, Length, Task};
use sketch_core::sketchy::convert::upgrade;
use sketch_core::sketchy::store::{CommitInfo, CommitOutcome, RevisionStatus, RevisionSummary, Store};

use crate::dock::{msg_action_btn, scroll};
use crate::theme::*;
use crate::{dialog, localtime, App, Message, SketchFileFormat};

/// What the Versions panel, the banner and the File menu ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionsMsg {
    /// File > Version History...: show the panel.
    Show,
    /// Mark as Final...: ask for a note, then commit a final revision.
    MarkFinal,
    /// Open a revision read-only (by revision id).
    View(String),
    /// Save a copy of a revision as a new `.sketchy` file (by revision id).
    SaveCopy(String),
    /// The banner's Restore this version.
    RestoreViewed,
    /// The banner's Save a copy...
    SaveCopyViewed,
    /// The banner's Back to latest.
    BackToLatest,
}

/// One row of the Versions panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    pub id: String,
    pub sequence: u64,
    /// Local date and time, ready to show.
    pub when: String,
    pub author: Option<String>,
    pub message: Option<String>,
    pub is_final: bool,
    pub is_head: bool,
}

/// The old revision on screen, read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewingRevision {
    pub id: String,
    pub sequence: u64,
    pub is_final: bool,
    /// Local date and time, ready to show.
    pub when: String,
    /// The read-only toast was already shown for this viewing session.
    pub told: bool,
}

/// Panel rows, newest first, from a store's oldest-first history.
pub fn rows_from_history(history: &[RevisionSummary]) -> Vec<VersionRow> {
    let head = history.last().map(|r| r.id.as_str());
    history
        .iter()
        .rev()
        .map(|r| VersionRow {
            id: r.id.clone(),
            sequence: r.sequence,
            when: localtime::display(&r.created_at),
            author: r.author.clone().filter(|a| !a.trim().is_empty()),
            message: r.message.clone().filter(|m| !m.trim().is_empty()),
            is_final: r.status == RevisionStatus::Final,
            is_head: Some(r.id.as_str()) == head,
        })
        .collect()
}

/// The refusal shown when an edit is attempted on an old version.
const READ_ONLY_TOAST: &str =
    "Viewing a past version (read-only). Restore it or go back to the latest to edit.";

/// The refusal for a write to a store that opened read-only.
const READ_ONLY_FILE_TOAST: &str = "This file is read-only.";

/// Whether `message` changes the document (or starts a change). Anything that
/// slips past this list is caught by the dirty check in `App::update`.
pub fn edits_document(message: &Message) -> bool {
    use crate::input::KeyCmd as K;
    use crate::options_dialog::OptMsg as O;
    use crate::popup_menu::PopupMsg as P;
    use crate::statusbar::StatusMsg as S;
    use Message as M;
    match message {
        M::Key(key) => matches!(
            key,
            K::Undo
                | K::Redo
                | K::Cut
                | K::Paste
                | K::Delete
                | K::DrawExterior
                | K::DrawInterior
                | K::RoomShape
                | K::OrientH
                | K::OrientV
                | K::CurveMode
                | K::AutoClose
                | K::BayMirror
                | K::InsertVertex
                | K::RotateSelected(_)
                | K::ScaleSelected(_)
                | K::FlipSketch(_)
                | K::RotateSketch(_)
                | K::Nudge(..)
                | K::Buffer(_)
                | K::DecimalPoint
                | K::BufferBackspace
                | K::Enter
        ),
        M::CanvasPress(..)
        | M::CanvasDrag(..)
        | M::CanvasDragEnd
        | M::CanvasClick { .. }
        | M::DockDragPress(_)
        | M::DockDragRelease
        | M::DockItemRelease(_)
        | M::Status(S::PageAdd | S::PageDelete)
        | M::PopupMenu(P::Flip(_))
        | M::AreaProps
        | M::CurveSelected
        | M::Color(_)
        | M::OpenColorDialog
        | M::LineColorClicked
        | M::TextColorClicked
        | M::AreaColorClicked(_)
        | M::WallSolids(_)
        | M::DefineFirst(_)
        | M::AreaType(..)
        | M::Options(O::Ok | O::RestoreDefaults)
        | M::PhotoScaleStart
        | M::PhotoSmaller
        | M::PhotoLarger
        | M::PhotoRotateLeft
        | M::PhotoRotateRight
        | M::PhotoOrientStart
        | M::PhotoOrientApply(_)
        | M::PhotoFlipH
        | M::PhotoFlipV
        | M::PhotoIntensity(_)
        | M::Disto(_)
        | M::RoomShapePlace
        | M::RoomShapePlaceAt(..)
        | M::RotateSketch
        | M::RotateSketchBy(_)
        | M::RotateSelected(_)
        | M::ScaleSelected(_)
        | M::LayerDrop(..)
        | M::LayerToggleVisible(_)
        | M::LayerToggleLocked(_)
        | M::LayerStartRename(_)
        | M::LayerRename(..)
        | M::LayerRenameCommit
        | M::LayerSendToBack
        | M::LayerBringToFront
        | M::ImportSketch
        | M::ImportLayer
        | M::ImportCad
        | M::PhotoLoad
        | M::PhotoRemove
        | M::PhotoScaleClick(_)
        | M::PhotoOrientClick(_)
        | M::ApplyLineLength
        | M::FlipOpening
        | M::OpeningWider
        | M::OpeningNarrower
        | M::SetSelectedTextStyle(..)
        | M::ToggleSelectedTextStyle(_)
        | M::AdjustSelectedTextSize(_)
        | M::CycleLineStyle
        | M::TextFontPicked(_)
        | M::TextFieldApply(_)
        | M::AreaCycle(_)
        | M::Undo
        | M::Redo
        | M::UndoTo(_)
        | M::Cut
        | M::Paste
        | M::Delete
        | M::ContextAction(_)
        | M::AreaDialog(_)
        | M::AreaAttributes(_)
        | M::AreaWorkflow(_)
        | M::DefEditor(_)
        | M::NewCode(_)
        | M::CodesEditor(_)
        | M::Shapes(_)
        | M::SubjectInfo(_)
        | M::TouchAction(_) => true,
        _ => false,
    }
}

/// `CommitInfo` for a revision this app writes, with a message and status.
fn info(message: Option<String>, status: RevisionStatus) -> CommitInfo {
    CommitInfo {
        message,
        status,
        ..crate::sketchy_commit_info()
    }
}

impl App {
    /// The bound `.sketchy` store, when the current file is one.
    fn store_file(&self) -> Option<crate::CurrentFile> {
        self.current_file
            .clone()
            .filter(|f| f.format == SketchFileFormat::Sketchy)
    }

    /// Re-read the store's history into the dock (the tab shows only for a
    /// `.sketchy` store). A store that cannot be read hides the tab.
    pub(crate) fn refresh_versions(&mut self) {
        self.dock.versions = self.store_file().and_then(|file| {
            let store = Store::open(&file.path).ok()?;
            Some(rows_from_history(&store.history().ok()?))
        });
        self.fix_versions_tab();
    }

    /// The document was replaced by something that is not a store revision.
    pub(crate) fn clear_versions(&mut self) {
        self.viewing_revision = None;
        self.dock.versions = None;
        self.fix_versions_tab();
    }

    /// The Versions tab cannot stay active once it is hidden.
    fn fix_versions_tab(&mut self) {
        if self.dock.versions.is_none() && self.dock.active == crate::dock::DockTab::Versions {
            self.dock.active = crate::dock::DockTab::Areas;
        }
    }

    /// The head's `(sequence, is_final)` for the title-bar pill.
    fn head_version(&self) -> Option<(u64, bool)> {
        self.store_file()?;
        let head = self.dock.versions.as_ref()?.first()?;
        Some((head.sequence, head.is_final))
    }

    /// The title bar pill: where the document stands against its file.
    pub(crate) fn save_state(&self) -> String {
        if let Some(viewing) = &self.viewing_revision {
            return format!("Viewing v{}", viewing.sequence);
        }
        let read_only = self.current_file.as_ref().is_some_and(|f| f.read_only);
        let sketchy_source = self
            .current_file
            .as_ref()
            .is_some_and(|f| f.format == SketchFileFormat::SketchyJson);
        // A store opened read-only, untouched, is not "unsaved": there is nothing to lose yet.
        let untouched_read_only = read_only && !self.dirty;
        let unsaved = (self.dirty || sketchy_source || read_only) && !untouched_read_only;
        let base = if untouched_read_only {
            "Read-only"
        } else if unsaved {
            "Unsaved"
        } else if self.current_file.is_some() {
            "Saved"
        } else {
            return "New document".into();
        };
        match self.head_version() {
            Some((sequence, true)) if !unsaved && !untouched_read_only => format!("Final \u{b7} v{sequence}"),
            Some((sequence, _)) => format!("{base} \u{b7} v{sequence}"),
            None => base.into(),
        }
    }

    /// Whether the pill reads "Final" (drawn green).
    pub(crate) fn save_is_final(&self) -> bool {
        self.viewing_revision.is_none()
            && !self.dirty
            && self.head_version().is_some_and(|(_, is_final)| is_final)
            && self.current_file.as_ref().is_some_and(|f| !f.read_only)
    }

    /// Whether File > Version History... is available.
    pub(crate) fn history_available(&self) -> bool {
        self.store_file().is_some()
    }

    /// The gate in front of `update` while an old version is on screen.
    /// Returns a toast task when the message was refused.
    pub(crate) fn refuse_edit_while_viewing(&mut self, message: &Message) -> Option<Task<Message>> {
        if self.viewing_revision.is_none() || !edits_document(message) {
            return None;
        }
        // Canvas gestures (selecting, panning-by-click) stay quiet, and the
        // reason is given once per viewing session.
        let quiet = matches!(
            message,
            Message::CanvasPress(..)
                | Message::CanvasDrag(..)
                | Message::CanvasDragEnd
                | Message::CanvasClick { .. }
        );
        let told = self.viewing_revision.as_ref().is_some_and(|v| v.told);
        if quiet || told {
            return Some(Task::none());
        }
        if let Some(viewing) = self.viewing_revision.as_mut() {
            viewing.told = true;
        }
        Some(self.toasts.show(READ_ONLY_TOAST.into()))
    }

    /// The net under the gate: an edit that got through is thrown away by
    /// reloading the revision on screen.
    pub(crate) fn revert_edits_while_viewing(&mut self) -> Option<Task<Message>> {
        let viewing = self.viewing_revision.clone()?;
        if !self.dirty {
            return None;
        }
        let reload = self.view_revision_unchecked(&viewing.id);
        Some(Task::batch([reload, self.toasts.show(READ_ONLY_TOAST.into())]))
    }

    pub(crate) fn versions_update(&mut self, msg: VersionsMsg) -> Task<Message> {
        match msg {
            VersionsMsg::Show => self.show_versions(),
            VersionsMsg::MarkFinal => self.ask_final_note(),
            VersionsMsg::View(id) => self.view_revision(&id),
            VersionsMsg::SaveCopy(id) => self.save_copy(&id),
            VersionsMsg::SaveCopyViewed => match self.viewing_revision.clone() {
                Some(viewing) => self.save_copy(&viewing.id),
                None => Task::none(),
            },
            VersionsMsg::RestoreViewed => self.restore_viewed(),
            VersionsMsg::BackToLatest => self.back_to_latest(),
        }
    }

    fn show_versions(&mut self) -> Task<Message> {
        self.close_file_menu();
        self.refresh_versions();
        if self.dock.versions.is_none() {
            return self.toasts.show(
                "Version history is kept in .sketchy files. Save this sketch as a .sketchy file first.".into(),
            );
        }
        self.dock.update(crate::dock::DockMsg::Show(crate::dock::DockTab::Versions));
        Task::none()
    }

    /// Mark as Final...: the optional note comes from a prompt.
    fn ask_final_note(&mut self) -> Task<Message> {
        let Some(file) = self.store_file() else {
            return self.toasts.show("Marking a version as final needs a .sketchy file.".into());
        };
        if self.viewing_revision.is_some() {
            return self.toasts.show("Go back to the latest version to mark it as final.".into());
        }
        if file.read_only {
            return self.toasts.show(READ_ONLY_FILE_TOAST.into());
        }
        let mut prompt = dialog::Dialog::prompt(
            "Add a note for this final version (optional):",
            "",
            dialog::Action::MarkFinal,
        );
        prompt.title = "Mark as Final".into();
        self.open_dialog(prompt)
    }

    /// The prompt's answer: commit the current document as a final revision.
    pub(crate) fn mark_final(&mut self, note: &str) -> Task<Message> {
        let Some(file) = self.store_file() else {
            return Task::none();
        };
        let note = Some(note.trim().to_string()).filter(|n| !n.is_empty());
        let mut model = self.build_save_model();
        model["fileName"] = self.file_name.clone().into();
        let info = info(note, RevisionStatus::Final);
        let written = match crate::write_store_with(&model, &file.path, Some(&file.path), &info) {
            Ok(written) => written,
            Err(e) => return self.toasts.show(crate::toast_error(&format!("Could not mark as final: {e}"))),
        };
        self.autosave_clean();
        self.refresh_versions();
        if written == crate::StoreWrite::HistoryLost {
            return self.toasts.show(crate::HISTORY_LOST_TOAST.into());
        }
        let version = self.head_version().map_or(String::new(), |(s, _)| format!(" v{s}"));
        self.toasts.show(format!("Marked{version} as final."))
    }

    /// Open a revision read-only. Unsaved edits would be lost by swapping the
    /// document, so they must be saved first.
    fn view_revision(&mut self, id: &str) -> Task<Message> {
        if self.store_file().is_none() {
            return self.toasts.show("Version history needs a .sketchy file.".into());
        }
        if self.viewing_revision.is_none() {
            self.refresh_dirty();
            if self.dirty {
                return self.toasts.show("Save your changes before viewing another version.".into());
            }
        }
        self.view_revision_unchecked(id)
    }

    fn view_revision_unchecked(&mut self, id: &str) -> Task<Message> {
        let Some(file) = self.store_file() else {
            return Task::none();
        };
        let loaded = Store::open(&file.path).and_then(|store| {
            let history = store.history()?;
            let doc = store.revision(id)?;
            Ok((history, doc))
        });
        let (history, doc) = match loaded {
            Ok(pair) => pair,
            Err(e) => return self.toasts.show(crate::toast_error(&format!("Could not open that version: {e}"))),
        };
        let Some(summary) = history.iter().find(|r| r.id == id) else {
            return self.toasts.show("That version no longer exists.".into());
        };
        if history.last().is_some_and(|head| head.id == id) {
            return self.back_to_latest();
        }
        let viewing = ViewingRevision {
            id: summary.id.clone(),
            sequence: summary.sequence,
            is_final: summary.status == RevisionStatus::Final,
            when: localtime::display(&summary.created_at),
            told: false,
        };
        let model = match sketch_core::sketchy::convert::from_sketchy(&doc) {
            Ok(model) => model,
            Err(e) => return self.toasts.show(crate::toast_error(&format!("Could not open that version: {e}"))),
        };
        let task = self.load_revision_model(&model, file);
        self.viewing_revision = Some(viewing);
        self.dock.versions = Some(rows_from_history(&history));
        task
    }

    /// Replace the document with a revision's model, keeping the file binding
    /// and the dock layout (a revision's saved settings must not move panels).
    fn load_revision_model(&mut self, model: &serde_json::Value, file: crate::CurrentFile) -> Task<Message> {
        let stem = file
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        let stem = stem.strip_suffix(".sketchy").unwrap_or(&stem).to_owned();
        let (active, open, pinned) = (self.dock.active, self.dock.open, self.dock.pinned);
        let task = self.apply_model(model, &stem);
        self.dock.active = active;
        self.dock.open = open;
        self.dock.pinned = pinned;
        self.current_file = Some(file);
        task
    }

    /// Reload the head and leave the read-only view.
    fn back_to_latest(&mut self) -> Task<Message> {
        let Some(file) = self.store_file() else {
            return Task::none();
        };
        let head = Store::open(&file.path)
            .and_then(|store| store.head())
            .map_err(|e| e.to_string())
            .and_then(|doc| sketch_core::sketchy::convert::from_sketchy(&doc).map_err(|e| e.to_string()));
        let model = match head {
            Ok(model) => model,
            Err(e) => return self.toasts.show(crate::toast_error(&format!("Could not open the latest version: {e}"))),
        };
        let task = self.load_revision_model(&model, file);
        self.refresh_versions();
        task
    }

    /// Commit the viewed snapshot as a new draft on top of the head.
    fn restore_viewed(&mut self) -> Task<Message> {
        let (Some(file), Some(viewing)) = (self.store_file(), self.viewing_revision.clone()) else {
            return Task::none();
        };
        if file.read_only {
            return self.toasts.show(READ_ONLY_FILE_TOAST.into());
        }
        let committed = Store::open(&file.path).and_then(|mut store| {
            // An old revision may be 0.1: the new head is written as the current format.
            let doc = upgrade(&store.revision(&viewing.id)?)?;
            let note = format!("Restored from v{}", viewing.sequence);
            store.commit(doc, &info(Some(note), RevisionStatus::Draft))
        });
        let outcome = match committed {
            Ok(outcome) => outcome,
            Err(e) => return self.toasts.show(crate::toast_error(&format!("Could not restore: {e}"))),
        };
        let task = self.back_to_latest();
        let said = match outcome {
            CommitOutcome::Created(_) => format!("Restored v{} as the latest version.", viewing.sequence),
            CommitOutcome::Unchanged(_) => format!("v{} is already the latest version.", viewing.sequence),
        };
        Task::batch([task, self.toasts.show(said)])
    }

    /// Save a copy of revision `id` as a new `.sketchy` store: a new document
    /// whose first revision is that snapshot, derived from this one.
    fn save_copy(&mut self, id: &str) -> Task<Message> {
        let Some(file) = self.store_file() else {
            return Task::none();
        };
        let base = self.file_name.strip_suffix(".sketchy").unwrap_or(&self.file_name).to_owned();
        let Some(chosen) = crate::file_dialogs::save_sketch(&format!("{base}.sketchy")) else {
            return Task::none();
        };
        let target = match self.save_target(chosen) {
            Ok(target) if target.format == SketchFileFormat::Sketchy => target,
            Ok(_) => return self.toasts.show("A copy of a version is saved as a .sketchy file.".into()),
            Err(toast) => return self.toasts.show(toast),
        };
        if crate::same_file(&target.path, &file.path) {
            return self.toasts.show("Choose a different file for the copy.".into());
        }
        match write_copy(&file.path, id, &target.path) {
            Ok(()) => {
                let name = target.path.file_name().and_then(|n| n.to_str()).unwrap_or("copy").to_owned();
                self.toasts.show(format!("Saved a copy: {name}"))
            }
            Err(e) => self.toasts.show(crate::toast_error(&format!("Could not save the copy: {e}"))),
        }
    }
}

/// Write revision `id` of the store at `source` as the first revision of a
/// new store at `target`, built beside it and moved into place.
fn write_copy(source: &std::path::Path, id: &str, target: &std::path::Path) -> Result<(), String> {
    use sketch_core::sketchy::store::new_id;
    let es = |e: sketch_core::sketchy::SketchyError| e.to_string();
    if crate::same_file(source, target) {
        return Err("the copy would replace the original file".into());
    }
    let store = Store::open(source).map_err(es)?;
    let sequence = store
        .history()
        .map_err(es)?
        .into_iter()
        .find(|r| r.id == id)
        .map(|r| r.sequence)
        .ok_or("that version no longer exists")?;
    // The first revision of a new store is written as the current format, whatever the old one was.
    let mut doc = upgrade(&store.revision(id).map_err(es)?).map_err(es)?;
    doc.id = new_id();
    doc.derived_from = Some(serde_json::json!({ "document": store.document_id(), "revision": id }));
    doc.revision = None;
    doc.history = None;
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("copy");
    let temp = target.with_file_name(format!(".{name}.{}.tmp", new_id()));
    let made = Store::create(&temp, doc, &info(Some(format!("Copied from v{sequence}")), RevisionStatus::Draft))
        .map(drop)
        .map_err(es)
        .and_then(|()| std::fs::rename(&temp, target).map_err(|e| e.to_string()));
    if made.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    made
}

/// `.badge` for a final revision.
fn final_badge<'a>() -> Element<'a, Message> {
    let green = hex(0x5dae84);
    container(label("FINAL", 10.0, ARIAL_BOLD, green, 12.0))
        .padding([1, 6])
        .style(move |_| container::Style {
            border: Border { color: green, width: 1.0, radius: 3.0.into() },
            ..container::Style::default()
        })
        .into()
}

fn version_row<'a>(r: &'a VersionRow, on_screen: bool) -> Element<'a, Message> {
    let mut title = row![label(&format!("v{}", r.sequence), 13.0, ARIAL_BOLD, text_main(), 15.0)]
        .spacing(6)
        .align_y(alignment::Vertical::Center);
    if r.is_final {
        title = title.push(final_badge());
    }
    if r.is_head {
        title = title.push(label("(current)", 11.0, ARIAL, muted(), 13.0));
    }
    let who = match &r.author {
        Some(author) => format!("{} \u{b7} {author}", r.when),
        None => r.when.clone(),
    };
    let mut body = column![title, label(&who, 11.0, ARIAL, muted(), 13.0)].spacing(3);
    if let Some(message) = &r.message {
        body = body.push(text(message.as_str()).size(12).font(ARIAL).color(text_main()));
    }
    let mut actions = row![].spacing(6);
    if !r.is_head {
        actions = actions.push(msg_action_btn("Open", Length::Fill, Message::Versions(VersionsMsg::View(r.id.clone()))));
    }
    actions = actions.push(msg_action_btn(
        "Save a copy...",
        Length::Fill,
        Message::Versions(VersionsMsg::SaveCopy(r.id.clone())),
    ));
    let border_color = if on_screen { accent() } else { border() };
    container(body.push(actions).spacing(6))
        .padding(8)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(bg_app().into()),
            border: Border { color: border_color, width: 1.0, radius: 3.0.into() },
            ..container::Style::default()
        })
        .into()
}

/// The Versions dock panel: Mark as Final... above the list, newest first.
pub fn panel<'a>(rows: &'a [VersionRow], viewing: Option<&str>) -> Element<'a, Message> {
    let list = column(rows.iter().map(|r| version_row(r, viewing == Some(r.id.as_str()))))
        .spacing(6)
        .padding(iced::Padding { right: 12.0, ..iced::Padding::ZERO });
    column![
        msg_action_btn("Mark as Final...", Length::Fill, Message::Versions(VersionsMsg::MarkFinal)),
        Space::new().height(8),
        scroll(list, bg_panel()),
    ]
    .into()
}

/// The banner across the top of the canvas while an old version is open.
pub fn banner<'a>(v: &ViewingRevision, read_only: bool) -> Element<'a, Message> {
    let status = if v.is_final { "Final" } else { "Draft" };
    let amber = ICON_ORANGE;
    let words = format!("Viewing v{} ({status}, {}) \u{2014} read-only", v.sequence, v.when);
    let mut bar = row![label(&words, 12.0, SEGOE, text_main(), 16.0), Space::new().width(Length::Fill)]
        .spacing(8)
        .align_y(alignment::Vertical::Center);
    // A read-only file cannot take the restored revision.
    if !read_only {
        bar = bar.push(msg_action_btn("Restore this version", 140, Message::Versions(VersionsMsg::RestoreViewed)));
    }
    bar = bar
        .push(msg_action_btn("Save a copy...", 104, Message::Versions(VersionsMsg::SaveCopyViewed)))
        .push(msg_action_btn("Back to latest", 104, Message::Versions(VersionsMsg::BackToLatest)));
    container(bar)
        .padding([5, 10])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(over(amber, 0.22, bg_panel()).into()),
            border: Border { color: amber, width: 1.0, radius: 0.0.into() },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(sequence: u64, status: RevisionStatus, author: Option<&str>) -> RevisionSummary {
        RevisionSummary {
            id: format!("r{sequence}"),
            parents: vec![],
            sequence,
            created_at: "2026-11-12T15:04:05.000Z".into(),
            author: author.map(str::to_string),
            message: (sequence == 2).then(|| "two".to_string()),
            status,
            content_hash: String::new(),
        }
    }

    #[test]
    fn rows_are_newest_first_and_mark_the_head() {
        let history = [
            summary(1, RevisionStatus::Draft, Some("Ann")),
            summary(2, RevisionStatus::Final, None),
            summary(3, RevisionStatus::Draft, Some(" ")),
        ];
        let rows = rows_from_history(&history);
        assert_eq!(rows.iter().map(|r| r.sequence).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(rows.iter().map(|r| r.is_head).collect::<Vec<_>>(), [true, false, false]);
        assert!(rows[1].is_final && !rows[0].is_final);
        assert_eq!(rows[1].message.as_deref(), Some("two"));
        assert_eq!(rows[2].author.as_deref(), Some("Ann"));
        assert_eq!(rows[0].author, None, "a blank author is no author");
    }

    #[test]
    fn edit_messages_are_gated_and_navigation_is_not() {
        assert!(edits_document(&Message::Delete));
        assert!(edits_document(&Message::CanvasPress(1.0, 2.0)));
        assert!(edits_document(&Message::Key(crate::input::KeyCmd::Paste)));
        assert!(!edits_document(&Message::Noop));
        assert!(!edits_document(&Message::Wheel { up: true, x: 0.0, y: 0.0, shift: false, ctrl: false }));
        assert!(!edits_document(&Message::Key(crate::input::KeyCmd::Escape)));
    }
}
