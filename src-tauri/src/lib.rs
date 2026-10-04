mod background;
mod convert;
mod files;
mod imageio;
mod project;
mod selection;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

use convert::{ConvertProgress, ConvertSummary};
use files::plural;
use project::{Project, ProjectDraft, ProjectView, Store};
use selection::SelectionSummary;

struct AppState {
    store: Mutex<Store>,
    converting: Mutex<HashSet<String>>,
    applying: Mutex<HashSet<String>>,
}

impl AppState {
    fn busy(&self) -> bool {
        !self.converting.lock().unwrap().is_empty() || !self.applying.lock().unwrap().is_empty()
    }

    fn view(&self, project: Project) -> ProjectView {
        let converting = self.converting.lock().unwrap().contains(&project.id);
        ProjectView::new(project, converting)
    }

    fn project(&self, id: &str) -> Result<Project, String> {
        self.store.lock().unwrap().get(id).cloned()
    }
}

#[tauri::command]
fn list_projects(state: State<AppState>) -> Vec<ProjectView> {
    let mut projects = state.store.lock().unwrap().projects.clone();
    projects.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    projects.into_iter().map(|p| state.view(p)).collect()
}

#[tauri::command]
fn get_project(state: State<AppState>, id: String) -> Result<ProjectView, String> {
    Ok(state.view(state.project(&id)?))
}

#[tauri::command]
fn draft_project(state: State<AppState>, source_dir: PathBuf) -> Result<ProjectDraft, String> {
    state.store.lock().unwrap().draft(&source_dir)
}

#[tauri::command]
fn create_project(state: State<AppState>, source_dir: PathBuf, name: String) -> Result<ProjectView, String> {
    let mut store = state.store.lock().unwrap();
    let draft = store.draft(&source_dir)?;
    let name = name.trim();
    let project = Project {
        id: format!("{:x}", project::now_millis()),
        name: if name.is_empty() { draft.name } else { name.to_string() },
        source_dir: draft.source_dir,
        jpg_dir: draft.jpg_dir,
        selection_dir: draft.selection_dir,
        created_at: project::now_millis(),
        last_convert: None,
        last_selection: None,
    };
    store.projects.push(project.clone());
    store.save()?;
    drop(store);
    Ok(state.view(project))
}

#[tauri::command]
fn rename_project(state: State<AppState>, id: String, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The name can't be empty.".into());
    }
    let mut store = state.store.lock().unwrap();
    store.get_mut(&id)?.name = name.to_string();
    store.save()
}

#[tauri::command]
async fn convert_project(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<ConvertSummary, String> {
    let project = state.project(&id)?;
    if !state.converting.lock().unwrap().insert(id.clone()) {
        return Err("This project is already being converted.".into());
    }
    let (name, project_id, emitter) = (project.name.clone(), id.clone(), app.clone());
    let result = tauri::async_runtime::spawn_blocking(move || {
        convert::convert(&project.source_dir, &project.jpg_dir, |done, total| {
            let progress = ConvertProgress { project_id: project_id.clone(), done, total };
            let _ = emitter.emit("convert-progress", progress);
        })
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r)
    .and_then(|summary| {
        let mut store = state.store.lock().unwrap();
        store.get_mut(&id)?.last_convert = Some(summary.clone());
        store.save()?;
        Ok(summary)
    });
    state.converting.lock().unwrap().remove(&id);

    match &result {
        Ok(s) => {
            let mut body = format!("{name}: {} ready to upload to Pixieset.", plural(s.converted + s.already_done, "photo"));
            if !s.failed.is_empty() {
                body += &format!(" {} couldn't be converted.", plural(s.failed.len(), "photo"));
            }
            background::job_finished(&app, "Photos converted", &body);
        }
        Err(e) => background::job_finished(&app, "Conversion stopped", &format!("{name}: {e}")),
    }
    result
}

#[tauri::command]
async fn apply_selection(app: AppHandle, state: State<'_, AppState>, id: String, csv_path: PathBuf) -> Result<SelectionSummary, String> {
    let project = state.project(&id)?;
    if !state.applying.lock().unwrap().insert(id.clone()) {
        return Err("This selection is already being copied.".into());
    }
    let name = project.name.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        selection::apply(&project.source_dir, &project.selection_dir, &csv_path)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r)
    .and_then(|summary| {
        let mut store = state.store.lock().unwrap();
        store.get_mut(&id)?.last_selection = Some(summary.clone());
        store.save()?;
        Ok(summary)
    });
    state.applying.lock().unwrap().remove(&id);

    match &result {
        Ok(s) => {
            let body = format!("{name}: {} in the selection folder.", plural(s.copied + s.already_there, "photo"));
            background::job_finished(&app, "Client selection ready", &body);
        }
        Err(e) => background::job_finished(&app, "Couldn't copy the selection", &format!("{name}: {e}")),
    }
    result
}

/// Moves the RAW, JPG and selection folders to the Trash and forgets the project.
#[tauri::command]
fn delete_project(state: State<AppState>, id: String) -> Result<(), String> {
    if state.converting.lock().unwrap().contains(&id) || state.applying.lock().unwrap().contains(&id) {
        return Err("Wait for the conversion or copy to finish before deleting the project.".into());
    }
    let mut store = state.store.lock().unwrap();
    let project = store.get(&id)?.clone();
    let folders: Vec<_> = [&project.source_dir, &project.jpg_dir, &project.selection_dir]
        .into_iter()
        .filter(|d| d.exists())
        .collect();
    if !folders.is_empty() {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut trash = trash::TrashContext::default();
        // The Finder method would ask her to let Pixieflow "control Finder".
        trash.set_delete_method(DeleteMethod::NsFileManager);
        trash.delete_all(folders).map_err(|e| format!("Couldn't move the folders to the Trash: {e}"))?;
    }
    store.projects.retain(|p| p.id != id);
    store.save()
}

#[tauri::command]
fn open_folder(path: PathBuf) -> Result<(), String> {
    if !path.is_dir() {
        return Err("That folder doesn't exist yet.".into());
    }
    open(path.as_os_str())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Only https links can be opened.".into());
    }
    open(url.as_ref())
}

fn open(target: &std::ffi::OsStr) -> Result<(), String> {
    Command::new("/usr/bin/open").arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .menu(background::menu)
        .on_menu_event(background::on_menu_event)
        .on_window_event(background::on_window_event)
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("projects.json");
            app.manage(AppState {
                store: Mutex::new(Store::load(path)),
                converting: Mutex::default(),
                applying: Mutex::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            get_project,
            draft_project,
            create_project,
            rename_project,
            convert_project,
            apply_selection,
            delete_project,
            open_folder,
            open_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Pixieflow")
        .run(background::on_run_event);
}
