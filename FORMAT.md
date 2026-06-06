# Megami Ibunroku Persona — Event Script Format Documentation

## Overview

The game stores event/dialogue scripts inside `ADV\E0.BIN` on the CD.
This file is a **container** holding 224 sub-files (individual event scripts).

---

## 1. CD Image (BIN/CUE)

- Format: MODE2/2352 (raw CD-ROM XA)
- Sector size: 2352 bytes
- User data offset: 24 bytes from sector start
- User data size: 2048 bytes per sector

The game uses internal file tables (not ISO9660) to locate files:
- **FSECT** (LBA 634): array of uint32_le, each entry = starting LBA of a game file
- **FSIZE** (LBA 623): array of uint32_le, each entry = size in bytes
- E0.BIN is at index **273** in both tables

---

## 2. E0 Container Format

```
┌─────────────────────────────────────────────┐
│ Pointer Table (variable length)             │
│   uint16_le[N+1] pointers                   │
│   Terminated by 0x0000                      │
├─────────────────────────────────────────────┤
│ Padding to 0x800 alignment                  │
├─────────────────────────────────────────────┤
│ Sub-file 0 (aligned to 0x800)               │
├─────────────────────────────────────────────┤
│ Sub-file 1 (aligned to 0x800)               │
├─────────────────────────────────────────────┤
│ ...                                         │
├─────────────────────────────────────────────┤
│ Sub-file N-1                                │
└─────────────────────────────────────────────┘
```

### Pointer Table

| Offset | Type      | Description                                        |
|--------|-----------|----------------------------------------------------|
| 0      | uint16_le | Pointer to sub-file 0 (in 0x800-byte units)        |
| 2      | uint16_le | Pointer to sub-file 1                              |
| ...    | ...       | ...                                                |
| N*2    | uint16_le | End-of-data pointer (marks end of last sub-file)   |
| (N+1)*2| uint16_le | 0x0000 (terminator)                                |

- Original E0 has **225 entries** (224 sub-files + 1 end marker)
- To get byte offset: `pointer_value * 0x800`
- Sub-file N size: `(ptr[N+1] - ptr[N]) * 0x800`

---

## 3. Event Script Sub-file Format

Each sub-file has:

```
┌─────────────────────────────────────────────┐
│ Sub-file Header (8 bytes typically)          │
├─────────────────────────────────────────────┤
│ Script Data Section                          │
│   ├── Event Command Table (offsets 0x00-0x60+)│
│   ├── Script Bytecode                        │
│   ├── String Pointer Table                   │
│   ├── Text Data (strings)                    │
│   └── Trailing Data                          │
└─────────────────────────────────────────────┘
```

### Sub-file Header

| Offset | Type      | Value   | Description              |
|--------|-----------|---------|--------------------------|
| 0      | uint16_le | 8       | Header size in bytes     |
| 2      | uint16_le | 0x8010  | Marker (always 10 80)    |
| 4      | uint16_le | varies  | (purpose TBD)            |
| 6      | uint16_le | 0x8010  | Marker (always 10 80)    |

### Script Data Section

The data section starts immediately after the header (`offset = hdr_size`).
All offsets below are relative to the start of the data section.

#### Key Fields

| Offset | Type      | Name       | Description                                      |
|--------|-----------|------------|--------------------------------------------------|
| 52     | uint16_le | ett        | End of text table / start of text blob area      |
| 96     | uint16_le | table_ptr  | Offset to string pointer table within data       |

If `table_ptr == 0xFFFF`, the sub-file has no dialogue strings.

#### String Pointer Table

Located at `data[table_ptr]`. Entries follow this pattern:

```
FF 55 00 00 [ptr_lo] [ptr_hi] 10 80
```

- `FF 55 00 00` = prefix marker (4 bytes before the pointer)
- `ptr_lo, ptr_hi` = uint16_le offset within data section pointing to string start
- `10 80` = suffix marker

The table continues until a different pattern is encountered.
To find all strings, scan from `table_ptr` looking for `FF 55 00 00 XX XX 10 80` sequences.

---

## 4. Text Encoding

### Single-byte characters (0x01-0x7F)

