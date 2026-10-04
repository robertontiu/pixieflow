import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ConvertProgress } from "./api";
import ProjectList from "./components/ProjectList";
import ProjectPage from "./components/ProjectPage";
import UpdatePrompt from "./components/UpdatePrompt";

export type Progress = Record<string, ConvertProgress>;

export default function App() {
  const [openId, setOpenId] = useState<string | null>(null);
  // Kept here so progress survives going back to the list mid-conversion.
  const [progress, setProgress] = useState<Progress>({});

  useEffect(() => {
    const unlisten = listen<ConvertProgress>("convert-progress", (e) =>
      setProgress((prev) => ({ ...prev, [e.payload.projectId]: e.payload })),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  return (
    <>
      {openId ? (
        <ProjectPage id={openId} progress={progress[openId]} onBack={() => setOpenId(null)} />
      ) : (
        <ProjectList progress={progress} onOpen={setOpenId} />
      )}
      <UpdatePrompt />
    </>
  );
}
