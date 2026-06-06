#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::{error, info, warn};
use persona_script_editor_lib::{e0, encoding, iso, script};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tauri::State;

/// Translation store: file_index -> scene_index -> string_index -> translated text
type TranslationMap = HashMap<usize, HashMap<usize, HashMap<usize, String>>>;

struct AppState {
    iso: Mutex<Option<iso::IsoImage>>,
    /// Cached containers (only E0-E3, ~6MB total)
    containers: Mutex<HashMap<usize, e0::Container>>,
    table: Mutex<encoding::CharTable>,
    translations: Mutex<TranslationMap>,
}

#[derive(Serialize)]
struct FileInfo {
    name: String,
    index: usize,
    lba: u32,
    size: u32,
    sub_file_count: usize,
    string_count: usize,
}

#[derive(Serialize)]
struct SceneInfo {
    index: usize,
    size: usize,
    string_count: usize,
    has_strings: bool,
}

#[derive(Serialize)]
struct StringInfo {
    index: usize,
    original: String,
    translation: String,
    raw_hex: String,
}

#[tauri::command]
fn open_iso(path: String, state: State<AppState>) -> Result<Vec<FileInfo>, String> {
    let start = Instant::now();
    info!("Opening ISO: {}", path);

    let image = iso::IsoImage::open(std::path::Path::new(&path)).map_err(|e| {
        error!("Failed to open ISO: {}", e);
        format!("Failed to open: {}", e)
    })?;

    info!("ISO validated: {} sectors in {:?}", image.sector_count, start.elapsed());

    let text_files = image.get_text_files().map_err(|e| format!("Read error: {}", e))?;
    info!("Found {} text containers", text_files.len());

    let mut result = Vec::new();
    let mut containers = HashMap::new();

    for gf in &text_files {
        let t = Instant::now();
        let file_data = image.read_file(gf.lba, gf.size).map_err(|e| format!("Read error: {}", e))?;
        let container = e0::parse_container(&file_data);
        let mut string_count = 0;
        let mut scenes_with_strings = 0;
        for sf in &container.sub_files {
            if let Some(parsed) = script::parse_script(sf) {
                if !parsed.strings.is_empty() {
                    scenes_with_strings += 1;
                    string_count += parsed.strings.len();
                }
            }
        }
        info!("  {} : LBA={} size={} scenes={}/{} strings={} ({:?})",
            gf.name, gf.lba, gf.size, scenes_with_strings, container.sub_files.len(), string_count, t.elapsed());
        
        result.push(FileInfo {
            name: gf.name.to_string(),
            index: gf.index,
            lba: gf.lba,
            size: gf.size,
            sub_file_count: container.sub_files.len(),
            string_count,
        });
        containers.insert(gf.index, container);
    }

    let total: usize = result.iter().map(|f| f.string_count).sum();
    info!("Total: {} strings, ~{}MB in RAM. Done in {:?}", total, 
        containers.values().map(|c| c.sub_files.iter().map(|s| s.len()).sum::<usize>()).sum::<usize>() / 1024 / 1024,
        start.elapsed());

    *state.iso.lock().unwrap() = Some(image);
    *state.containers.lock().unwrap() = containers;
    state.translations.lock().unwrap().clear();

    Ok(result)
}

#[tauri::command]
fn get_scenes(file_index: usize, state: State<AppState>) -> Result<Vec<SceneInfo>, String> {
    let containers = state.containers.lock().unwrap();
    let container = containers.get(&file_index).ok_or("File not loaded")?;

    let mut scenes = Vec::new();
    for (i, sf) in container.sub_files.iter().enumerate() {
        let (string_count, has_strings) = if let Some(parsed) = script::parse_script(sf) {
            (parsed.strings.len(), !parsed.strings.is_empty())
        } else {
            (0, false)
        };
        scenes.push(SceneInfo { index: i, size: sf.len(), string_count, has_strings });
    }
    Ok(scenes)
}

