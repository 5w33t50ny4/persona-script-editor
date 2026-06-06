# Persona Script Editor

A GUI tool for translating the PS1 game **Megami Ibunroku Persona** (真・女神転生ペルソナ / Revelations: Persona).

Parses dialogue scripts directly from a BIN/CUE disc image, provides JSON export/import for translators, and writes translations back into the image.

Built with Rust + Tauri. Runs on Windows. Minimal RAM usage (~6MB for script data, not the full 667MB image).

---

## Quick Start

1. Place `Persona_jap.tbl` (or any `.tbl` file) next to the exe
2. Run `persona-script-editor.exe`
3. **Open ISO** → select the `.bin` disc image (Redump Rev 1, ~699MB)
4. Four files appear: E0, E1, E2, E3 — all game dialogue (~12000 strings total)
5. Click a file → scene list → click a scene → strings

---

## Translation Workflow

### For the project lead

```
Open ISO → Export JSON → distribute to translators → Import JSON → Save ISO
```

### For translators

You receive a JSON file. Open it in any text editor (VS Code, Notepad++, etc).  
Fill in the `translation` field for each string:

```json
{
  "index": 0,
  "original": "南条：やあ、[firstname]。[nl]今日もいい天気だな。[end]",
  "translation": "Nanjo: Hey, [firstname].[nl]Nice weather today.[end]"
}
```

Rules:
- **Do not remove or modify** control codes: `[nl]`, `[clear]`, `[end]`, `[firstname]`, `[lastname]`, `[choice=N]`
- `[nl]` = line break (max ~20 characters per line)
- `[clear]` = clear text box (next "page")
- `[end]` = end of dialogue
- `[choice=N]` = choice menu
- Maximum **3 lines** between `[clear]` tags (PSX text box limitation)

---

## Control Codes

| Code | Meaning |
|------|---------|
| `[nl]` | New line |
| `[clear]` | Clear text box |
| `[end]` | End dialogue |
| `[wait=N]` | Pause for N frames |
| `[color=N]` | Set text color |
| `[firstname]` | Player's first name |
| `[lastname]` | Player's last name |
| `[nickname]` | Player's nickname |
| `[choice=N]` | Choice menu (ID=N) |

---

## Game File Structure

| File | Contents | Strings |
|------|----------|---------|
| ADV/E0.BIN | Dialogue (main story) | 3878 |
| ADV/E1.BIN | Dialogue (side scenes) | 2242 |
| ADV/E2.BIN | Dialogue (side scenes) | 4632 |
| ADV/E3.BIN | Dialogue (ending) | 1380 |
| **Total** | | **~12000** |

All other files (MES, BGM, BVB, EBG, SE, B/*) contain graphics, audio, or map data. No translatable text.

---

## Technical Details

### Disc Image Format
- MODE2/2352 (raw CD-ROM XA)
- Sector: 2352 bytes total, user data at offset 24, size 2048
- Game uses internal FSECT table (LBA 634) to locate files, not ISO9660 directory

### Container Format (E0–E3)

```
[Pointer Table]  uint16_le[] × (N+1), terminated by 0x0000
[Sub-files]      aligned to 0x800 (2048) bytes
```

Each pointer value × 0x800 = byte offset of the sub-file within the container.

### Event Script Sub-file Format

```
[Header]   8 bytes: hdr_size(u16) + marker 0x8010(u16) + data(4 bytes)
[Data]     Script bytecode + string pointer table + text blob
```

Key fields in the data section (offsets relative to data start):
- `data[52]` (uint16_le) = `ett` — start of text blob area
- `data[96]` (uint16_le) = `table_ptr` — start of string pointer table (0xFFFF = no strings)

String pointer entries follow the pattern: `FF 55 00 00 [ptr_lo] [ptr_hi] 10 80`

### Text Encoding
- `0x01–0x7F` — single-byte characters (kana in JP, can be remapped to Cyrillic/Latin via font)
- `0x80xx` — two-byte characters (kanji, punctuation). Mapped via `.tbl` file
- `0xFF xx` — control codes (see table above)
- `0xFF 0x01` — string terminator

### TBL File

Plain text mapping of byte codes to Unicode characters. Format: `XXYY=char` per line.

Example:
```
80A5=。
80CB=：
80D0=？
80D1=！
80D5=、
```

The editor auto-loads any `.tbl` file found in the same directory as the executable.

---

## Memory Usage

The editor does **not** load the entire disc image into RAM. It reads sectors on demand via file seek. Only the parsed script containers (~6MB total) are kept in memory.

---

## Saving

When saving a translated ISO:
- If the rebuilt container fits in its original location → written in-place
- If the container grew (longer translations) → appended to end of image, FSECT table updated automatically

---

## Logs

Log file: `%LOCALAPPDATA%\com.persona.script-editor\logs\Persona Script Editor.log`

Logs are also output to:
- stdout (when launched from terminal)
- DevTools console (F12 inside the app window)

---

## Building from Source

```bash
cd persona-script-editor
npm install
npx tauri build
```

Requirements: Rust 1.70+, Node 18+, npm, Windows (MSVC toolchain).

Output:
- `src-tauri/target/release/persona-script-editor.exe` — portable exe
- `src-tauri/target/release/bundle/nsis/Persona Script Editor_0.1.0_x64-setup.exe` — installer

**Important:** Place any `.tbl` file (e.g. `Persona_jap.tbl`) in the same folder as the exe before running.

> **Note:** Do not use `cargo build --release` directly — it builds only the Rust backend without the frontend.  
> Always use `npx tauri build` to get a working executable with the UI embedded.

---

## License

MIT
