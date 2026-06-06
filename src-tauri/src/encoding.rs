//! Text encoding/decoding for Persona scripts
//!
//! Original Japanese encoding:
//! - Single byte 0x01-0x7F: kana characters (font glyph index)
//! - Two bytes 0x80xx: kanji/punctuation (TBL lookup)
//! - 0xFF xx: control codes (newline, clear, wait, etc.)
//!
//! For Russian translation, single bytes 0x01-0x43 are remapped to Cyrillic.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// FF-prefix control code
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlCode {
    End,                // FF 01 (string terminator)
    EndDialogue,       // FF 02
    Newline,           // FF 03
    Clear,             // FF 04
    Wait(u8),          // FF 05 XX 00
    Color(u8),         // FF 06 XX
    FirstName,         // FF 07
    Nickname,          // FF 08
    Choice(u8),        // FF 0E XX
    LastName,          // FF 0F
    Unknown(u8),       // FF XX (anything else)
}

/// A decoded text element (either a character or control code)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextElement {
    /// A displayable character (decoded to Unicode)
    Char(char),
    /// A raw two-byte code that couldn't be decoded
    RawCode(u8, u8),
    /// A control code
    Control(ControlCode),
}

/// Character table mapping two-byte codes to Unicode characters
#[derive(Debug, Clone)]
pub struct CharTable {
    /// Map from 2-byte code (big-endian u16) to character
    pub code_to_char: HashMap<u16, char>,
    /// Map from character to 2-byte code
    pub char_to_code: HashMap<char, u16>,
    /// Single-byte glyph table (index -> char)
    pub single_byte: HashMap<u8, char>,
    /// Reverse single-byte (char -> index)
    pub single_byte_rev: HashMap<char, u8>,
}

/// Standard Japanese kana table (single-byte 0x01-0x7F)
const KANA: &[char] = &[
    '\0', 'あ','い','う','え','お','か','き','く','け','こ',
    'さ','し','す','せ','そ','た','ち','つ','て','と',
    'な','に','ぬ','ね','の','は','ひ','ふ','へ','ほ',
    'ま','み','む','め','も','や','ゆ','よ','ら','り',
    'る','れ','ろ','わ','を','ん',
    'ア','イ','ウ','エ','オ','カ','キ','ク','ケ','コ',
    'サ','シ','ス','セ','ソ','タ','チ','ツ','テ','ト',
    'ナ','ニ','ヌ','ネ','ノ','ハ','ヒ','フ','ヘ','ホ',
    'マ','ミ','ム','メ','モ','ヤ','ユ','ヨ','ラ','リ',
    'ル','レ','ロ','ワ','ヲ','ン',
    'ガ','ギ','グ','ゲ','ゴ','ザ','ジ','ズ','ゼ','ゾ',
    'ダ','ヂ','ヅ','デ','ド','バ','ビ','ブ','ベ','ボ',
    'パ','ピ','プ','ペ','ポ','ァ','ィ','ゥ','ェ','ォ',
    'ャ','ュ','ョ','ッ','。',
    // 0x7F = 。 is the last single-byte kana (0x01-0x7F range)
    // 0x80+ are always first byte of two-byte codes (via TBL)
];

/// Russian Cyrillic single-byte mapping (0x01-0x43)
const CYRILLIC: &str = "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя";

impl CharTable {
    /// Create a table for original Japanese encoding
    pub fn japanese() -> Self {
        let mut single_byte = HashMap::new();
        let mut single_byte_rev = HashMap::new();
        for (i, &ch) in KANA.iter().enumerate().skip(1) {
            if ch != '\0' {
                single_byte.insert(i as u8, ch);
                single_byte_rev.insert(ch, i as u8);
            }
        }
        CharTable {
            code_to_char: HashMap::new(),
            char_to_code: HashMap::new(),
            single_byte,
            single_byte_rev,
        }
    }

