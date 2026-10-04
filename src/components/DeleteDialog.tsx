import { useState } from "react";
import { api, errorText, plural, Project } from "../api";
import Modal from "./Modal";

export default function DeleteDialog({
  project,
  onCancel,
  onDeleted,
}: {
  project: Project;
  onCancel: () => void;
  onDeleted: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function remove() {
    setBusy(true);
    try {
      await api.deleteProject(project.id);
      onDeleted();
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  }

  return (
    <Modal
      title={`Delete “${project.name}”?`}
      onClose={busy ? undefined : onCancel}
      actions={
        <>
          <button onClick={onCancel} disabled={busy} autoFocus>
            Cancel
          </button>
          <button className="danger" onClick={remove} disabled={busy}>
            Move to Trash
          </button>
        </>
      }
    >
      <p>These folders will be moved to the Trash:</p>
      <dl className="folders">
        <dt>{plural(project.rawCount, "RAW photo")}</dt>
        <dd>{project.sourceDir}</dd>
        <dt>{plural(project.jpgCount, "JPG")}</dt>
        <dd>{project.jpgDir}</dd>
        <dt>{plural(project.selectionCount, "selected photo")}</dt>
        <dd>{project.selectionDir}</dd>
      </dl>
      <p className="muted">They stay in the Trash until you empty it, in case you change your mind.</p>
      {error && <p className="banner error">{error}</p>}
    </Modal>
  );
}
