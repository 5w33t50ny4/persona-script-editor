import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { attachConsole, info, error } from "@tauri-apps/plugin-log";

interface FileInfo {
  name: string;
  index: number;
  lba: number;
  size: number;
  sub_file_count: number;
  string_count: number;
}

interface SceneInfo {
  index: number;
  size: number;
  string_count: number;
  has_strings: boolean;
}

interface StringInfo {
  index: number;
  original: string;
  translation: string;
  raw_hex: string;
}

let currentFileIndex: number | null = null;
let currentSceneIndex: number | null = null;

const btnOpen = document.getElementById("btn-open")!;
const btnExport = document.getElementById("btn-export")!;
const btnImport = document.getElementById("btn-import")!;
const btnSave = document.getElementById("btn-save") as HTMLButtonElement | null;
const statusEl = document.getElementById("status")!;
const fileList = document.getElementById("file-list")!;
const sceneList = document.getElementById("scene-list")!;
const stringsContainer = document.getElementById("strings-container")!;

btnOpen.addEventListener("click", openIso);
btnExport.addEventListener("click", exportJson);
btnImport.addEventListener("click", importJson);
if (btnSave) btnSave.addEventListener("click", saveIso);

// Attach console to receive backend logs in devtools
attachConsole();
info("[UI] Persona Script Editor started");

async function openIso() {
  const path = await open({
    filters: [{ name: "CD Image", extensions: ["bin", "img"] }],
  });
  if (!path) return;

  statusEl.textContent = "Loading...";
  info(`[UI] Opening ISO: ${path}`);
  try {
    const files: FileInfo[] = await invoke("open_iso", { path });
    renderFiles(files);
    const totalStrings = files.reduce((a, f) => a + f.string_count, 0);
    statusEl.textContent = `Loaded: ${path.split("\\").pop()} (${totalStrings} strings)`;
    info(`[UI] ISO loaded: ${files.length} files, ${totalStrings} total strings`);
    btnExport.removeAttribute("disabled");
    btnImport.removeAttribute("disabled");
    if (btnSave) btnSave.removeAttribute("disabled");
  } catch (e) {
    error(`[UI] Failed to open ISO: ${e}`);
    statusEl.textContent = `Error: ${e}`;
  }
}

async function exportJson() {
  if (currentFileIndex === null) return;
  const path = await save({
    filters: [{ name: "JSON", extensions: ["json"] }],
    defaultPath: `E${currentFileIndex - 273}_strings.json`,
  });
  if (!path) return;

  info(`[UI] Exporting file_index=${currentFileIndex} to ${path}`);
  try {
    const result: string = await invoke("export_json", {
      fileIndex: currentFileIndex,
      outputPath: path,
    });
    statusEl.textContent = result;
    info(`[UI] Export done: ${result}`);
  } catch (e) {
    error(`[UI] Export failed: ${e}`);
    statusEl.textContent = `Export error: ${e}`;
  }
}

