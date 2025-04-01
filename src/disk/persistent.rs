//! Persistent ring buffer implementation for disk storage
//! 
//! This module implements a single-producer single-consumer (SPSC) ring buffer
//! using memory-mapped files for persistent storage. Key features include:
//! 
//! - Memory-mapped file access for efficient I/O
//! - Control block with CRC32 integrity verification
//! - Record headers with markers and checksums
//! - Wrap-around handling for continuous logging
//! - Thread-safe reading and writing with minimal locking
//! 
//! The buffer stores both a header (control block) that tracks buffer state
//! and the actual log records. Each record includes its own header with
//! size, CRC32 checksum, timestamp, and a marker for validity checking.
//! 
//! The persistent buffer is designed to be read using a cursor (from the
//! cursor module) which provides stateful consumption of records.

use crate::disk::{ControlBlock, Header};
use crate::memory::Record;
use memmap2::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use std::io::{self, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use parking_lot::RwLock as PLRwLock;

/// Error types for the persistent ring buffer
#[derive(Debug)]
pub enum PersistentError {
    /// An IO error occurred
    Io(io::Error),
    /// The buffer file is invalid or corrupted
    InvalidBuffer,
    /// A record is invalid or corrupted
    InvalidRecord,
}

impl From<io::Error> for PersistentError {
    fn from(error: io::Error) -> Self {
        PersistentError::Io(error)
    }
}

/// Result type for persistent buffer operations
pub type Result<T> = std::result::Result<T, PersistentError>;

/// Persistent ring buffer for disk storage
pub struct PersistentRingBuffer {
    /// Path to the buffer file
    path: PathBuf,
    /// Memory-mapped file
    mmap: RwLock<MmapMut>,
    /// Control block for buffer management
    control: PLRwLock<ControlBlock>,
    /// Size of the buffer in bytes
    size: usize,
    /// Guard to ensure single-writer access
    write_lock: Mutex<()>,
}

impl PersistentRingBuffer {
    /// Create or open a persistent ring buffer at the given path
    /// 
    /// # Arguments
    /// 
    /// * `path` - Path to the buffer file
    /// * `size` - Size of the buffer in bytes (ignored if the file already exists)
    pub fn new<P: AsRef<Path>>(path: P, size: usize) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        // Open or create the file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        
        // Check if it's a new file
        let file_size = file.metadata()?.len();
        let is_new = file_size == 0;
        
        // Set the file size if it's a new file
        if is_new {
            // Add control block size to total size
            let total_size = size + ControlBlock::SIZE;
            file.set_len(total_size as u64)?;
        }
        
        // Memory map the file
        let mut mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        
        let control_block = if is_new {
            // Initialize a new control block
            let mut control = ControlBlock::new(size as u64);
            
            // Write control block to file
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    &control as *const ControlBlock as *const u8,
                    ControlBlock::SIZE,
                )
            };
            
            mmap[0..ControlBlock::SIZE].copy_from_slice(bytes);
            control
        } else {
            // Read existing control block
            let control_bytes = &mmap[0..ControlBlock::SIZE];
            let control = unsafe {
                std::ptr::read(control_bytes.as_ptr() as *const ControlBlock)
            };
            
            // Verify control block
            if control.magic != ControlBlock::MAGIC || !control.verify_crc() {
                return Err(PersistentError::InvalidBuffer);
            }
            
            control
        };
        
        Ok(Self {
            path,
            mmap: RwLock::new(mmap),
            control: PLRwLock::new(control_block),
            size: if is_new { size } else { control_block.buffer_size as usize },
            write_lock: Mutex::new(()),
        })
    }
    
    /// Get the file path
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// Get the buffer size
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Get a copy of the control block
    pub fn control_block(&self) -> ControlBlock {
        *self.control.read()
    }
    
    /// Write a record to the buffer
    /// 
    /// # Arguments
    /// 
    /// * `record` - The Record to write
    pub fn write(&self, record: &Record) -> Result<()> {
        // Acquire the write lock
        let _lock = self.write_lock.lock().unwrap();
        
        // Get current control block
        let mut control = self.control.write();
        
        // Calculate required space
        let total_size = Header::SIZE + record.data.len();
        
        // Check if the record fits in the buffer
        if total_size > self.size {
            return Err(PersistentError::InvalidRecord);
        }
        
        // Create header
        let mut header = Header::new(
            record.data.len() as u32,
            record.header.timestamp_us,
            record.header.tag,
        );
        header.update_crc(&record.data);
        
        // Calculate the current write position (after control block)
        let data_start = ControlBlock::SIZE + control.write_pos as usize;
        
        // Check if we need to wrap around
        let wrap = data_start + total_size > (ControlBlock::SIZE + self.size);
        
        let mut mmap = self.mmap.write().expect("Failed to acquire write lock");
        
        if wrap {
            // Wrap around to the start of the data section
            control.write_pos = 0;
            control.wrap_count += 1;
            
            // Optionally: write a zero-length record marker to indicate wrap
            // But we'll rely on the control block for simplicity
        }
        
        // Write header
        let header_bytes = unsafe {
            std::slice::from_raw_parts(
                &header as *const Header as *const u8,
                Header::SIZE,
            )
        };
        
        let header_pos = ControlBlock::SIZE + control.write_pos as usize;
        mmap[header_pos..header_pos + Header::SIZE].copy_from_slice(header_bytes);
        
        // Write data
        let data_pos = header_pos + Header::SIZE;
        mmap[data_pos..data_pos + record.data.len()].copy_from_slice(&record.data);
        
        // Update write position
        control.write_pos += total_size as u64;
        
        // Update the control block in memory
        control.update_crc();
        
        // Write updated control block to the file
        let control_bytes = unsafe {
            std::slice::from_raw_parts(
                &*control as *const ControlBlock as *const u8,
                ControlBlock::SIZE,
            )
        };
        
        mmap[0..ControlBlock::SIZE].copy_from_slice(control_bytes);
        
        // Sync the file to disk
        mmap.flush()?;
        
        Ok(())
    }
    
    /// Read a record at the given position
    /// 
    /// # Arguments
    /// 
    /// * `position` - Position in the buffer (after control block)
    /// 
    /// # Returns
    /// 
    /// The record and its size, or None if no valid record was found
    pub fn read_at(&self, position: u64) -> Result<Option<(Record, usize)>> {
        let mmap = self.mmap.read().expect("Failed to acquire read lock");
        
        // Check if position is valid
        if position >= self.size as u64 {
            return Ok(None);
        }
        
        // Calculate absolute position
        let abs_pos = ControlBlock::SIZE + position as usize;
        
        // Check if we have enough space to read the header
        if abs_pos + Header::SIZE > mmap.len() {
            return Ok(None);
        }
        
        // Read header
        let header = unsafe {
            std::ptr::read(mmap[abs_pos..abs_pos + Header::SIZE].as_ptr() as *const Header)
        };
        
        // Check if this is a valid record
        if !header.is_valid() {
            return Ok(None);
        }
        
        // Check if we have enough space to read the data
        let data_size = header.size as usize;
        let data_pos = abs_pos + Header::SIZE;
        
        if data_pos + data_size > mmap.len() {
            return Ok(None);
        }
        
        // Read data
        let mut data = vec![0u8; data_size];
        data.copy_from_slice(&mmap[data_pos..data_pos + data_size]);
        
        // Verify CRC
        if !header.verify_crc(&data) {
            return Err(PersistentError::InvalidRecord);
        }
        
        // Convert to memory Record format
        let mem_header = crate::memory::RecordHeader {
            size: header.size,
            crc32: header.crc32,
            timestamp_us: header.timestamp_us,
            tag: header.tag,
        };
        
        let record = Record {
            header: mem_header,
            data,
        };
        
        Ok(Some((record, Header::SIZE + data_size)))
    }
    
    /// Read all available records from the current read position
    pub fn read_all(&self) -> Result<Vec<Record>> {
        let mut control = self.control.write();
        
        // If read position equals write position, there's nothing to read
        if control.read_pos == control.write_pos {
            return Ok(Vec::new());
        }
        
        let mut records = Vec::new();
        let mut current_pos = control.read_pos;
        
        // Read until we catch up to the write position
        while current_pos < control.write_pos {
            match self.read_at(current_pos)? {
                Some((record, size)) => {
                    records.push(record);
                    current_pos += size as u64;
                }
                None => {
                    // Couldn't read a valid record, try to skip ahead
                    current_pos += Header::SIZE as u64;
                }
            }
        }
        
        // Update read position
        control.read_pos = current_pos;
        control.update_crc();
        
        // Write updated control block
        let control_bytes = unsafe {
            std::slice::from_raw_parts(
                &*control as *const ControlBlock as *const u8,
                ControlBlock::SIZE,
            )
        };
        
        let mut mmap = self.mmap.write().expect("Failed to acquire write lock");
        mmap[0..ControlBlock::SIZE].copy_from_slice(control_bytes);
        mmap.flush()?;
        
        Ok(records)
    }
    
    /// Reset the read position to skip old records
    pub fn reset_read_position(&self) -> Result<()> {
        let mut control = self.control.write();
        control.read_pos = control.write_pos;
        control.update_crc();
        
        // Write updated control block
        let control_bytes = unsafe {
            std::slice::from_raw_parts(
                &*control as *const ControlBlock as *const u8,
                ControlBlock::SIZE,
            )
        };
        
        let mut mmap = self.mmap.write().expect("Failed to acquire write lock");
        mmap[0..ControlBlock::SIZE].copy_from_slice(control_bytes);
        mmap.flush()?;
        
        Ok(())
    }
    
    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<()> {
        self.mmap.write().expect("Failed to acquire write lock").flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::memory::Record;
    
    #[test]
    fn test_create_new_buffer() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_buffer.dat");
        
        let buffer = PersistentRingBuffer::new(&path, 1024).unwrap();
        assert_eq!(buffer.size(), 1024);
        
        let control = buffer.control_block();
        assert_eq!(control.magic, ControlBlock::MAGIC);
        assert_eq!(control.version, ControlBlock::VERSION);
        assert_eq!(control.buffer_size, 1024);
        assert_eq!(control.write_pos, 0);
        assert_eq!(control.read_pos, 0);
        assert_eq!(control.wrap_count, 0);
        assert!(control.verify_crc());
    }
    
    #[test]
    fn test_write_read_records() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_buffer.dat");
        
        let buffer = PersistentRingBuffer::new(&path, 1024).unwrap();
        
        // Write some records
        for i in 0..5 {
            let data = format!("test message {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(&record).unwrap();
        }
        
        // Read back the records
        let records = buffer.read_all().unwrap();
        assert_eq!(records.len(), 5);
        
        for (i, record) in records.iter().enumerate() {
            let expected = format!("test message {}", i);
            assert_eq!(record.data, expected.as_bytes());
            assert_eq!(record.header.tag, i as u32);
        }
    }
    
    #[test]
    fn test_buffer_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_buffer.dat");
        
        // Create and write to buffer
        {
            let buffer = PersistentRingBuffer::new(&path, 1024).unwrap();
            
            let data = b"persistence test".to_vec();
            let record = Record::new(data, 42);
            buffer.write(&record).unwrap();
            buffer.flush().unwrap();
        }
        
        // Reopen and read
        {
            let buffer = PersistentRingBuffer::new(&path, 0).unwrap(); // Size ignored for existing files
            assert_eq!(buffer.size(), 1024);
            
            let records = buffer.read_all().unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].data, b"persistence test");
            assert_eq!(records[0].header.tag, 42);
        }
    }
    
    #[test]
    fn test_buffer_wrapping() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_buffer.dat");
        
        // Create a small buffer to force wrapping
        let buffer = PersistentRingBuffer::new(&path, 100).unwrap();
        
        // Write enough data to trigger a wrap
        for i in 0..20 {
            let data = format!("msg{}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(&record).unwrap();
        }
        
        // Verify wrap count increased
        let control = buffer.control_block();
        assert!(control.wrap_count > 0);
    }
}