import { useEffect, useState } from "react";
import { api, errorText, plural, Project, ProjectDraft } from "../api";
import Modal from "./Modal";

export default function NewProjectDialog({
  sourceDir,
  onCancel,
  onCreated,
}: {
  sourceDir: string;
  onCancel: () => void;
  onCreated: (p: Project) => void;
}) {
  const [draft, setDraft] = useState<ProjectDraft | null>(null);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.draftProject(sourceDir).then(
      (d) => {
        setDraft(d);
        setName(d.name);
      },
      (e) => setError(errorText(e)),
    );
  }, [sourceDir]);

  async function create() {
    setBusy(true);
    try {
      onCreated(await api.createProject(sourceDir, name));
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  }

  return (
    <Modal
      title="New project"
      onClose={onCancel}
      actions={
        <>
          <button onClick={onCancel}>Cancel</button>
          {draft && (
            <button className="primary" onClick={create} disabled={busy || !name.trim()}>
              Create project
            </button>
          )}
        </>
      }
    >
      {error && <p className="banner error">{error}</p>}
      {!draft && !error && <p className="muted">Looking for photos…</p>}
      {draft && (
        <>
          <label className="field">
            <span>Name</span>
            <input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          </label>
          <p>
            Found <b>{plural(draft.rawCount, "RAW photo")}</b>.
          </p>
          <dl className="folders">
            <dt>Photos</dt>
            <dd>{draft.sourceDir}</dd>
            <dt>JPGs will go to</dt>
            <dd>{draft.jpgDir}</dd>
            <dt>Client's picks will go to</dt>
            <dd>{draft.selectionDir}</dd>
          </dl>
        </>
      )}
    </Modal>
  );
}