#[tauri::command]
fn get_strings(file_index: usize, scene_index: usize, state: State<AppState>) -> Result<Vec<StringInfo>, String> {
    let containers = state.containers.lock().unwrap();
    let table = state.table.lock().unwrap();
    let translations = state.translations.lock().unwrap();

    let container = containers.get(&file_index).ok_or("File not loaded")?;
    let sf = container.sub_files.get(scene_index).ok_or("Scene not found")?;
    let parsed = script::parse_script(sf).ok_or("Failed to parse script")?;

    let scene_trans = translations.get(&file_index).and_then(|f| f.get(&scene_index));

    let mut result = Vec::new();
    for (i, s) in parsed.strings.iter().enumerate() {
        let elements = encoding::decode_string(&s.raw, &table);
        let original = encoding::elements_to_display(&elements);
        let raw_hex = s.raw.iter().map(|b| format!("{:02x}", b)).collect::<String>();
        let translation = scene_trans
            .and_then(|st| st.get(&i))
            .cloned()
            .unwrap_or_else(|| original.clone());
        result.push(StringInfo { index: i, original, translation, raw_hex });
    }
    Ok(result)
}

#[tauri::command]
fn update_string(file_index: usize, scene_index: usize, string_index: usize, translation: String, state: State<AppState>) -> Result<(), String> {
    state.translations.lock().unwrap()
        .entry(file_index).or_default()
        .entry(scene_index).or_default()
        .insert(string_index, translation);
    Ok(())
}

#[tauri::command]
fn export_json(file_index: usize, output_path: String, state: State<AppState>) -> Result<String, String> {
    let start = Instant::now();
    info!("export_json: file={} -> {}", file_index, output_path);

    let containers = state.containers.lock().unwrap();
    let table = state.table.lock().unwrap();
    let translations = state.translations.lock().unwrap();

    let container = containers.get(&file_index).ok_or("File not loaded")?;
    let file_name = match file_index { 273=>"E0.BIN", 274=>"E1.BIN", 275=>"E2.BIN", 276=>"E3.BIN", _=>"unknown" };
    let scene_trans = translations.get(&file_index);

    #[derive(Serialize)]
    struct ExportScene { index: usize, strings: Vec<ExportString> }
    #[derive(Serialize)]
    struct ExportString { index: usize, original: String, translation: String }
    #[derive(Serialize)]
    struct ExportFile { file_index: usize, file_name: String, scenes: Vec<ExportScene> }

    let mut scenes = Vec::new();
    for (si, sf) in container.sub_files.iter().enumerate() {
        if let Some(parsed) = script::parse_script(sf) {
            if parsed.strings.is_empty() { continue; }
            let st = scene_trans.and_then(|f| f.get(&si));
            let strings: Vec<ExportString> = parsed.strings.iter().enumerate().map(|(i, s)| {
                let elements = encoding::decode_string(&s.raw, &table);
                let original = encoding::elements_to_display(&elements);
                let translation = st.and_then(|m| m.get(&i)).cloned().unwrap_or_else(|| original.clone());
                ExportString { index: i, original, translation }
            }).collect();
            scenes.push(ExportScene { index: si, strings });
        }
    }

    let export = ExportFile { file_index, file_name: file_name.to_string(), scenes };
    let json = serde_json::to_string_pretty(&export).map_err(|e| format!("JSON error: {}", e))?;
    std::fs::write(&output_path, &json).map_err(|e| format!("Write error: {}", e))?;

    let sc = export.scenes.len();
    let stc: usize = export.scenes.iter().map(|s| s.strings.len()).sum();
    info!("Exported {} scenes, {} strings in {:?}", sc, stc, start.elapsed());
    Ok(format!("Exported {} scenes, {} strings", sc, stc))
}