    /// Create a table for Russian Cyrillic encoding
    pub fn russian() -> Self {
        let mut single_byte = HashMap::new();
        let mut single_byte_rev = HashMap::new();
        for (i, ch) in CYRILLIC.chars().enumerate() {
            let code = (i + 1) as u8;
            single_byte.insert(code, ch);
            single_byte_rev.insert(ch, code);
        }
        // Space = 0x43 (index 67)
        let space_code = (CYRILLIC.chars().count() + 1) as u8;
        single_byte.insert(space_code, ' ');
        single_byte_rev.insert(' ', space_code);

        CharTable {
            code_to_char: HashMap::new(),
            char_to_code: HashMap::new(),
            single_byte,
            single_byte_rev,
        }
    }

    /// Load a TBL file to populate two-byte mappings
    pub fn load_tbl(&mut self, tbl_content: &str) {
        for line in tbl_content.lines() {
            let line = line.trim();
            if let Some((hex_part, char_part)) = line.split_once('=') {
                if let Ok(code) = u16::from_str_radix(hex_part.trim(), 16) {
                    if let Some(ch) = char_part.chars().next() {
                        self.code_to_char.insert(code, ch);
                        self.char_to_code.entry(ch).or_insert(code);
                    }
                }
            }
        }
    }
}

/// Decode raw script bytes into text elements
pub fn decode_string(raw: &[u8], table: &CharTable) -> Vec<TextElement> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == 0xFF {
            // Control code
            if i + 1 >= raw.len() {
                break;
            }
            let cmd = raw[i + 1];
            let ctrl = match cmd {
                0x01 => { i += 2; ControlCode::End }
                0x02 => { i += 2; ControlCode::EndDialogue }
                0x03 => { i += 2; ControlCode::Newline }
                0x04 => { i += 2; ControlCode::Clear }
                0x05 => {
                    let val = if i + 2 < raw.len() { raw[i + 2] } else { 0 };
                    i += 4; // FF 05 XX 00
                    ControlCode::Wait(val)
                }
                0x06 => {
                    let val = if i + 2 < raw.len() { raw[i + 2] } else { 0 };
                    i += 3;
                    ControlCode::Color(val)
                }
                0x07 => { i += 2; ControlCode::FirstName }
                0x08 => { i += 2; ControlCode::Nickname }
                0x0E => {
                    let val = if i + 2 < raw.len() { raw[i + 2] } else { 0 };
                    i += 3;
                    ControlCode::Choice(val)
                }
                0x0F => { i += 2; ControlCode::LastName }
                _ => { i += 2; ControlCode::Unknown(cmd) }
            };
            result.push(TextElement::Control(ctrl));
        } else if b >= 0x80 {
            // Two-byte character (kanji, punctuation via TBL)
            if i + 1 >= raw.len() {
                break;
            }
            let hi = b;
            let lo = raw[i + 1];
            let code = ((hi as u16) << 8) | (lo as u16);
            if let Some(&ch) = table.code_to_char.get(&code) {
                result.push(TextElement::Char(ch));
            } else {
                result.push(TextElement::RawCode(hi, lo));
            }
            i += 2;
        } else if b >= 0x01 {
            // Single-byte character (0x01-0x7F kana only)
            if let Some(&ch) = table.single_byte.get(&b) {
                result.push(TextElement::Char(ch));
            } else {
                result.push(TextElement::RawCode(0x00, b));
            }
            i += 1;
        } else {
            // Null byte - skip
            i += 1;
        }
    }
    result
}

