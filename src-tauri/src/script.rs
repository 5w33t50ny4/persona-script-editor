//! Event script parser - extracts and rebuilds dialogue strings
//!
//! Sub-file internal structure:
//! - Header: 8 bytes (hdr_size=8, marker=0x8010, varies, 0x8010)
//! - Data section (everything after header):
//!   - Script bytecode and command tables
//!   - f[52]: uint16_le = ett (end of text table / start of text blob)
//!   - f[96]: uint16_le = table_ptr (string pointer table offset, 0xFFFF = no strings)
//!   - String pointer table entries: pattern FF 55 00 00 [ptr_lo ptr_hi] 10 80
//!   - Text blob: strings terminated by FF 01

use log::{debug, trace, warn};
use serde::{Deserialize, Serialize};

/// A single dialogue string extracted from a script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptString {
    /// Offset of the pointer in the string pointer table (relative to data section)
    pub ptr_table_offset: usize,
    /// Offset of the actual text data (relative to data section)  
    pub text_offset: usize,
    /// Raw encoded bytes of the string (including FF 01 terminator)
    pub raw: Vec<u8>,
}

/// Parsed event script sub-file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventScript {
    /// Header size in bytes (typically 8)
    pub header_size: usize,
    /// Raw header bytes
    pub header: Vec<u8>,
    /// End-of-text-table offset (start of text blob area) within data section
    pub ett: usize,
    /// String pointer table offset within data section (0xFFFF = no strings)
    pub table_ptr: usize,
    /// Extracted dialogue strings
    pub strings: Vec<ScriptString>,
    /// Full raw data section (everything after header)
    pub data: Vec<u8>,
}

/// Parse an event script sub-file
pub fn parse_script(sub_file: &[u8]) -> Option<EventScript> {
    if sub_file.len() < 108 {
        trace!("parse_script: sub_file too small ({} bytes)", sub_file.len());
        return None;
    }

    let header_size = u16::from_le_bytes([sub_file[0], sub_file[1]]) as usize;
    if header_size < 4 || header_size > 256 || header_size >= sub_file.len() {
        warn!("parse_script: invalid header_size={} (file_len={})", header_size, sub_file.len());
        return None;
    }

    let header = sub_file[..header_size].to_vec();
    let data = sub_file[header_size..].to_vec();

    if data.len() < 100 {
        trace!("parse_script: data section too small ({} bytes)", data.len());
        return None;
    }

    let ett = u16::from_le_bytes([data[52], data[53]]) as usize;
    let table_ptr = u16::from_le_bytes([data[96], data[97]]) as usize;

    debug!("parse_script: size={} hdr={} data={} ett=0x{:04x} table_ptr=0x{:04x}", 
        sub_file.len(), header_size, data.len(), ett, table_ptr);

    // Scan for string pointers using FF 55 00 00 [lo] [hi] 10 80 pattern.
    // If table_ptr is valid, start scanning from there.
    // If table_ptr is 0xFFFF, do a full scan of the data section (fallback).
    let scan_start = if table_ptr != 0xFFFF && table_ptr < data.len() {
        table_ptr
    } else {
        debug!("parse_script: table_ptr=0x{:04x}, using full scan fallback", table_ptr);
        4 // start from beginning (need at least 4 bytes for prefix)
    };

    let mut strings = Vec::new();
    let mut i = scan_start;
    while i + 4 <= data.len() {
        // Check if bytes at i+2..i+4 are 10 80
        // AND bytes at i-4..i are FF 55 00 00
        if i >= 4
            && i + 4 <= data.len()
            && data[i + 2] == 0x10
            && data[i + 3] == 0x80
            && data[i - 4] == 0xFF
            && data[i - 3] == 0x55
            && data[i - 2] == 0x00
            && data[i - 1] == 0x00
        {
            let ptr_val = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            if ptr_val > 0 && ptr_val < data.len() {
                // Read string until FF 01 terminator
                let mut raw = Vec::new();
                let mut j = ptr_val;
                while j < data.len() - 1 {
                    if data[j] == 0xFF && data[j + 1] == 0x01 {
                        raw.push(0xFF);
                        raw.push(0x01);
                        break;
                    }
                    raw.push(data[j]);
                    j += 1;
                }
                // Only accept if we found the terminator
                if raw.ends_with(&[0xFF, 0x01]) {
                    strings.push(ScriptString {
                        ptr_table_offset: i,
                        text_offset: ptr_val,
                        raw,
                    });
                }
            }
            i += 4;
        } else {
            i += 2;
        }
    }

    debug!("parse_script: found {} strings", strings.len());

    Some(EventScript {
        header_size,
        header,
        ett,
        table_ptr,
        strings,
        data,
    })
}

/// Rebuild a script sub-file with new string data
///
/// Takes the original script and a list of new raw string bytes (one per original string).
/// Returns the rebuilt sub-file bytes.
pub fn rebuild_script(script: &EventScript, new_strings: &[Vec<u8>]) -> Option<Vec<u8>> {
    if new_strings.len() != script.strings.len() {
        warn!("rebuild_script: string count mismatch: got {} expected {}", new_strings.len(), script.strings.len());
        return None;
    }

    debug!("rebuild_script: rebuilding with {} strings, ett=0x{:04x}", new_strings.len(), script.ett);

    let mut data = script.data.clone();

    // Build new text blob starting at ett
    let mut blob = Vec::new();
    let mut new_offsets: Vec<usize> = Vec::with_capacity(new_strings.len());

    for raw in new_strings {
        let offset = script.ett + blob.len();
        new_offsets.push(offset);
        let mut s = raw.clone();
        if !s.ends_with(&[0xFF, 0x01]) {
            s.push(0xFF);
            s.push(0x01);
        }
        blob.extend_from_slice(&s);
    }

    // Update string pointer values in the data section
    for (i, string_info) in script.strings.iter().enumerate() {
        let ptr_off = string_info.ptr_table_offset;
        let new_val = new_offsets[i] as u16;
        if ptr_off + 2 <= data.len() {
            data[ptr_off] = new_val as u8;
            data[ptr_off + 1] = (new_val >> 8) as u8;
        }
    }

    // Replace text area: truncate at ett, append new blob, then append trailing data
    // "Trailing data" = everything after the last string in the original
    let original_text_end = if let Some(last) = script.strings.last() {
        last.text_offset + last.raw.len()
    } else {
        script.ett
    };
    let trailing = if original_text_end < script.data.len() {
        script.data[original_text_end..].to_vec()
    } else {
        Vec::new()
    };

    data.truncate(script.ett);
    data.extend_from_slice(&blob);
    data.extend_from_slice(&trailing);

    // Rebuild full sub-file: header + data
    let mut result = script.header.clone();
    result.extend_from_slice(&data);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        assert!(parse_script(&[]).is_none());
        assert!(parse_script(&[0; 50]).is_none());
    }
}
