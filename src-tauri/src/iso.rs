//! ISO 9660 MODE2/2352 raw CD image — sector-based I/O
//!
//! PSX discs use raw sectors of 2352 bytes each.
//! User data is at offset 24, size 2048 per sector.
//! This module reads/writes sectors on demand via file seek, not loading the entire image.

use log::{info, warn};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const SECTOR_SIZE: usize = 2352;
pub const USER_OFFSET: usize = 24;
pub const USER_SIZE: usize = 2048;

/// Game file table LBA (FSECT = sector start for each game file)
pub const FSECT_LBA: u32 = 634;

/// Known file indices in the game's internal file table
pub const FILE_IDX_E0: usize = 273;
pub const FILE_IDX_E1: usize = 274;
pub const FILE_IDX_E2: usize = 275;
pub const FILE_IDX_E3: usize = 276;
pub const FILE_IDX_FONT: usize = 5;

/// Handle to an opened ISO image (sector-based I/O)
pub struct IsoImage {
    pub path: PathBuf,
    pub sector_count: u32,
}

impl IsoImage {
    /// Open an ISO image and validate it
    pub fn open(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let file_size = metadata.len() as usize;

        if file_size % SECTOR_SIZE != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("File size {} is not a multiple of {} (not a raw CD image)", file_size, SECTOR_SIZE),
            ));
        }

        let sector_count = (file_size / SECTOR_SIZE) as u32;
        info!("ISO opened: {} bytes, {} sectors", file_size, sector_count);

        Ok(IsoImage {
            path: path.to_path_buf(),
            sector_count,
        })
    }

    /// Read user data (2048 bytes) from a single sector
    pub fn read_sector(&self, lba: u32) -> io::Result<Vec<u8>> {
        let mut file = File::open(&self.path)?;
        let offset = lba as u64 * SECTOR_SIZE as u64 + USER_OFFSET as u64;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; USER_SIZE];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read multiple consecutive sectors' user data
    pub fn read_sectors(&self, lba: u32, count: u32) -> io::Result<Vec<u8>> {
        let mut file = File::open(&self.path)?;
        let mut data = Vec::with_capacity(count as usize * USER_SIZE);
        for i in 0..count {
            let offset = (lba + i) as u64 * SECTOR_SIZE as u64 + USER_OFFSET as u64;
            file.seek(SeekFrom::Start(offset))?;
            let mut buf = vec![0u8; USER_SIZE];
            file.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }
        Ok(data)
    }

    /// Read a game file (consecutive sectors, trimmed to exact size)
    pub fn read_file(&self, lba: u32, size: u32) -> io::Result<Vec<u8>> {
        let sector_count = (size as usize + USER_SIZE - 1) / USER_SIZE;
        let mut data = self.read_sectors(lba, sector_count as u32)?;
        data.truncate(size as usize);
        Ok(data)
    }

    /// Write user data to consecutive sectors (preserves sector headers)
    pub fn write_file(&self, lba: u32, data: &[u8]) -> io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).open(&self.path)?;
        let sector_count = (data.len() + USER_SIZE - 1) / USER_SIZE;
        for i in 0..sector_count {
            let src_start = i * USER_SIZE;
            let src_end = std::cmp::min(src_start + USER_SIZE, data.len());
            let chunk = &data[src_start..src_end];

            let offset = (lba as usize + i) * SECTOR_SIZE + USER_OFFSET;
            file.seek(SeekFrom::Start(offset as u64))?;
            file.write_all(chunk)?;

            // Zero-pad remainder if chunk is short
            if chunk.len() < USER_SIZE {
                let padding = vec![0u8; USER_SIZE - chunk.len()];
                file.write_all(&padding)?;
            }
        }
        Ok(())
    }

    /// Append data at end of image (extends the file). Returns new LBA.
    pub fn append_file(&self, data: &[u8]) -> io::Result<u32> {
        let mut file = std::fs::OpenOptions::new().write(true).read(true).open(&self.path)?;
        let file_size = file.seek(SeekFrom::End(0))?;
        let current_sectors = file_size as usize / SECTOR_SIZE;
        let new_lba = current_sectors as u32;

        let sector_count = (data.len() + USER_SIZE - 1) / USER_SIZE;

        // Write new sectors (full 2352-byte sectors with minimal headers)
        for i in 0..sector_count {
            let src_start = i * USER_SIZE;
            let src_end = std::cmp::min(src_start + USER_SIZE, data.len());
            let chunk = &data[src_start..src_end];

            // Write a blank sector first
            let mut sector = vec![0u8; SECTOR_SIZE];
            // Copy user data at offset 24
            sector[USER_OFFSET..USER_OFFSET + chunk.len()].copy_from_slice(chunk);
            file.write_all(&sector)?;
        }

        info!("Appended {} sectors at LBA {}", sector_count, new_lba);
        Ok(new_lba)
    }

    /// Read uint32_le from FSECT table at given index
    pub fn read_fsect_entry(&self, index: usize) -> io::Result<u32> {
        let sector_in_table = (index * 4) / USER_SIZE;
        let offset_in_sector = (index * 4) % USER_SIZE;
        let sector_data = self.read_sector(FSECT_LBA + sector_in_table as u32)?;
        Ok(u32::from_le_bytes(
            sector_data[offset_in_sector..offset_in_sector + 4].try_into().unwrap(),
        ))
    }

    /// Write uint32_le to FSECT table at given index
    pub fn write_fsect_entry(&self, index: usize, value: u32) -> io::Result<()> {
        let sector_in_table = (index * 4) / USER_SIZE;
        let offset_in_sector = (index * 4) % USER_SIZE;
        let mut sector_data = self.read_sector(FSECT_LBA + sector_in_table as u32)?;
        sector_data[offset_in_sector..offset_in_sector + 4].copy_from_slice(&value.to_le_bytes());
        self.write_sector_data(FSECT_LBA + sector_in_table as u32, &sector_data)
    }

    /// Write 2048 bytes of user data to a specific sector
    fn write_sector_data(&self, lba: u32, data: &[u8]) -> io::Result<()> {
        assert!(data.len() == USER_SIZE);
        let mut file = std::fs::OpenOptions::new().write(true).open(&self.path)?;
        let offset = lba as u64 * SECTOR_SIZE as u64 + USER_OFFSET as u64;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        Ok(())
    }

    /// Determine container size by reading its pointer table from first sector
    pub fn get_container_size(&self, lba: u32) -> io::Result<u32> {
        let first_sector = self.read_sector(lba)?;
        let mut last_ptr: u16 = 0;
        let mut p = 0;
        while p + 2 <= first_sector.len() {
            let v = u16::from_le_bytes([first_sector[p], first_sector[p + 1]]);
            if v == 0 {
                break;
            }
            if last_ptr > 0 && v <= last_ptr {
                break;
            }
            last_ptr = v;
            p += 2;
        }
        Ok((last_ptr as u32) * (crate::e0::ALIGNMENT as u32))
    }

    /// Get info about all text container files
    pub fn get_text_files(&self) -> io::Result<Vec<GameFile>> {
        let files = [
            (FILE_IDX_E0, "E0.BIN"),
            (FILE_IDX_E1, "E1.BIN"),
            (FILE_IDX_E2, "E2.BIN"),
            (FILE_IDX_E3, "E3.BIN"),
        ];

        let mut result = Vec::new();
        for &(idx, name) in &files {
            let lba = self.read_fsect_entry(idx)?;
            let size = self.get_container_size(lba)?;
            result.push(GameFile { index: idx, name, lba, size });
        }
        Ok(result)
    }
}

/// Information about a game file
#[derive(Debug, Clone)]
pub struct GameFile {
    pub index: usize,
    pub name: &'static str,
    pub lba: u32,
    pub size: u32,
}
