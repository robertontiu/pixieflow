import { useCallback, useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { api, ConvertProgress, errorText, PIXIESET_URL, plural, Project, stageOf } from "../api";
import DeleteDialog from "./DeleteDialog";

export default function ProjectPage({
  id,
  progress,
  onBack,
}: {
  id: string;
  progress?: ConvertProgress;
  onBack: () => void;
}) {
  const [project, setProject] = useState<Project | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectionError, setSelectionError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [renaming, setRenaming] = useState(false);

  const reload = useCallback(() => {
    api.getProject(id).then(setProject, (e) => setError(errorText(e)));
  }, [id]);

  useEffect(() => {
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, [reload]);

  const converting = project?.converting ?? false;
  const done = progress && progress.done === progress.total;
  useEffect(() => {
    if (done) reload();
  }, [done, reload]);

  async function convert() {
    setError(null);
    setProject((p) => p && { ...p, converting: true });
    try {
      await api.convertProject(id);
    } catch (e) {
      setError(errorText(e));
    }
    reload();
  }

  const applySelection = useCallback(
    async (csvPath: string) => {
      setSelectionError(null);
      setApplying(true);
      try {
        await api.applySelection(id, csvPath);
      } catch (e) {
        setSelectionError(errorText(e));
      }
      setApplying(false);
      reload();
    },
    [id, reload],
  );

  async function chooseCsv() {
    const file = await open({ title: "Choose the favorites list from Pixieset", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (typeof file === "string") applySelection(file);
  }

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      const t = e.payload.type;
      if (t === "enter" || t === "over") setDragging(true);
      else setDragging(false);
      if (t === "drop") {
        const csv = e.payload.paths.find((p) => p.toLowerCase().endsWith(".csv"));
        if (csv) applySelection(csv);
        else setSelectionError("That's not a CSV file. Drop the favorites list you exported from Pixieset.");
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [applySelection]);

  if (!project) {
    return (
      <div className="page">
        <BackButton onBack={onBack} />
        {error && <p className="banner error">{error}</p>}
      </div>
    );
  }

  const stage = stageOf(project);
  const lc = project.lastConvert;
  const sel = project.lastSelection;

  return (
    <div className={`page ${dragging ? "dragging" : ""}`}>
      <BackButton onBack={onBack} />
      <header className="project-header">
        {renaming ? (
          <RenameInput
            name={project.name}
            onDone={async (name) => {
              setRenaming(false);
              if (name && name !== project.name) {
                await api.renameProject(id, name).catch((e) => setError(errorText(e)));
                reload();
              }
            }}
          />
        ) : (
          <h1 onDoubleClick={() => setRenaming(true)} title="Double-click to rename">
            {project.name}
          </h1>
        )}
        <button className="link" onClick={() => api.openFolder(project.sourceDir)}>
          {plural(project.rawCount, "RAW photo")} in “{basename(project.sourceDir)}”
        </button>
      </header>

      {!project.sourceExists && (
        <p className="banner error">
          The photos folder can't be found. Was it moved or renamed? It should be at: {project.sourceDir}
        </p>
      )}
      {error && <p className="banner error">{error}</p>}

      <ol className="steps">
        <Step n={1} title="Convert to JPG" state={stage === "convert" ? "active" : "done"}>
          {converting ? (
            <ProgressBar done={progress?.done ?? 0} total={progress?.total ?? 0} />
          ) : project.pendingConvert > 0 ? (
            <>
              <p>
                {project.jpgCount === 0
                  ? `Makes small JPG copies of all ${plural(project.rawCount, "photo")} for Pixieset.`
                  : `${plural(project.pendingConvert, "new photo")} still need converting.`}
              </p>
              <button className="primary" onClick={convert} disabled={!project.sourceExists}>
                {project.jpgCount === 0 ? "Convert to JPG" : `Convert ${plural(project.pendingConvert, "photo")}`}
              </button>
            </>
          ) : (
            <p>✓ {plural(project.jpgCount, "JPG")} ready.</p>
          )}
          {!converting && lc && lc.failed.length > 0 && (
            <Issues title={`${plural(lc.failed.length, "photo")} couldn't be converted`} items={lc.failed} />
          )}
          {!converting && lc && lc.duplicates.length > 0 && (
            <Issues
              title={`${plural(lc.duplicates.length, "photo")} skipped — another photo has the same name`}
              items={lc.duplicates}
            />
          )}
        </Step>

        <Step n={2} title="Upload to Pixieset" state={stage === "convert" ? "todo" : stage === "waiting" ? "active" : "done"}>
          <p>Drag the photos from the JPG folder into your Pixieset collection, then send your client the link.</p>
          <div className="row">
            <button className={stage === "waiting" ? "primary" : ""} onClick={() => api.openFolder(project.jpgDir)} disabled={project.jpgCount === 0}>
              Show JPG folder
            </button>
            <button onClick={() => api.openUrl(PIXIESET_URL)}>Open Pixieset</button>
          </div>
        </Step>

        <Step n={3} title="Client's selection" state={stage === "selected" ? "done" : stage === "waiting" ? "active" : "todo"}>
          {sel && (
            <div className="result">
              <p>
                ✓ <b>{plural(sel.copied + sel.alreadyThere, "photo")}</b> in the selection folder
                {sel.collection && <> · from “{sel.collection}”</>}
              </p>
              {sel.notesWritten > 0 && (
                <p className="muted">
                  {plural(sel.notesWritten, "photo")} with a client note — you'll find it in Lightroom under Metadata → Caption.
                </p>
              )}
              {sel.missing.length > 0 && (
                <Issues title={`${plural(sel.missing.length, "photo")} from the list aren't in the photos folder`} items={sel.missing} />
              )}
              <div className="row">
                <button className="primary" onClick={() => api.openFolder(project.selectionDir)}>
                  Show selection folder
                </button>
              </div>
            </div>
          )}
          {selectionError && <p className="banner error">{selectionError}</p>}
          <div className={`dropzone ${dragging ? "over" : ""}`}>
            {applying ? (
              <p>Copying photos…</p>
            ) : (
              <>
                <p>
                  {sel ? "Got an updated list? " : "When your client has picked their favorites, export the list from Pixieset and "}
                  drop the <b>.csv</b> file here
                </p>
                <button onClick={chooseCsv}>Choose file…</button>
              </>
            )}
          </div>
        </Step>
      </ol>

      <footer className="footer">
        <button className="link danger" onClick={() => setDeleting(true)} disabled={converting}>
          Delete project…
        </button>
      </footer>

      {deleting && <DeleteDialog project={project} onCancel={() => setDeleting(false)} onDeleted={onBack} />}
    </div>
  );
}

function BackButton({ onBack }: { onBack: () => void }) {
  return (
    <button className="link back" onClick={onBack}>
      ‹ All projects
    </button>
  );
}

function Step({ n, title, state, children }: { n: number; title: string; state: "todo" | "active" | "done"; children: React.ReactNode }) {
  return (
    <li className={`step ${state}`}>
      <div className="step-badge">{state === "done" ? "✓" : n}</div>
      <div className="step-body">
        <h3>{title}</h3>
        {children}
      </div>
    </li>
  );
}

function ProgressBar({ done, total }: { done: number; total: number }) {
  const pct = total ? Math.round((done / total) * 100) : 0;
  return (
    <div className="progress">
      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${pct}%` }} />
      </div>
      <p className="muted">
        Converting {done} of {total}… You can close this window — Pixieflow keeps going and lets you know when it's done.
      </p>
    </div>
  );
}

function Issues({ title, items }: { title: string; items: string[] }) {
  return (
    <details className="issues">
      <summary>⚠️ {title}</summary>
      <ul>
        {items.map((i) => (
          <li key={i}>{i}</li>
        ))}
      </ul>
    </details>
  );
}

function RenameInput({ name, onDone }: { name: string; onDone: (name: string) => void }) {
  const [value, setValue] = useState(name);
  return (
    <input
      className="rename"
      value={value}
      autoFocus
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onDone(value.trim())}
      onKeyDown={(e) => {
        if (e.key === "Enter") onDone(value.trim());
        if (e.key === "Escape") onDone(name);
      }}
    />
  );
}

function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}
