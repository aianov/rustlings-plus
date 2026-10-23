use anyhow::{Context, Result};
use notify::{
    Event, EventKind,
    event::{AccessKind, AccessMode, MetadataKind, ModifyKind, RenameMode},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{RecvTimeoutError, Sender, SyncSender, sync_channel},
    thread,
    time::Duration,
};

use crate::{
    course::{Course, Kind},
    watch::{WatchEvent, events_paused},
};

const DEBOUNCE_DURATION: Duration = Duration::from_millis(200);

pub struct WatchMap {
    roots: Vec<PathBuf>,
    files: HashMap<String, usize>,
    dirs: Vec<(String, usize)>,
    n_targets: usize,
}

fn ignored(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.split('/').any(|c| c == "target" || c == ".git")
        || name == "Cargo.lock"
        || name.starts_with('.')
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".md")
}

impl WatchMap {
    pub fn new(course: &Course, root: &Path) -> Self {
        let mut roots = vec![root.to_path_buf()];
        if let Ok(canonical) = root.canonicalize()
            && canonical != root
        {
            roots.push(canonical);
        }
        let mut files = HashMap::new();
        let mut dirs = Vec::new();
        for (ind, ex) in course.exercises.iter().enumerate() {
            match ex.kind {
                Kind::File => {
                    files.insert(ex.src.clone(), ind);
                }
                Kind::Project => dirs.push((format!("{}/", ex.src), ind)),
                Kind::Milestone | Kind::Question => (),
            }
        }
        let n = course.exercises.len();
        for (j, journey) in course.journeys.iter().enumerate() {
            dirs.push((format!("{}/", journey.dir), n + j));
        }
        Self {
            roots,
            files,
            dirs,
            n_targets: n + course.journeys.len(),
        }
    }

    pub fn target(&self, path: &Path) -> Option<usize> {
        let rel = self.roots.iter().find_map(|r| path.strip_prefix(r).ok())?;
        let rel = rel.to_str()?.replace('\\', "/");
        if ignored(&rel) {
            return None;
        }
        if let Some(ind) = self.files.get(&rel) {
            return Some(*ind);
        }
        self.dirs
            .iter()
            .find(|(prefix, _)| rel.starts_with(prefix.as_str()))
            .map(|(_, ind)| *ind)
    }
}

pub struct NotifyEventHandler {
    error_sender: Sender<WatchEvent>,
    update_sender: SyncSender<usize>,
    map: WatchMap,
}

impl NotifyEventHandler {
    pub fn build(watch_event_sender: Sender<WatchEvent>, map: WatchMap) -> Result<Self> {
        let (update_sender, update_receiver) = sync_channel(0);
        let error_sender = watch_event_sender.clone();
        let n_targets = map.n_targets;

        thread::Builder::new()
            .spawn(move || {
                let mut updated = vec![false; n_targets];
                loop {
                    match update_receiver.recv_timeout(DEBOUNCE_DURATION) {
                        Ok(target) => updated[target] = true,
                        Err(RecvTimeoutError::Timeout) => {
                            for (target, flag) in updated.iter_mut().enumerate() {
                                if *flag {
                                    if watch_event_sender
                                        .send(WatchEvent::FileChange { target })
                                        .is_err()
                                    {
                                        break;
                                    }
                                    *flag = false;
                                }
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .context("Failed to spawn a thread to debounce file changes")?;

        Ok(Self {
            error_sender,
            update_sender,
            map,
        })
    }
}

impl notify::EventHandler for NotifyEventHandler {
    fn handle_event(&mut self, input_event: notify::Result<Event>) {
        if events_paused() {
            return;
        }

        let input_event = match input_event {
            Ok(v) => v,
            Err(e) => {
                let _ = self.error_sender.send(WatchEvent::NotifyErr(e));
                return;
            }
        };

        match input_event.kind {
            EventKind::Any | EventKind::Create(_) | EventKind::Remove(_) => (),
            EventKind::Modify(modify_kind) => match modify_kind {
                ModifyKind::Any | ModifyKind::Data(_) => (),
                ModifyKind::Name(rename_mode) => match rename_mode {
                    RenameMode::Any | RenameMode::To | RenameMode::Both => (),
                    RenameMode::From | RenameMode::Other => return,
                },
                ModifyKind::Metadata(metadata_kind) => match metadata_kind {
                    MetadataKind::Any | MetadataKind::WriteTime => (),
                    MetadataKind::AccessTime
                    | MetadataKind::Permissions
                    | MetadataKind::Ownership
                    | MetadataKind::Extended
                    | MetadataKind::Other => return,
                },
                ModifyKind::Other => return,
            },
            EventKind::Access(access_kind) => match access_kind {
                AccessKind::Any => (),
                AccessKind::Close(access_mode) => match access_mode {
                    AccessMode::Any | AccessMode::Write => (),
                    AccessMode::Execute | AccessMode::Read | AccessMode::Other => return,
                },
                AccessKind::Read | AccessKind::Open(_) | AccessKind::Other => return,
            },
            EventKind::Other => return,
        }

        let _ = input_event
            .paths
            .iter()
            .filter_map(|path| self.map.target(path))
            .try_for_each(|target| self.update_sender.send(target));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::tests::fixture_course;

    #[test]
    fn paths_map_to_targets() {
        let course = fixture_course();
        let root = Path::new("/learner");
        let map = WatchMap::new(course, root);
        let t = |p: &str| map.target(&root.join(p));
        assert_eq!(t("exercises/basics/00_intro/intro1.rs"), Some(0));
        assert_eq!(t("exercises/basics/00_intro/other.rs"), None);
        assert_eq!(t("exercises/basics/02_more/proj/src/main.rs"), Some(2));
        assert_eq!(t("exercises/basics/02_more/proj/Cargo.toml"), Some(2));
        assert_eq!(t("exercises/basics/02_more/proj/target/debug/x"), None);
        assert_eq!(t("exercises/basics/02_more/proj/Cargo.lock"), None);
        assert_eq!(t("backend/src/lib.rs"), Some(5));
        assert_eq!(t("backend/README.md"), None);
        assert_eq!(map.target(Path::new("/elsewhere/x.rs")), None);
    }
}
