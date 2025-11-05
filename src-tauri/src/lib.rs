mod vault;

use std::fs;
use std::path::PathBuf;

use tauri::Manager;
use tauri::{AppHandle, State};
use time::macros::format_description;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::vault::{
    vault_file_path, Entry, EntryInfo, TextEncryption, UnlockResponse, VaultManager,
};

#[derive(Default)]
struct AppState {
    manager: VaultManager,
}

fn resolve_vault_path(app: &AppHandle, directory: Option<String>) -> Result<PathBuf, String> {
    let base = if let Some(dir) = directory {
        let trimmed = dir.trim();
        if trimmed.is_empty() {
            return Err("所选目录无效".to_string());
        }
        let path = PathBuf::from(trimmed);
        if path.is_file() {
            return Err("所选路径不是文件夹".to_string());
        }
        fs::create_dir_all(&path).map_err(|err| format!("无法创建所选目录: {err}"))?;
        path
    } else {
        app.path()
            .app_local_data_dir()
            .map_err(|err| format!("failed to resolve app data dir: {err}"))?
    };

    Ok(vault_file_path(base))
}

#[tauri::command]
fn unlock_vault(
    passphrase: String,
    directory: Option<String>,
    encryption: Option<TextEncryption>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<UnlockResponse, String> {
    let path = resolve_vault_path(&app, directory)?;
    state
        .manager
        .unlock(&passphrase, path, encryption)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn lock_vault(state: State<AppState>) -> Result<(), String> {
    state.manager.lock();
    Ok(())
}

#[tauri::command]
fn list_entries(state: State<AppState>) -> Result<Vec<EntryInfo>, String> {
    state.manager.list().map_err(|err| err.to_string())
}

#[tauri::command]
fn load_entry(id: Uuid, state: State<AppState>) -> Result<Entry, String> {
    state.manager.load_entry(id).map_err(|err| err.to_string())
}

#[tauri::command]
fn create_entry(
    title: Option<String>,
    content: Option<String>,
    encryption: Option<TextEncryption>,
    state: State<AppState>,
) -> Result<Entry, String> {
    let title = title.unwrap_or_else(|| "Untitled entry".to_string());
    let content = content.unwrap_or_default();
    state
        .manager
        .create_entry(&title, &content, encryption)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn update_entry(entry: Entry, state: State<AppState>) -> Result<Entry, String> {
    state
        .manager
        .update_entry(entry)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn delete_entry(id: Uuid, state: State<AppState>) -> Result<(), String> {
    state
        .manager
        .delete_entry(id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn export_plaintext(state: State<AppState>) -> Result<String, String> {
    state
        .manager
        .export_plaintext()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn store_image(path: String, state: State<AppState>) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("未选择任何文件".to_string());
    }

    let source = PathBuf::from(trimmed);
    state
        .manager
        .store_image(source)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn store_image_from_bytes(
    name: Option<String>,
    mime: Option<String>,
    data: Vec<u8>,
    state: State<AppState>,
) -> Result<String, String> {
    state
        .manager
        .store_image_bytes(name, mime, data)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn export_plaintext_file(state: State<AppState>) -> Result<String, String> {
    let content = state
        .manager
        .export_plaintext()
        .map_err(|err| err.to_string())?;

    let date_fmt = format_description!("[year]-[month]-[day]");
    let now = OffsetDateTime::now_utc();
    let suggested = format!(
        "diary-{}.md",
        now.format(&date_fmt).unwrap_or_else(|_| "today".into())
    );

    let mut export_dir = state.manager.vault_root().map_err(|err| err.to_string())?;
    export_dir.push("exports");
    fs::create_dir_all(&export_dir).map_err(|err| err.to_string())?;

    export_dir.push(suggested);
    fs::write(&export_dir, content).map_err(|err| err.to_string())?;

    Ok(export_dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn decrypt_image(path: String, state: State<AppState>) -> Result<Vec<u8>, String> {
    state
        .manager
        .decrypt_image(&path)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn change_vault_passphrase(
    old_passphrase: String,
    new_passphrase: String,
    state: State<AppState>,
) -> Result<(), String> {
    if new_passphrase.trim().is_empty() {
        return Err("新密码不能为空".to_string());
    }
    if new_passphrase.len() < 6 {
        return Err("新密码长度至少需要 6 个字符".to_string());
    }
    state
        .manager
        .change_passphrase(&old_passphrase, &new_passphrase)
        .map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            unlock_vault,
            lock_vault,
            list_entries,
            load_entry,
            create_entry,
            update_entry,
            delete_entry,
            export_plaintext,
            store_image,
            store_image_from_bytes,
            export_plaintext_file,
            decrypt_image,
            change_vault_passphrase
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
