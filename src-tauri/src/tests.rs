//! End-to-end run of the whole flow on a throwaway folder. The "RAWs" are
//! JPEGs with a RAW extension — ImageIO reads by content, so that's enough.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::project::{now_millis, Store};
use crate::{convert, selection};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pixieflow-{name}-{}", now_millis()));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn fake_raw(sample: &Path, path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::copy(sample, path).unwrap();
}

#[test]
fn full_flow() {
    let root = temp_dir("flow");
    let sample = root.join("sample.jpg");
    let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icon.png");
    assert!(Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg", icon, "--out"])
        .arg(&sample)
        .output()
        .unwrap()
        .status
        .success());

    let shoot = root.join("Ana and Matei");
    fake_raw(&sample, &shoot.join("card1/IMG_0001.CR3"));
    fake_raw(&sample, &shoot.join("card2/IMG_0002.cr3"));
    fake_raw(&sample, &shoot.join("card2/IMG_0001.CR3")); // same name as card1's
    fake_raw(&sample, &shoot.join(".hidden/IMG_0003.CR3"));
    fs::write(shoot.join("card1/readme.txt"), "not a photo").unwrap();

    let store = Store::load(root.join("projects.json"));
    let draft = store.draft(&shoot).unwrap();
    assert_eq!(draft.raw_count, 3);
    assert_eq!(draft.jpg_dir, root.join("Ana and Matei - JPG"));
    assert_eq!(draft.selection_dir, root.join("Ana and Matei - Selection"));

    // Convert, then convert again: the second run has nothing left to do.
    let summary = convert::convert(&shoot, &draft.jpg_dir, |_, _| {}).unwrap();
    assert_eq!(summary.converted, 2);
    assert!(summary.failed.is_empty());
    assert_eq!(summary.duplicates, vec!["card2/IMG_0001.CR3".to_string()]);
    assert!(draft.jpg_dir.join("IMG_0001.jpg").is_file());
    assert!(draft.jpg_dir.join("IMG_0002.jpg").is_file());
    let again = convert::convert(&shoot, &draft.jpg_dir, |_, _| {}).unwrap();
    assert_eq!((again.converted, again.already_done), (0, 2));

    // Apply the client's picks.
    let csv = root.join("favlist.csv");
    fs::write(
        &csv,
        "\"Collection: Ana and Matei\",\"Favorite: My Favorites\",\"Email x@y.z\",\n\
         Name,Note,\"Photo Set\",\"Created at\"\n\
         IMG_0001.jpg,\"B&W <please>\",Highlights,4/10/2026\n\
         IMG_0002.jpg,,Highlights,4/10/2026\n\
         IMG_9999.jpg,,Highlights,4/10/2026\n",
    )
    .unwrap();
    let sel = selection::apply(&shoot, &draft.selection_dir, &csv).unwrap();
    assert_eq!(sel.collection.as_deref(), Some("Ana and Matei"));
    assert_eq!((sel.requested, sel.copied, sel.notes_written), (3, 2, 1));
    assert_eq!(sel.missing, vec!["IMG_9999.jpg".to_string()]);
    assert!(draft.selection_dir.join("IMG_0001.CR3").is_file());
    assert!(draft.selection_dir.join("IMG_0002.cr3").is_file());
    let xmp = fs::read_to_string(draft.selection_dir.join("IMG_0001.xmp")).unwrap();
    assert!(xmp.contains("B&amp;W &lt;please&gt;"));

    // Re-applying doesn't copy again, and never overwrites a sidecar Lightroom wrote.
    fs::write(draft.selection_dir.join("IMG_0001.xmp"), "<lightroom edits/>").unwrap();
    let sel = selection::apply(&shoot, &draft.selection_dir, &csv).unwrap();
    assert_eq!((sel.copied, sel.already_there, sel.notes_written), (0, 2, 0));
    assert_eq!(fs::read_to_string(draft.selection_dir.join("IMG_0001.xmp")).unwrap(), "<lightroom edits/>");

    // A list from another shoot is rejected outright.
    let other = root.join("other.csv");
    fs::write(&other, "Name,Note\nDSC_0001.jpg,\n").unwrap();
    assert!(selection::apply(&shoot, &draft.selection_dir, &other).is_err());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn refuses_dangerous_folders() {
    let store = Store::load(temp_dir("store").join("projects.json"));
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    assert!(store.draft(&home).is_err());
    assert!(store.draft(&home.join("Pictures")).is_err());
    assert!(store.draft(Path::new("/")).is_err());

    let card = temp_dir("card").join("DCIM/100CANON");
    fs::create_dir_all(&card).unwrap();
    let err = store.draft(&card).err().unwrap();
    assert!(err.contains("memory card"), "{err}");
}

/// Runs against a real camera file if one is dropped in the repo root
/// (RAWs are git-ignored, so this is skipped on a fresh checkout).
#[test]
fn converts_real_raw_from_embedded_preview() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let Some(raw) = fs::read_dir(root).unwrap().flatten().map(|e| e.path()).find(|p| crate::files::is_raw(p)) else {
        eprintln!("no sample RAW in the repo root, skipping");
        return;
    };
    let dir = temp_dir("real");
    let shoot = dir.join("shoot");
    fs::create_dir_all(&shoot).unwrap();
    for i in 0..20 {
        fs::copy(&raw, shoot.join(format!("IMG_{i:04}.CR3"))).unwrap();
    }
    let jpgs = dir.join("jpg");

    let start = std::time::Instant::now();
    let summary = convert::convert(&shoot, &jpgs, |_, _| {}).unwrap();
    let elapsed = start.elapsed();
    assert_eq!((summary.converted, summary.failed.len()), (20, 0));

    let out = Command::new("/usr/bin/sips")
        .args(["-g", "pixelWidth", "-g", "pixelHeight", "-g", "profile"])
        .arg(jpgs.join("IMG_0000.jpg"))
        .output()
        .unwrap();
    let info = String::from_utf8_lossy(&out.stdout);
    eprintln!("20 photos in {elapsed:?}\n{info}");
    assert!(info.contains("pixelWidth: 2048") || info.contains("pixelHeight: 2048"), "{info}");
    assert!(info.contains("sRGB"), "{info}");
    fs::remove_dir_all(dir).unwrap();
}