async function importJson() {
  const path = await open({
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) return;

  info(`[UI] Importing from ${path}`);
  try {
    const result: string = await invoke("import_json", { inputPath: path });
    statusEl.textContent = result;
    info(`[UI] Import done: ${result}`);
    // Refresh current scene if loaded
    if (currentFileIndex !== null && currentSceneIndex !== null) {
      const strings: StringInfo[] = await invoke("get_strings", {
        fileIndex: currentFileIndex,
        sceneIndex: currentSceneIndex,
      });
      renderStrings(strings);
    }
  } catch (e) {
    error(`[UI] Import failed: ${e}`);
    statusEl.textContent = `Import error: ${e}`;
  }
}

async function saveIso() {
  const path = await save({
    filters: [{ name: "CD Image", extensions: ["bin"] }],
    defaultPath: "Persona_translated.bin",
  });
  if (!path) return;

  statusEl.textContent = "Saving...";
  info(`[UI] Saving ISO to ${path}`);
  try {
    const result: string = await invoke("save_iso", { outputPath: path });
    statusEl.textContent = result;
    info(`[UI] Save done: ${result}`);
  } catch (e) {
    error(`[UI] Save failed: ${e}`);
    statusEl.textContent = `Save error: ${e}`;
  }
}

function renderFiles(files: FileInfo[]) {
  fileList.innerHTML = "";
  for (const f of files) {
    const div = document.createElement("div");
    div.className = "file-item";
    div.innerHTML = `<span>${f.name}</span><span class="count">${f.string_count} str</span>`;
    div.addEventListener("click", () => selectFile(f.index, div));
    fileList.appendChild(div);
  }
}

async function selectFile(index: number, el: HTMLElement) {
  currentFileIndex = index;
  document.querySelectorAll(".file-item").forEach((e) => e.classList.remove("active"));
  el.classList.add("active");

  info(`[UI] Loading scenes for file_index=${index}`);
  const scenes: SceneInfo[] = await invoke("get_scenes", { fileIndex: index });
  info(`[UI] Got ${scenes.length} scenes (${scenes.filter((s) => s.has_strings).length} with strings)`);
  renderScenes(scenes);
  stringsContainer.innerHTML = `<p class="placeholder">Select a scene</p>`;
}

function renderScenes(scenes: SceneInfo[]) {
  sceneList.innerHTML = "";
  for (const s of scenes) {
    const div = document.createElement("div");
    div.className = `scene-item${s.has_strings ? "" : " no-strings"}`;
    div.innerHTML = `<span>S${String(s.index).padStart(3, "0")}</span><span class="count">${s.string_count}</span>`;
    if (s.has_strings) {
      div.addEventListener("click", () => selectScene(s.index, div));
    }
    sceneList.appendChild(div);
  }
}

async function selectScene(index: number, el: HTMLElement) {
  if (currentFileIndex === null) return;
  currentSceneIndex = index;
  document.querySelectorAll(".scene-item").forEach((e) => e.classList.remove("active"));
  el.classList.add("active");

  info(`[UI] Loading strings for scene S${String(index).padStart(3, "0")}`);
  const strings: StringInfo[] = await invoke("get_strings", {
    fileIndex: currentFileIndex,
    sceneIndex: index,
  });
  info(`[UI] Scene S${String(index).padStart(3, "0")}: ${strings.length} strings loaded`);
  renderStrings(strings);
}

function renderStrings(strings: StringInfo[]) {
  stringsContainer.innerHTML = "";
  if (strings.length === 0) {
    stringsContainer.innerHTML = `<p class="placeholder">No strings in this scene</p>`;
    return;
  }

  for (const s of strings) {
    const row = document.createElement("div");
    row.className = "string-row";
    row.innerHTML = `
      <div class="label">String #${s.index}</div>
      <div class="original">${highlightControls(escapeHtml(s.original))}</div>
      <textarea data-index="${s.index}" rows="3">${escapeHtml(s.translation)}</textarea>
    `;
    stringsContainer.appendChild(row);

    // Debounced save on edit
    const textarea = row.querySelector("textarea")!;
    let saveTimeout: number | undefined;
    textarea.addEventListener("input", () => {
      clearTimeout(saveTimeout);
      saveTimeout = window.setTimeout(() => {
        saveString(s.index, textarea.value);
      }, 500);
    });
  }
}

async function saveString(stringIndex: number, translation: string) {
  if (currentFileIndex === null || currentSceneIndex === null) return;
  try {
    await invoke("update_string", {
      fileIndex: currentFileIndex,
      sceneIndex: currentSceneIndex,
      stringIndex,
      translation,
    });
  } catch (e) {
    error(`[UI] Failed to save string ${stringIndex}: ${e}`);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function highlightControls(s: string): string {
  return s.replace(/\[(.*?)\]/g, '<span class="ctrl">[$1]</span>');
}