#[tauri::command]
fn import_json(input_path: String, state: State<AppState>) -> Result<String, String> {
    let start = Instant::now();
    info!("import_json: {}", input_path);

    let content = std::fs::read_to_string(&input_path).map_err(|e| format!("Read error: {}", e))?;

    #[derive(Deserialize)]
    struct ImportFile { file_index: usize, scenes: Vec<ImportScene>, #[serde(default)] file_name: String }
    #[derive(Deserialize)]
    struct ImportScene { index: usize, strings: Vec<ImportString> }
    #[derive(Deserialize)]
    struct ImportString { index: usize, translation: String, #[serde(default)] original: String }

    let import: ImportFile = serde_json::from_str(&content).map_err(|e| format!("JSON error: {}", e))?;

    let mut translations = state.translations.lock().unwrap();
    let mut count = 0;
    for scene in &import.scenes {
        for s in &scene.strings {
            if s.translation != s.original && !s.translation.is_empty() {
                translations.entry(import.file_index).or_default()
                    .entry(scene.index).or_default()
                    .insert(s.index, s.translation.clone());
                count += 1;
            }
        }
    }

    info!("Imported {} translations in {:?}", count, start.elapsed());
    Ok(format!("Imported {} translations for {}", count, import.file_name))
}

#[tauri::command]
fn save_iso(output_path: String, state: State<AppState>) -> Result<String, String> {
    let start = Instant::now();
    info!("save_iso: {}", output_path);

    let iso_guard = state.iso.lock().unwrap();
    let image = iso_guard.as_ref().ok_or("No ISO loaded")?;
    let containers = state.containers.lock().unwrap();
    let table = state.table.lock().unwrap();
    let translations = state.translations.lock().unwrap();

    if translations.is_empty() {
        return Err("No translations to save".into());
    }

    // Copy original ISO to output path
    info!("Copying ISO to output...");
    std::fs::copy(&image.path, &output_path).map_err(|e| format!("Copy error: {}", e))?;

    let out_image = iso::IsoImage::open(std::path::Path::new(&output_path))
        .map_err(|e| format!("Open output error: {}", e))?;

    let mut files_modified = 0;
    let mut strings_applied = 0;

    for (&file_index, file_trans) in translations.iter() {
        let container = match containers.get(&file_index) {
            Some(c) => c,
            None => continue,
        };

        let orig_lba = out_image.read_fsect_entry(file_index).map_err(|e| format!("FSECT read: {}", e))?;
        let orig_size = out_image.get_container_size(orig_lba).map_err(|e| format!("Size read: {}", e))?;

        let mut new_container = container.clone();
        let mut modified = false;

        for (&scene_idx, scene_trans) in file_trans.iter() {
            if scene_idx >= new_container.sub_files.len() { continue; }
            let sf = &new_container.sub_files[scene_idx];
            let parsed = match script::parse_script(sf) {
                Some(p) if !p.strings.is_empty() => p,
                _ => continue,
            };

            let mut new_strings: Vec<Vec<u8>> = Vec::new();
            let mut scene_modified = false;
            for (i, orig) in parsed.strings.iter().enumerate() {
                if let Some(trans_text) = scene_trans.get(&i) {
                    new_strings.push(encoding::encode_display_string(trans_text, &table));
                    strings_applied += 1;
                    scene_modified = true;
                } else {
                    new_strings.push(orig.raw.clone());
                }
            }

            if scene_modified {
                if let Some(rebuilt) = script::rebuild_script(&parsed, &new_strings) {
                    info!("  file={} S{:03}: {} -> {} bytes", file_index, scene_idx, sf.len(), rebuilt.len());
                    new_container.sub_files[scene_idx] = rebuilt;
                    modified = true;
                } else {
                    warn!("  file={} S{:03}: rebuild FAILED", file_index, scene_idx);
                }
            }
        }

        if modified {
            let new_data = e0::build_container(&new_container);
            info!("  Container rebuilt: {} bytes (was {})", new_data.len(), orig_size);

            if new_data.len() as u32 <= orig_size {
                // Fits in-place
                out_image.write_file(orig_lba, &new_data).map_err(|e| format!("Write error: {}", e))?;
                info!("  Written in-place at LBA {}", orig_lba);
            } else {
                // Relocate: append at end, update FSECT
                let new_lba = out_image.append_file(&new_data).map_err(|e| format!("Append error: {}", e))?;
                out_image.write_fsect_entry(file_index, new_lba).map_err(|e| format!("FSECT write: {}", e))?;
                info!("  Relocated: LBA {} -> {} (FSECT updated)", orig_lba, new_lba);
            }
            files_modified += 1;
        }
    }

    let msg = format!("Saved! {} files, {} strings applied in {:?}", files_modified, strings_applied, start.elapsed());
    info!("{}", msg);
    Ok(msg)
}

fn main() {
    let mut table = encoding::CharTable::japanese();

    // Look for any *.tbl file next to the exe
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let mut tbl_loaded = false;
    if let Some(dir) = &exe_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("tbl") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        table.load_tbl(&content);
                        eprintln!("[init] Loaded TBL: {} codes from {:?}", table.code_to_char.len(), path.file_name().unwrap());
                        tbl_loaded = true;
                        break;
                    }
                }
            }
        }
    }
    if !tbl_loaded {
        eprintln!("[init] WARNING: No .tbl file found next to exe!");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState {
            iso: Mutex::new(None),
            containers: Mutex::new(HashMap::new()),
            table: Mutex::new(table),
            translations: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            open_iso,
            get_scenes,
            get_strings,
            update_string,
            export_json,
            import_json,
            save_iso,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