Originally mapped to Japanese kana. Values 0x01-0x7F are indices into the font file.

| Range     | Original Content         |
|-----------|--------------------------|
| 0x01-0x2E | Hiragana (あ-ん)         |
| 0x2F-0x5E | Katakana (ア-ン)         |
| 0x5F-0x7F | Extended katakana + marks |

For Russian translation, these are remapped to Cyrillic:
| Range     | Content                  |
|-----------|--------------------------|
| 0x01-0x42 | А-я (66 letters + Ё/ё)  |
| 0x43      | Space                    |

### Two-byte characters (0x80xx)

High byte 0x80+ triggers a two-byte read. The pair forms a code looked up in the TBL file.

Common punctuation codes:
| Code   | Character |
|--------|-----------|
| 80 A5  | .  (period)  |
| 80 CB  | :  (colon)   |
| 80 CC  | 　 (JP space) |
| 80 D0  | ?  (question)|
| 80 D1  | !  (exclaim) |
| 80 D5  | ,  (comma)   |
| 80 E1  | -  (dash)    |

### Control codes (0xFF xx)

| Code        | Parameters    | Name         | Description                        |
|-------------|---------------|--------------|------------------------------------|
| FF 00       | none          | [ff00]       | Unknown/NOP                        |
| FF 01       | none          | (terminator) | End of string                      |
| FF 02       | none          | [end]        | End of dialogue sequence           |
| FF 03       | none          | [nl]         | Newline                            |
| FF 04       | none          | [clear]      | Clear text box                     |
| FF 05 XX 00 | 1 byte + 00   | [wait=XX]    | Pause for XX frames                |
| FF 06 XX    | 1 byte        | [color=XX]   | Set text color                     |
| FF 07       | none          | [firstname]  | Insert player's first name         |
| FF 08       | none          | [nickname]   | Insert player's nickname           |
| FF 09       | none          | [ff09]       | Unknown                            |
| FF 0A       | none          | [ff0a]       | Unknown                            |
| FF 0B       | none          | [ff0b]       | Unknown                            |
| FF 0C       | none          | [ff0c]       | Unknown                            |
| FF 0D       | none          | [ff0d]       | Unknown                            |
| FF 0E XX    | 1 byte        | [choice=XX]  | Display choice menu (XX = menu ID) |
| FF 0F       | none          | [lastname]   | Insert player's last name          |

---

## 5. Font File

- Located at game file index **5** in FSECT/FSIZE
- Size: 65536 bytes
- Glyph size: 32 bytes each (16x16 pixels, 1bpp, 2 bytes per row)
- Glyph index N is at byte offset `N * 32`
- Indices 0x01-0x7F correspond to single-byte text codes
- Indices 0x80+ correspond to two-byte text code second bytes (TBL lookup)

---

## 6. TBL File (Persona_jap.tbl)

Plain text, UTF-8, format: `XXXX=char` per line where XXXX is hex code.

Example:
```
8090=漢
80A5=。
80CB=：
80CC=　
80D0=？
80D1=！
80D5=、
```

The hex code is the 2-byte value as it appears in the script (big-endian in the TBL, but stored little-endian in the binary when the injector tool writes it).

---

## 7. Workflow for Translation

1. **Unpack**: Read ISO → extract E0 → parse pointer table → for each sub-file, parse strings
2. **Edit**: Modify string text (respecting encoding constraints and control codes)
3. **Repack**: Encode strings → rebuild sub-file (update string pointers) → rebuild E0 container → patch ISO

### Constraints when editing:
- New text blob must fit within `sub-file_size - ett` bytes (or sub-file must be resized)
- String pointer table positions are fixed (they're part of the script bytecode)
- Only the text area (after `ett`) is safe to resize
- If sub-file grows, the E0 container must be relocated in the ISO

---

## 8. Statistics (Original Japanese Rev 1)

- E0 container: 1,828,864 bytes (893 sectors)
- Sub-files: 224
- Sub-file sizes: 6,144 - 34,816 bytes
- Total dialogue strings: ~2,500 across all sub-files
- Sub-files with no strings (table_ptr=0xFFFF): ~27