/// Convert text elements back to a display string (for the editor UI)
pub fn elements_to_display(elements: &[TextElement]) -> String {
    let mut s = String::new();
    for el in elements {
        match el {
            TextElement::Char(ch) => s.push(*ch),
            TextElement::RawCode(hi, lo) => {
                s.push_str(&format!("|{:02X}{:02X}", hi, lo));
            }
            TextElement::Control(ctrl) => {
                match ctrl {
                    ControlCode::End => s.push_str("[end]"),
                    ControlCode::EndDialogue => {} // don't show terminator
                    ControlCode::Newline => s.push_str("[nl]"),
                    ControlCode::Clear => s.push_str("[clear]"),
                    ControlCode::Wait(n) => s.push_str(&format!("[wait={}]", n)),
                    ControlCode::Color(n) => s.push_str(&format!("[color={}]", n)),
                    ControlCode::FirstName => s.push_str("[firstname]"),
                    ControlCode::Nickname => s.push_str("[nickname]"),
                    ControlCode::Choice(n) => s.push_str(&format!("[choice={}]", n)),
                    ControlCode::LastName => s.push_str("[lastname]"),
                    ControlCode::Unknown(n) => s.push_str(&format!("[ff{:02x}]", n)),
                }
            }
        }
    }
    s
}

/// Encode a display string (with [nl], [clear], etc.) back to raw script bytes.
/// This is the inverse of decode_string + elements_to_display.
pub fn encode_display_string(text: &str, table: &CharTable) -> Vec<u8> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '[' {
            // Parse control code tag
            if let Some(end) = chars[i..].iter().position(|&c| c == ']') {
                let tag: String = chars[i + 1..i + end].iter().collect();
                let encoded = encode_control_tag(&tag);
                out.extend_from_slice(&encoded);
                i += end + 1;
                continue;
            }
        }

        if ch == '|' && i + 4 < chars.len() {
            // Raw hex code |XXYY
            let hex: String = chars[i + 1..i + 5].iter().collect();
            if let Ok(val) = u16::from_str_radix(&hex, 16) {
                if val <= 0xFF {
                    out.push(val as u8);
                } else {
                    out.push((val >> 8) as u8);
                    out.push((val & 0xFF) as u8);
                }
                i += 5;
                continue;
            }
        }

        // Try single-byte encoding first
        if let Some(&code) = table.single_byte_rev.get(&ch) {
            out.push(code);
        } else if let Some(&code) = table.char_to_code.get(&ch) {
            // Two-byte encoding
            out.push((code >> 8) as u8);
            out.push((code & 0xFF) as u8);
        } else {
            // Unknown character - skip with warning
            log::warn!("encode_display_string: unknown char '{}' (U+{:04X}), skipping", ch, ch as u32);
        }

        i += 1;
    }

    // Ensure string ends with FF 01
    if !out.ends_with(&[0xFF, 0x01]) {
        out.push(0xFF);
        out.push(0x01);
    }

    out
}

/// Encode a single control tag like "nl", "clear", "wait=32", "choice=16"
fn encode_control_tag(tag: &str) -> Vec<u8> {
    let lower = tag.to_lowercase();

    // Check for tags with values: "wait=X", "color=X", "choice=X"
    if let Some(val_str) = lower.strip_prefix("wait=") {
        let val: u8 = val_str.parse().unwrap_or(32);
        return vec![0xFF, 0x05, val, 0x00];
    }
    if let Some(val_str) = lower.strip_prefix("color=") {
        let val: u8 = val_str.parse().unwrap_or(0);
        return vec![0xFF, 0x06, val];
    }
    if let Some(val_str) = lower.strip_prefix("choice=") {
        let val: u8 = val_str.parse().unwrap_or(0);
        return vec![0xFF, 0x0E, val];
    }

    // Check for ff## pattern (raw FF code)
    if lower.starts_with("ff") && lower.len() == 4 {
        if let Ok(val) = u8::from_str_radix(&lower[2..], 16) {
            return vec![0xFF, val];
        }
    }

    match lower.as_str() {
        "end" => vec![0xFF, 0x02],
        "nl" => vec![0xFF, 0x03],
        "clear" => vec![0xFF, 0x04],
        "firstname" => vec![0xFF, 0x07],
        "nickname" => vec![0xFF, 0x08],
        "lastname" => vec![0xFF, 0x0F],
        "close" => vec![0xFF, 0x01], // close = terminator
        _ => {
            log::warn!("encode_control_tag: unknown tag [{}]", tag);
            vec![]
        }
    }
}
