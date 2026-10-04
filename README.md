# Pixieflow

A small Mac app I built for my wife, who does photography as a hobby.

Her process for each shoot is always the same. She copies the RAW photos off the
memory card, makes small JPG copies, uploads those to [Pixieset](https://pixieset.com)
and sends the gallery link to the client. Days or weeks later the client picks
their favorites. She then exports that list and pulls the matching RAW files
into a separate folder to edit in Lightroom.

At first I wrote two shell scripts for this. They worked, but every time she
needed them the question was "how do I run the script again?". Pixieflow wraps
the same process in an app she can just click through.

## What it does

Each shoot is a **project**. She picks the folder she copied the RAWs into, and
the app keeps track of it until the client has chosen. Projects are remembered
between launches, since the client can take a while.

1. **Convert to JPG.** Every RAW in the folder (subfolders included) is converted
   to a 1080px JPG with macOS's built-in `sips`, into a `<folder> - JPG` folder
   next to the original. If she quits halfway, it picks up where it stopped.
2. **Upload to Pixieset.** Buttons open the JPG folder and Pixieset so she can
   drag the photos in. Pixieset has no public API, so this step stays manual.
3. **Client's selection.** She drops the favorites CSV exported from Pixieset
   onto the app. The matching RAWs are copied into `<folder> - Selection`.
   Any notes the client left on a photo are written into an XMP sidecar, so
   they show up in Lightroom as the photo's caption.

**Delete project** moves all three folders (RAW, JPG and selection) to the Trash.

```
Pictures/
├── Ana and Matei/               ← the RAWs she copied off the card
├── Ana and Matei - JPG/         ← created by the app, uploaded to Pixieset
└── Ana and Matei - Selection/   ← created by the app, imported into Lightroom
```

## Installing

Download the `.dmg` from the [latest release](https://github.com/robertontiu/pixieflow/releases/latest)
and drag Pixieflow into Applications.

The app isn't notarized by Apple (that needs a paid developer account), so macOS
blocks it the first time. Open it once, then go to **System Settings → Privacy &
Security** and click **Open Anyway**. After that the app updates itself: when a
new version is out, it asks on launch whether to install it.

## Development

Built with [Tauri 2](https://tauri.app) (a Rust backend with a React UI).
You need Node, Rust and Xcode's command line tools.

```sh
npm install
npm run tauri dev               # run the app
cd src-tauri && cargo test      # run the backend tests
```

The backend in `src-tauri/src/` does the actual work:

| File | What it does |
| --- | --- |
| `files.rs` | finds RAW files and matches them by name |
| `convert.rs` | RAW → JPG conversion with `sips`, in parallel |
| `selection.rs` | reads the Pixieset CSV, copies the picks, writes the XMP captions |
| `project.rs` | projects, where they're saved, and the safety checks on which folders can be used |

## Releasing

```sh
scripts/release.sh 0.2.0 "What changed, shown in the update prompt"
scripts/release.sh --dry-run    # build only; nothing is committed or published
```

The script sets the version everywhere and builds a universal app (Intel and
Apple Silicon). It then signs the update and writes `latest.json` for the
updater, commits, tags, pushes, and creates the GitHub release.

Updates are signed with a key at `~/.tauri/pixieflow.key`, which is not in this
repo. The app only installs updates signed with that key, so if the key is lost,
installed copies can't update anymore and have to be reinstalled by hand.
