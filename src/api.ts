import { invoke } from "@tauri-apps/api/core";

export interface ConvertSummary {
  finishedAt: number;
  converted: number;
  alreadyDone: number;
  failed: string[];
  duplicates: string[];
}

export interface SelectionSummary {
  appliedAt: number;
  csvFile: string;
  collection: string | null;
  requested: number;
  copied: number;
  alreadyThere: number;
  notesWritten: number;
  missing: string[];
}

export interface Project {
  id: string;
  name: string;
  sourceDir: string;
  jpgDir: string;
  selectionDir: string;
  createdAt: number;
  lastConvert: ConvertSummary | null;
  lastSelection: SelectionSummary | null;
  sourceExists: boolean;
  rawCount: number;
  pendingConvert: number;
  jpgCount: number;
  selectionCount: number;
  converting: boolean;
}

export interface ProjectDraft {
  name: string;
  sourceDir: string;
  jpgDir: string;
  selectionDir: string;
  rawCount: number;
}

export interface ConvertProgress {
  projectId: string;
  done: number;
  total: number;
}

export type Stage = "convert" | "waiting" | "selected";

export function stageOf(p: Project): Stage {
  if (p.lastSelection) return "selected";
  if (p.jpgCount > 0 && p.pendingConvert === 0) return "waiting";
  return "convert";
}

export const PIXIESET_URL = "https://galleries.pixieset.com/collections";

export const api = {
  listProjects: () => invoke<Project[]>("list_projects"),
  getProject: (id: string) => invoke<Project>("get_project", { id }),
  draftProject: (sourceDir: string) => invoke<ProjectDraft>("draft_project", { sourceDir }),
  createProject: (sourceDir: string, name: string) => invoke<Project>("create_project", { sourceDir, name }),
  renameProject: (id: string, name: string) => invoke<void>("rename_project", { id, name }),
  convertProject: (id: string) => invoke<ConvertSummary>("convert_project", { id }),
  applySelection: (id: string, csvPath: string) => invoke<SelectionSummary>("apply_selection", { id, csvPath }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong.";
}

export function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

export function daysSince(ms: number): number {
  return Math.floor((Date.now() - ms) / 86_400_000);
}
