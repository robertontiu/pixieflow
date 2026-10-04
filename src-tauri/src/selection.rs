//! Applying a Pixieset favorites export: copy the matching RAWs into the
//! selection folder, and put the client's notes where Lightroom can see them.
//!
//! The export looks like this (first line is collection info, then a header):
//!
//! ```text
//! "Collection: Ana and Matei","Favorite: My Favorites","Email ...",
//! Name,Note,"Photo Set","Created at"
//! IMG_1206.jpg,,Highlights,4/10/2026
//! IMG_1508.jpg,"Can this one be black & white?",Highlights,4/10/2026
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::files::{scan_raws, stem_key};

/// Marker so we only ever overwrite sidecars we wrote ourselves, never one
/// Lightroom wrote after she started editing.
const XMP_MARKER: &str = r#"x:xmptk="Pixieflow""#;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSummary {
    pub applied_at: u64,
    pub csv_file: String,
    pub collection: Option<String>,
    /// Distinct photos in the list.
    pub requested: usize,
    /// RAWs copied in this run.
    pub copied: usize,
    /// RAWs that were already in the selection folder.
    pub already_there: usize,
    /// Photos whose note was written as a Lightroom caption.
    pub notes_written: usize,
    /// Names from the list with no matching RAW.
    pub missing: Vec<String>,
}

struct Pick {
    name: String,
    notes: Vec<String>,
}

struct Export {
    collection: Option<String>,
    picks: Vec<Pick>,
}

pub fn apply(source_dir: &Path, selection_dir: &Path, csv_path: &Path) -> Result<SelectionSummary, String> {
    let export = parse_export(csv_path)?;
    if export.picks.is_empty() {
        return Err("This favorites list is empty.".into());
    }

    let raws = scan_raws(source_dir).map_err(|e| format!("Couldn't read the photos folder: {e}"))?;
    let mut by_key = HashMap::new();
    for raw in raws {
        let name = raw.file_name().unwrap().to_string_lossy().into_owned();
        by_key.entry(stem_key(&name)).or_insert(raw);
    }

    if !export.picks.iter().any(|p| by_key.contains_key(&stem_key(&p.name))) {
        return Err(format!(
            "None of the {} photos in this list are in this project's folder. \
             Is it the list for a different client?",
            export.picks.len()
        ));
    }

    fs::create_dir_all(selection_dir).map_err(|e| format!("Couldn't create the selection folder: {e}"))?;

    let mut summary = SelectionSummary {
        applied_at: crate::project::now_millis(),
        csv_file: csv_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        collection: export.collection,
        requested: export.picks.len(),
        copied: 0,
        already_there: 0,
        notes_written: 0,
        missing: Vec::new(),
    };

    for pick in &export.picks {
        let Some(raw) = by_key.get(&stem_key(&pick.name)) else {
            summary.missing.push(pick.name.clone());
            continue;
        };
        let dest = selection_dir.join(raw.file_name().unwrap());
        let same_size = |a: &Path, b: &Path| {
            matches!((fs::metadata(a), fs::metadata(b)), (Ok(x), Ok(y)) if x.len() == y.len())
        };
        if same_size(raw, &dest) {
            summary.already_there += 1;
        } else {
            fs::copy(raw, &dest).map_err(|e| format!("Couldn't copy {}: {e}", pick.name))?;
            summary.copied += 1;
        }
        if !pick.notes.is_empty() && write_caption_sidecar(&dest, &pick.notes.join("\n")) {
            summary.notes_written += 1;
        }
    }
    Ok(summary)
}

fn parse_export(csv_path: &Path) -> Result<Export, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(csv_path)
        .map_err(|e| format!("Couldn't open the file: {e}"))?;

    let mut collection = None;
    let mut columns: Option<(usize, Option<usize>)> = None;
    let mut picks: Vec<Pick> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for record in reader.byte_records() {
        let record = record.map_err(|e| format!("Couldn't read the file: {e}"))?;
        let fields: Vec<String> = record
            .iter()
            .map(|f| String::from_utf8_lossy(f).trim_start_matches('\u{feff}').trim().to_string())
            .collect();

        let Some((name_col, note_col)) = columns else {
            for f in &fields {
                if let Some(c) = f.strip_prefix("Collection:") {
                    collection = Some(c.trim().to_string());
                }
            }
            let find = |label: &str| fields.iter().position(|f| f.eq_ignore_ascii_case(label));
            if let Some(name_col) = find("Name") {
                columns = Some((name_col, find("Note")));
            }
            continue;
        };

        let name = fields.get(name_col).cloned().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let note = note_col.and_then(|i| fields.get(i)).cloned().unwrap_or_default();
        // The same photo can appear more than once (e.g. in several photo sets).
        let i = *index.entry(stem_key(&name)).or_insert_with(|| {
            picks.push(Pick { name, notes: Vec::new() });
            picks.len() - 1
        });
        if !note.is_empty() && !picks[i].notes.contains(&note) {
            picks[i].notes.push(note);
        }
    }

    if columns.is_none() {
        return Err("This doesn't look like a Pixieset favorites export (no \"Name\" column).".into());
    }
    Ok(Export { collection, picks })
}

/// Writes `IMG_0001.xmp` next to `IMG_0001.CR3` with the note as the photo's
/// caption. Lightroom reads RAW sidecars on import and shows it under
/// Metadata → Caption. Returns false if a sidecar we didn't write is in the way.
fn write_caption_sidecar(raw: &Path, note: &str) -> bool {
    let sidecar = raw.with_extension("xmp");
    if let Ok(existing) = fs::read_to_string(&sidecar) {
        if !existing.contains(XMP_MARKER) {
            return false;
        }
    }
    let xmp = format!(
        r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" {XMP_MARKER}>
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:description>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">{note}</rdf:li>
    </rdf:Alt>
   </dc:description>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#,
        bom = '\u{feff}',
        note = escape_xml(note),
    );
    fs::write(&sidecar, xmp).is_ok()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pixieset_export_with_notes_and_duplicates() {
        let dir = std::env::temp_dir().join(format!("pixieflow-test-{}", crate::project::now_millis()));
        fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("favlist.csv");
        fs::write(
            &csv,
            "\u{feff}\"Collection: Ana and Matei\",\"Favorite: My Favorites\",\"Email a@b.c\",\n\
             Name,Note,\"Photo Set\",\"Created at\"\n\
             IMG_1206.jpg,,Highlights,4/10/2026\n\
             IMG_1508.jpg,\"Black & white, please\",Highlights,4/10/2026\n\
             IMG_1508.jpg,,Part II,4/10/2026\n\
             \n",
        )
        .unwrap();
        let export = parse_export(&csv).unwrap();
        assert_eq!(export.collection.as_deref(), Some("Ana and Matei"));
        assert_eq!(export.picks.len(), 2);
        assert_eq!(export.picks[1].notes, vec!["Black & white, please".to_string()]);
        fs::remove_dir_all(dir).unwrap();
    }
}
