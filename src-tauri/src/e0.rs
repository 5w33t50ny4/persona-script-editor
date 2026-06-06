//! E0/E1/E2/E3 container format parser
//!
//! Each container has:
//! - Pointer table: uint16_le[] values, each = offset in 0x800-byte units
//!   Terminated by 0x0000. Last entry marks end-of-data.
//! - Sub-files: aligned to 0x800 boundaries

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

pub const ALIGNMENT: usize = 0x800;

/// Parsed E0-style container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    /// Raw sub-file data for each slot
    pub sub_files: Vec<Vec<u8>>,
}

/// Parse a container from raw bytes
pub fn parse_container(data: &[u8]) -> Container {
    let ptrs = parse_pointer_table(data);
    let num_sub_files = if ptrs.is_empty() { 0 } else { ptrs.len() - 1 };

    debug!("parse_container: {} bytes, {} pointer entries, {} sub-files", data.len(), ptrs.len(), num_sub_files);

    let mut sub_files = Vec::with_capacity(num_sub_files);
    for i in 0..num_sub_files {
        let start = ptrs[i] as usize * ALIGNMENT;
        let end = ptrs[i + 1] as usize * ALIGNMENT;
        let end = end.min(data.len());
        if start < end && start < data.len() {
            sub_files.push(data[start..end].to_vec());
        } else {
            warn!("parse_container: sub-file {} has invalid range [{:#x}..{:#x}], data.len()={:#x}", i, start, end, data.len());
            sub_files.push(Vec::new());
        }
    }

    Container { sub_files }
}

/// Rebuild container from sub-files back to raw bytes
pub fn build_container(container: &Container) -> Vec<u8> {
    let n = container.sub_files.len();
    info!("build_container: rebuilding {} sub-files", n);
    // Pointer table: n+1 entries (sub-file starts + end marker) + terminator 0x0000
    // Padded to ALIGNMENT
    let _table_size = (n + 2) * 2; // +1 for end ptr, +1 for 0x0000 terminator
    let data_start = ALIGNMENT; // First sub-file always at 0x800

    // Calculate offsets for each sub-file
    let mut offsets: Vec<usize> = Vec::with_capacity(n + 1);
    let mut current = data_start;
    for sf in &container.sub_files {
        offsets.push(current);
        let aligned_size = (sf.len() + ALIGNMENT - 1) & !(ALIGNMENT - 1);
        current += if sf.is_empty() { ALIGNMENT } else { aligned_size };
    }
    offsets.push(current); // end-of-data pointer

    let total_size = current;
    let mut out = vec![0u8; total_size];

    // Write pointer table
    for (i, &off) in offsets.iter().enumerate() {
        let ptr_val = (off / ALIGNMENT) as u16;
        let table_off = i * 2;
        if table_off + 2 <= out.len() {
            out[table_off..table_off + 2].copy_from_slice(&ptr_val.to_le_bytes());
        }
    }
    // Terminator 0x0000 is already there (vec initialized to 0)

    // Write sub-files
    for (i, sf) in container.sub_files.iter().enumerate() {
        if !sf.is_empty() {
            let off = offsets[i];
            out[off..off + sf.len()].copy_from_slice(sf);
        }
    }

    out
}

/// Parse pointer table from container start, returns vec of uint16 values
fn parse_pointer_table(data: &[u8]) -> Vec<u16> {
    let mut ptrs = Vec::new();
    let mut p = 0;
    while p + 2 <= data.len() && p < 4096 {
        let v = u16::from_le_bytes([data[p], data[p + 1]]);
        if v == 0 {
            break;
        }
        if !ptrs.is_empty() && v <= *ptrs.last().unwrap() {
            break;
        }
        ptrs.push(v);
        p += 2;
    }
    ptrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        // Create a simple container with 2 sub-files
        let container = Container {
            sub_files: vec![vec![0x42; 100], vec![0x55; 200]],
        };
        let built = build_container(&container);
        let parsed = parse_container(&built);
        assert_eq!(parsed.sub_files.len(), 2);
        // Sub-files are padded to ALIGNMENT in the container
        assert!(parsed.sub_files[0].starts_with(&[0x42; 100]));
        assert!(parsed.sub_files[1].starts_with(&[0x55; 200]));
    }
}
