//! Disk-based persistent ring buffer implementation
//! 
//! This module provides structures and functions for a persistent, disk-based ring buffer
//! using memory-mapped I/O. It consists of several components:
//! 
//! - Control block for tracking buffer state with integrity verification
//! - Record headers with data integrity checks
//! - Persistent buffer implementation for writing/reading records
//! - Cursor-based reading interface for flexible consumption
//! 
//! The disk module handles all aspects of persistent storage, including:
//! - File creation and management
//! - Memory mapping for efficient I/O
//! - Integrity verification using CRC32 checksums
//! - Handling buffer wrap-arounds
//! - Thread-safe access to the persistent data

pub mod persistent;
pub mod cursor;

use std::path::Path;
use std::fs::{File, OpenOptions};
use memmap2::{MmapMut, MmapOptions};
use std::io::{self, Write, Seek, SeekFrom};
use std::sync::atomic::{AtomicUsize, Ordering};
use crc32fast::Hasher;

/// Buffer control block for persistent storage
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ControlBlock {
    /// Magic number to identify the buffer format (0x5348524C = "SHRL")
    pub magic: u32,
    /// Version of the buffer format
    pub version: u32,
    /// Total size of the buffer in bytes
    pub buffer_size: u64,
    /// Current write position
    pub write_pos: u64,
    /// Current read position
    pub read_pos: u64,
    /// Number of wraps around the buffer
    pub wrap_count: u64,
    /// CRC32 of the control block (excluding this field)
    pub crc32: u32,
}

impl ControlBlock {
    /// Size of the control block in bytes
    pub const SIZE: usize = std::mem::size_of::<Self>();
    
    /// Magic number for buffer format identification ("SHRL")
    pub const MAGIC: u32 = 0x5348524C;
    
    /// Current buffer format version
    pub const VERSION: u32 = 1;
    
    /// Create a new control block for a new buffer
    pub fn new(buffer_size: u64) -> Self {
        let mut block = Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            buffer_size,
            write_pos: 0,
            read_pos: 0,
            wrap_count: 0,
            crc32: 0,
        };
        
        block.update_crc();
        block
    }
    
    /// Calculate CRC32 for the control block and update the field
    pub fn update_crc(&mut self) {
        let mut hasher = Hasher::new();
        
        // Hash all fields except crc32
        let bytes = unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                Self::SIZE - 4, // Exclude crc32 field
            )
        };
        
        hasher.update(bytes);
        self.crc32 = hasher.finalize();
    }
    
    /// Verify the CRC32 of the control block
    pub fn verify_crc(&self) -> bool {
        let mut hasher = Hasher::new();
        
        // Hash all fields except crc32
        let bytes = unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                Self::SIZE - 4, // Exclude crc32 field
            )
        };
        
        hasher.update(bytes);
        self.crc32 == hasher.finalize()
    }
}

/// Record header for disk storage
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Header {
    /// Size of the record data in bytes
    pub size: u32,
    /// CRC32 of the record data
    pub crc32: u32,
    /// Timestamp in microseconds since epoch
    pub timestamp_us: u64,
    /// Record type/tag
    pub tag: u32,
    /// A marker indicating a valid record (0xAA55)
    pub marker: u16,
    /// Reserved for future use
    pub reserved: u16,
}

impl Header {
    /// Size of the header in bytes
    pub const SIZE: usize = std::mem::size_of::<Self>();
    
    /// Marker value for valid records
    pub const MARKER: u16 = 0xAA55;
    
    /// Create a new record header
    pub fn new(size: u32, timestamp_us: u64, tag: u32) -> Self {
        Self {
            size,
            crc32: 0, // Will be calculated when data is written
            timestamp_us,
            tag,
            marker: Self::MARKER,
            reserved: 0,
        }
    }
    
    /// Calculate CRC32 for the given data and update the header
    pub fn update_crc(&mut self, data: &[u8]) {
        let mut hasher = Hasher::new();
        hasher.update(data);
        self.crc32 = hasher.finalize();
    }
    
    /// Verify the CRC32 of the given data against the stored CRC
    pub fn verify_crc(&self, data: &[u8]) -> bool {
        let mut hasher = Hasher::new();
        hasher.update(data);
        self.crc32 == hasher.finalize()
    }
    
    /// Check if this header represents a valid record
    pub fn is_valid(&self) -> bool {
        self.marker == Self::MARKER
    }
}