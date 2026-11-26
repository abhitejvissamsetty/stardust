//! The browser the installer carries. `cargo xtask installer` appends a gzipped tar of
//! `dist/` to the installer program, followed by a trailer saying where it is:
//!
//! ```text
//! [installer program][package][b"STARDUST"][offset: u64 LE][length: u64 LE]
//! ```
//!
//! Programs ignore data after their end, so the file still runs.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

const MAGIC: &[u8; 8] = b"STARDUST";
const TRAILER: u64 = 24;

/// Where the package is: a file, and the bytes within it.
#[derive(Debug, Clone)]
pub struct Payload {
    pub file: PathBuf,
    pub offset: u64,
    pub length: u64,
}

impl Payload {
    /// The package appended to this program, or, for testing, the file named by
    /// `STARDUST_INSTALLER_PAYLOAD`.
    pub fn find() -> io::Result<Self> {
        if let Some(file) = std::env::var_os("STARDUST_INSTALLER_PAYLOAD") {
            let length = std::fs::metadata(&file)?.len();
            return Ok(Self {
                file: file.into(),
                offset: 0,
                length,
            });
        }
        let file = std::env::current_exe()?;
        let mut handle = File::open(&file)?;
        let size = handle.metadata()?.len();
        if size < TRAILER {
            return Err(missing());
        }
        handle.seek(SeekFrom::End(-(TRAILER as i64)))?;
        let mut trailer = [0; TRAILER as usize];
        handle.read_exact(&mut trailer)?;
        let (magic, numbers) = trailer.split_at(8);
        if magic != MAGIC {
            return Err(missing());
        }
        let number = |bytes: &[u8]| u64::from_le_bytes(bytes.try_into().expect("8 bytes"));
        let (offset, length) = (number(&numbers[..8]), number(&numbers[8..]));
        if offset
            .checked_add(length)
            .is_none_or(|end| end > size - TRAILER)
        {
            return Err(missing());
        }
        Ok(Self {
            file,
            offset,
            length,
        })
    }

    /// Reads the package.
    pub fn open(&self) -> io::Result<io::Take<File>> {
        let mut file = File::open(&self.file)?;
        file.seek(SeekFrom::Start(self.offset))?;
        Ok(file.take(self.length))
    }
}

/// The trailer that follows the package, as `cargo xtask installer` writes it.
#[cfg(test)]
pub fn trailer(offset: u64, length: u64) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(offset.to_le_bytes());
    bytes.extend(length.to_le_bytes());
    bytes
}

fn missing() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "This installer is incomplete: it doesn't carry Stardust. Download it again.",
    )
}
