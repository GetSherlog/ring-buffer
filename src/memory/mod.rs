//! Memory-based volatile ring buffer implementation
//! 
//! This module provides core data structures and implementations for the in-memory
//! volatile ring buffer. Key components include:
//! 
//! - Record structure that combines header metadata with data payload
//! - RecordHeader with timing, size, and integrity information
//! - AtomicIndex for lock-free concurrent access
//! - Cache-line padding to prevent false sharing
//! 
//! The memory module implements data structures needed for both:
//! - The volatile in-memory buffer (faster, but non-persistent)
//! - Records that are transferred to the persistent storage
//! 
//! The design focuses on performance, with carefully implemented atomic operations
//! and memory layout optimizations for concurrent access patterns.

pub mod volatile;

use std::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_utils::CachePadded;
use crc32fast::Hasher;

/// Record header information
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct RecordHeader {
    /// Size of the record in bytes (not including header)
    pub size: u32,
    /// CRC32 checksum of the record data
    pub crc32: u32,
    /// Timestamp of the record (microseconds since epoch)
    pub timestamp_us: u64,
    /// Record type/tag for filtering
    pub tag: u32,
}

impl RecordHeader {
    /// Size of the record header in bytes
    pub const SIZE: usize = std::mem::size_of::<Self>();
    
    /// Create a new record header
    pub fn new(size: u32, tag: u32) -> Self {
        Self {
            size,
            crc32: 0, // Will be calculated when the record is written
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
            tag,
        }
    }
    
    /// Calculate CRC32 for the given data and update the header
    pub fn update_crc(&mut self, data: &[u8]) {
        let mut hasher = Hasher::new();
        hasher.update(data);
        self.crc32 = hasher.finalize();
    }
    
    /// Verify the CRC of the given data against the stored CRC
    pub fn verify_crc(&self, data: &[u8]) -> bool {
        let mut hasher = Hasher::new();
        hasher.update(data);
        self.crc32 == hasher.finalize()
    }
}

/// A record in the ring buffer, containing header and data
#[derive(Debug)]
pub struct Record {
    /// Record metadata
    pub header: RecordHeader,
    /// Actual record data
    pub data: Vec<u8>,
}

impl Record {
    /// Create a new record with the given data and tag
    pub fn new(data: Vec<u8>, tag: u32) -> Self {
        let mut header = RecordHeader::new(data.len() as u32, tag);
        header.update_crc(&data);
        Self { header, data }
    }
    
    /// Total size of the record including header
    pub fn total_size(&self) -> usize {
        RecordHeader::SIZE + self.data.len()
    }
}

/// A multi-producer, single-consumer atomic index
pub(crate) struct AtomicIndex {
    /// The current index value, cache-line padded to avoid false sharing
    value: CachePadded<AtomicUsize>,
    /// Mask for quickly wrapping indices to capacity
    mask: usize,
}

impl AtomicIndex {
    /// Create a new atomic index with the given capacity
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be a power of two");
        Self {
            value: CachePadded::new(AtomicUsize::new(0)),
            mask: capacity - 1,
        }
    }
    
    /// Load the current index value with the specified ordering
    pub fn load(&self, ordering: Ordering) -> usize {
        self.value.load(ordering)
    }
    
    /// Store a new index value with the specified ordering
    pub fn store(&self, val: usize, ordering: Ordering) {
        self.value.store(val, ordering);
    }
    
    /// Compare and swap the index value
    pub fn compare_exchange(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.value.compare_exchange(current, new, success, failure)
    }
    
    /// Fetch and add to the index value, wrapping around capacity
    pub fn fetch_add(&self, val: usize, ordering: Ordering) -> usize {
        // Use the mask to wrap around capacity
        let result = self.value.fetch_add(val, ordering);
        result & self.mask
    }
    
    /// Wrap a value to the capacity using the mask
    pub fn wrap(&self, val: usize) -> usize {
        val & self.mask
    }
}