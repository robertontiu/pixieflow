import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, daysSince, errorText, formatDate, plural, Project, stageOf } from "../api";
import type { Progress } from "../App";
import NewProjectDialog from "./NewProjectDialog";
import { checkForUpdates } from "./UpdatePrompt";

export default function ProjectList({ progress, onOpen }: { progress: Progress; onOpen: (id: string) => void }) {
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [newSource, setNewSource] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState("");

  const reload = useCallback(() => {
    api.listProjects().then(setProjects, (e) => setError(errorText(e)));
  }, []);

  useEffect(() => {
    reload();
    import("@tauri-apps/api/app").then(({ getVersion }) => getVersion().then(setVersion));
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, [reload]);

  // Refresh the cards when a conversion finishes in the background.
  const finished = Object.values(progress).filter((p) => p.done === p.total).length;
  useEffect(reload, [finished, reload]);

  async function startNew() {
    const dir = await open({ directory: true, title: "Choose the folder with this shoot's RAW photos" });
    if (typeof dir === "string") setNewSource(dir);
  }

  return (
    <div className="page">
      <header className="topbar">
        <h1>Projects</h1>
        <button className="primary" onClick={startNew}>
          New project
        </button>
      </header>

      {error && <p className="banner error">{error}</p>}

      {projects?.length === 0 && (
        <div className="empty">
          <div className="empty-icon">📷</div>
          <h2>No projects yet</h2>
          <p>
            Copy a shoot's photos from the memory card into a folder on your Mac,
            <br />
            then click <b>New project</b> and choose that folder.
          </p>
          <button className="primary" onClick={startNew}>
            New project
          </button>
        </div>
      )}

      <ul className="cards">
        {projects?.map((p) => (
          <li key={p.id}>
            <button className="card" onClick={() => onOpen(p.id)}>
              <span className="card-title">{p.name}</span>
              <StatusPill project={p} progress={progress[p.id]} />
              <span className="card-meta">
                {plural(p.rawCount, "photo")}
                {p.lastSelection && <> · {p.selectionCount} picked</>} · {formatDate(p.createdAt)}
              </span>
            </button>
          </li>
        ))}
      </ul>

      <footer className="footer">
        Pixieflow {version} ·{" "}
        <button className="link" onClick={() => checkForUpdates(true)}>
          Check for updates
        </button>
      </footer>

      {newSource && (
        <NewProjectDialog
          sourceDir={newSource}
          onCancel={() => setNewSource(null)}
          onCreated={(p) => {
            setNewSource(null);
            onOpen(p.id);
          }}
        />
      )}
    </div>
  );
}

function StatusPill({ project, progress }: { project: Project; progress?: { done: number; total: number } }) {
  if (!project.sourceExists) return <span className="pill warn">Photos folder missing</span>;
  if (project.converting && progress) {
    return (
      <span className="pill busy">
        Converting {progress.done} / {progress.total}
      </span>
    );
  }
  switch (stageOf(project)) {
    case "convert":
      return <span className="pill todo">Ready to convert</span>;
    case "waiting": {
      const since = project.lastConvert ? daysSince(project.lastConvert.finishedAt) : 0;
      return (
        <span className="pill waiting">
          Waiting for client{since > 0 && ` · ${plural(since, "day")}`}
        </span>
      );
    }
    case "selected":
      return <span className="pill done">Selection ready</span>;
  }
}
