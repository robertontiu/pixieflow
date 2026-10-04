//! Finding RAW photos on disk and matching them by name.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// RAW extensions we recognise (compared case-insensitively).
pub const RAW_EXTS: &[&str] = &[
    "cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "rw2", "pef", "srw", "raw",
];

pub fn has_ext(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

pub fn is_raw(path: &Path) -> bool {
    has_ext(path, RAW_EXTS)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

/// The key two files are matched on: the file name without extension,
/// lowercased. `IMG_0042.CR3` in the folder matches `IMG_0042.jpg` in Pixieset.
pub fn stem_key(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    stem.to_lowercase()
}

/// All RAW files under `dir`, recursively, sorted by path. Hidden files and
/// folders (e.g. `.Trashes`, `._IMG_0001.CR3`) are ignored.
pub fn scan_raws(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            if is_hidden(&path) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if is_raw(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Number of visible files directly in `dir` with one of `exts`.
/// A missing folder counts as zero.
pub fn count_files(dir: &Path, exts: &[&str]) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && !is_hidden(p) && has_ext(p, exts))
        .count()
}
