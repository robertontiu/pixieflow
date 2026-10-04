//! Projects and where they're saved between sessions.
//!
//! A project is one shoot: the folder of RAWs she copied off the card, plus
//! two sibling folders the app creates next to it:
//!
//! ```text
//! Pictures/Ana and Matei/              ← RAWs (she picks this)
//! Pictures/Ana and Matei - JPG/        ← upload these to Pixieset
//! Pictures/Ana and Matei - Selection/  ← RAWs the client picked
//! ```

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::convert::ConvertSummary;
use crate::files::{count_files, scan_raws, stem_key, RAW_EXTS};
use crate::selection::SelectionSummary;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub source_dir: PathBuf,
    pub jpg_dir: PathBuf,
    pub selection_dir: PathBuf,
    pub created_at: u64,
    pub last_convert: Option<ConvertSummary>,
    pub last_selection: Option<SelectionSummary>,
}

/// A project plus what's currently on disk, for the UI.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    #[serde(flatten)]
    pub project: Project,
    pub source_exists: bool,
    pub raw_count: usize,
    /// RAWs that don't have a JPG yet.
    pub pending_convert: usize,
    pub jpg_count: usize,
    pub selection_count: usize,
    pub converting: bool,
}

impl ProjectView {
    pub fn new(project: Project, converting: bool) -> Self {
        let raws = scan_raws(&project.source_dir).unwrap_or_default();
        let mut seen = HashSet::new();
        let pending_convert = raws
            .iter()
            .filter(|raw| {
                let name = raw.file_name().unwrap().to_string_lossy();
                seen.insert(stem_key(&name))
                    && !project.jpg_dir.join(format!("{}.jpg", raw.file_stem().unwrap().to_string_lossy())).exists()
            })
            .count();
        Self {
            source_exists: project.source_dir.is_dir(),
            raw_count: raws.len(),
            pending_convert,
            jpg_count: count_files(&project.jpg_dir, &["jpg"]),
            selection_count: count_files(&project.selection_dir, RAW_EXTS),
            converting,
            project,
        }
    }
}

/// What creating a project from a folder would do, shown before she confirms.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDraft {
    pub name: String,
    pub source_dir: PathBuf,
    pub jpg_dir: PathBuf,
    pub selection_dir: PathBuf,
    pub raw_count: usize,
}

pub struct Store {
    path: PathBuf,
    pub projects: Vec<Project>,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let projects = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, projects }
    }

    pub fn save(&self) -> Result<(), String> {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.path.parent() {
                fs::create_dir_all(dir)?;
            }
            let tmp = self.path.with_extension("json.tmp");
            fs::write(&tmp, serde_json::to_vec_pretty(&self.projects)?)?;
            fs::rename(tmp, &self.path)
        };
        write().map_err(|e| format!("Couldn't save projects: {e}"))
    }

    pub fn get(&self, id: &str) -> Result<&Project, String> {
        self.projects.iter().find(|p| p.id == id).ok_or_else(|| "Project not found.".to_string())
    }

    pub fn get_mut(&mut self, id: &str) -> Result<&mut Project, String> {
        self.projects.iter_mut().find(|p| p.id == id).ok_or_else(|| "Project not found.".to_string())
    }

    /// Checks a folder she picked and works out the project's folders.
    pub fn draft(&self, source_dir: &Path) -> Result<ProjectDraft, String> {
        let source_dir = source_dir
            .canonicalize()
            .map_err(|_| "That folder doesn't exist anymore.".to_string())?;
        if !source_dir.is_dir() {
            return Err("Please choose a folder, not a file.".into());
        }
        check_not_special(&source_dir)?;

        for p in &self.projects {
            for dir in [&p.source_dir, &p.jpg_dir, &p.selection_dir] {
                if source_dir.starts_with(dir) || dir.starts_with(&source_dir) {
                    return Err(format!("This folder is already part of the project \"{}\".", p.name));
                }
            }
        }

        let raw_count = scan_raws(&source_dir).map_err(|e| format!("Couldn't read that folder: {e}"))?.len();
        if raw_count == 0 {
            return Err("There are no RAW photos in that folder.".into());
        }

        let name = source_dir.file_name().unwrap().to_string_lossy().into_owned();
        let parent = source_dir.parent().unwrap();
        Ok(ProjectDraft {
            jpg_dir: parent.join(format!("{name} - JPG")),
            selection_dir: parent.join(format!("{name} - Selection")),
            name,
            source_dir,
            raw_count,
        })
    }
}

/// Deleting a project deletes its RAW folder, so refuse folders where that
/// would be a disaster: the home folder, its standard folders, a whole disk,
/// or the memory card itself.
fn check_not_special(dir: &Path) -> Result<(), String> {
    if dir.parent().is_none() || dir.parent() == Some(Path::new("/Volumes")) {
        return Err("Please choose the folder with this shoot's photos, not a whole disk.".into());
    }
    if dir.components().any(|c| matches!(c, Component::Normal(n) if n.eq_ignore_ascii_case("DCIM"))) {
        return Err("This looks like the camera's memory card. Copy the photos to a folder on the Mac first, then choose that folder.".into());
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let special = ["", "Desktop", "Documents", "Downloads", "Pictures", "Movies", "Music", "Library"];
        if special.iter().any(|s| dir == home.join(s)) || !dir.starts_with(&home) && home.starts_with(dir) {
            return Err("Please choose the folder with this shoot's photos, not a general folder.".into());
        }
    }
    Ok(())
}

pub fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
