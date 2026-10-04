//! RAW → JPG conversion for the Pixieset proofing gallery.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

use serde::{Deserialize, Serialize};

use crate::files::{scan_raws, stem_key};

/// Longest edge in pixels. Sharp full-screen on most laptops and monitors,
/// without uploading 24MP files.
const MAX_EDGE: u32 = 2048;
/// Visually indistinguishable from 1.0 at about a third of the file size.
const JPEG_QUALITY: f64 = 0.9;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertProgress {
    pub project_id: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertSummary {
    pub finished_at: u64,
    /// Photos converted in this run.
    pub converted: usize,
    /// Photos that already had a JPG from an earlier run.
    pub already_done: usize,
    /// RAW files that couldn't be read or converted.
    pub failed: Vec<String>,
    /// RAW files skipped because another RAW has the same name
    /// (their JPGs would overwrite each other in Pixieset).
    pub duplicates: Vec<String>,
}

/// Converts every RAW in `source_dir` that doesn't have a JPG yet.
/// `on_progress(done, total)` is called from worker threads.
pub fn convert(
    source_dir: &Path,
    jpg_dir: &Path,
    on_progress: impl Fn(usize, usize) + Sync,
) -> Result<ConvertSummary, String> {
    let raws = scan_raws(source_dir).map_err(|e| format!("Couldn't read the photos folder: {e}"))?;
    if raws.is_empty() {
        return Err("There are no RAW photos in this project's folder.".into());
    }
    fs::create_dir_all(jpg_dir).map_err(|e| format!("Couldn't create the JPG folder: {e}"))?;
    remove_partial_files(jpg_dir);

    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    let mut duplicates = Vec::new();
    for raw in raws {
        let name = raw.file_name().unwrap().to_string_lossy().into_owned();
        if seen.insert(stem_key(&name)) {
            unique.push(raw);
        } else {
            duplicates.push(relative(&raw, source_dir));
        }
    }

    let jobs: Vec<(PathBuf, PathBuf)> = unique
        .iter()
        .map(|raw| (raw.clone(), jpg_dir.join(jpg_name(raw))))
        .filter(|(_, jpg)| !jpg.exists())
        .collect();
    let already_done = unique.len() - jobs.len();
    let total = jobs.len();

    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = Mutex::new(Vec::new());
    on_progress(0, total);

    let workers = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(total.max(1));
    thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((raw, jpg)) = jobs.get(i) else { break };
                if !convert_one(raw, jpg) {
                    failed.lock().unwrap().push(relative(raw, source_dir));
                }
                on_progress(done.fetch_add(1, Ordering::SeqCst) + 1, total);
            });
        }
    });

    let mut failed = failed.into_inner().unwrap();
    failed.sort();
    Ok(ConvertSummary {
        finished_at: crate::project::now_millis(),
        converted: total - failed.len(), already_done, failed, duplicates })
}

fn jpg_name(raw: &Path) -> String {
    format!("{}.jpg", raw.file_stem().unwrap().to_string_lossy())
}

fn partial_path(jpg: &Path) -> PathBuf {
    let name = jpg.file_name().unwrap().to_string_lossy();
    jpg.with_file_name(format!(".{name}.partial.jpg"))
}

/// Writes to a hidden temp file first and renames on success, so a quit
/// mid-conversion never leaves a half-written JPG that looks finished.
fn convert_one(raw: &Path, jpg: &Path) -> bool {
    let partial = partial_path(jpg);
    let ok = crate::imageio::web_jpeg(raw, &partial, MAX_EDGE, JPEG_QUALITY);
    if ok && partial.exists() && fs::rename(&partial, jpg).is_ok() {
        return true;
    }
    let _ = fs::remove_file(&partial);
    false
}

fn remove_partial_files(jpg_dir: &Path) {
    let Ok(entries) = fs::read_dir(jpg_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && name.ends_with(".partial.jpg") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().into_owned()
}
