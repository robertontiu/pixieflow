import { useEffect, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errorText } from "../api";
import Modal from "./Modal";

type State =
  | { kind: "idle" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; percent: number | null }
  | { kind: "upToDate" }
  | { kind: "failed"; message: string };

// Lets the "Check for updates" link reuse the one prompt mounted in App.
let requestCheck: (manual: boolean) => void = () => {};
export const checkForUpdates = (manual: boolean) => requestCheck(manual);

export default function UpdatePrompt() {
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    requestCheck = async (manual) => {
      try {
        const update = await check();
        if (update) setState({ kind: "available", update });
        else if (manual) setState({ kind: "upToDate" });
      } catch (e) {
        // Being offline at launch isn't worth interrupting her for.
        if (manual) setState({ kind: "failed", message: errorText(e) });
      }
    };
    requestCheck(false);
  }, []);

  async function install(update: Update) {
    let total = 0;
    let received = 0;
    setState({ kind: "downloading", percent: null });
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          received += e.data.chunkLength;
          setState({ kind: "downloading", percent: total ? Math.round((received / total) * 100) : null });
        }
      });
      await relaunch();
    } catch (e) {
      setState({ kind: "failed", message: errorText(e) });
    }
  }

  const close = () => setState({ kind: "idle" });

  switch (state.kind) {
    case "idle":
      return null;
    case "available":
      return (
        <Modal
          title="A new version of Pixieflow is available"
          onClose={close}
          actions={
            <>
              <button onClick={close}>Later</button>
              <button className="primary" onClick={() => install(state.update)} autoFocus>
                Update now
              </button>
            </>
          }
        >
          <p>
            Version {state.update.version} is ready (you have {state.update.currentVersion}). Pixieflow will restart
            after updating — your projects are kept.
          </p>
          {state.update.body && <p className="muted pre">{state.update.body}</p>}
        </Modal>
      );
    case "downloading":
      return (
        <Modal title="Updating Pixieflow…" actions={null}>
          <div className="progress-track">
            <div className="progress-fill" style={{ width: `${state.percent ?? 100}%` }} />
          </div>
        </Modal>
      );
    case "upToDate":
      return (
        <Modal title="You're up to date" onClose={close} actions={<button onClick={close} autoFocus>OK</button>}>
          <p>You have the latest version of Pixieflow.</p>
        </Modal>
      );
    case "failed":
      return (
        <Modal title="Couldn't check for updates" onClose={close} actions={<button onClick={close} autoFocus>OK</button>}>
          <p>Check your internet connection and try again later.</p>
          <p className="muted">{state.message}</p>
        </Modal>
      );
  }
}
