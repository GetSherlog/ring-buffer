//! Sherlog Ring Buffer - A multi-producer single-consumer ring buffer
//! for logging on Android systems with disk persistence.
//! 
//! # Overview
//! 
//! Sherlog provides a high-performance logging system with two main components:
//! 
//! 1. A lock-free, MPSC (Multi-Producer, Single-Consumer) volatile ring buffer in memory
//! 2. A persistent, SPSC (Single-Producer, Single-Consumer) ring buffer mapped to disk
//! 
//! A background flush daemon moves data from the volatile to persistent buffer
//! automatically, based on configurable thresholds and intervals.
//! 
//! # Key Features
//! 
//! - Lock-free concurrent writes from multiple threads
//! - Memory-mapped I/O for efficient disk persistence
//! - CRC32 checksums for data integrity
//! - Reserve-Write-Commit pattern for efficient buffer usage
//! - Cursor-based reading for flexible consumption patterns
//! - Android integration via JNI
//! 
//! # Usage
//! 
//! The library is typically used by:
//! 1. Initializing both buffers
//! 2. Starting the flush daemon
//! 3. Writing logs from application code
//! 4. Reading logs using cursors
//! 
//! See the `examples` directory for detailed usage examples.

#![deny(missing_docs)]

mod memory;
mod disk;
#[cfg(feature = "android")]
mod jni;
#[cfg(feature = "ios")]
mod ios;

pub use memory::volatile::VolatileRingBuffer;
pub use disk::persistent::PersistentRingBuffer;
pub use disk::cursor::Cursor;

use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::time::Duration;
use std::thread;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global instance of the volatile memory buffer for multi-thread access
static VOLATILE_BUFFER: OnceCell<Arc<VolatileRingBuffer>> = OnceCell::new();

/// Global instance of the persistent disk buffer
static PERSISTENT_BUFFER: OnceCell<Arc<PersistentRingBuffer>> = OnceCell::new();

/// Flush daemon control flag
static FLUSH_DAEMON_RUNNING: AtomicBool = AtomicBool::new(false);

/// Initialize the global MPSC volatile ring buffer with custom capacity
/// 
/// # Arguments
///
/// * `capacity` - Number of slots in the buffer (rounded up to power of 2)
pub fn init_memory_buffer(capacity: usize) -> Arc<VolatileRingBuffer> {
    let buffer = Arc::new(VolatileRingBuffer::new(capacity));
    
    // We cannot replace the OnceCell directly, so we'll use this buffer for all
    // subsequent calls to get_memory_buffer
    VOLATILE_BUFFER.get_or_init(|| buffer.clone());
    
    buffer
}

/// Initialize the persistent disk buffer with a specified path and size
///
/// # Arguments
///
/// * `path` - Path to the persistent buffer file
/// * `size` - Size of the buffer in bytes
pub fn init_persistent_buffer<P: AsRef<Path>>(path: P, size: usize) -> Arc<PersistentRingBuffer> {
    let result = PersistentRingBuffer::new(path, size);
    
    match result {
        Ok(buffer) => {
            let buffer_arc = Arc::new(buffer);
            
            // We cannot replace the OnceCell directly, so we'll use this buffer for all
            // subsequent calls to get_persistent_buffer
            PERSISTENT_BUFFER.get_or_init(|| buffer_arc.clone());
            
            buffer_arc
        }
        Err(e) => {
            panic!("Failed to create persistent buffer: {:?}", e);
        }
    }
}

/// Get a reference to the global volatile ring buffer
pub fn get_memory_buffer() -> Arc<VolatileRingBuffer> {
    VOLATILE_BUFFER.get_or_init(|| {
        // Default 4096 slots (with 256 bytes each = ~1MB)
        Arc::new(VolatileRingBuffer::new(4096))
    }).clone()
}

/// Get a reference to the global persistent disk buffer
pub fn get_persistent_buffer() -> Arc<PersistentRingBuffer> {
    PERSISTENT_BUFFER.get_or_init(|| {
        // Default: use a temporary file, this will be replaced in init_persistent_buffer
        let path = std::env::temp_dir().join("sherlog_temp.dat");
        Arc::new(PersistentRingBuffer::new(path, 10 * 1024 * 1024)
            .expect("Failed to create temporary buffer"))
    }).clone()
}

/// Configurations for the flush daemon
pub struct FlushDaemonConfig {
    /// Interval between flush operations in milliseconds
    pub interval_ms: u64,
    /// High watermark percentage to trigger immediate flush
    pub high_watermark_percent: f32,
    /// Maximum number of records to flush in one operation
    pub max_records_per_flush: usize,
}

impl Default for FlushDaemonConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1000, // 1 second
            high_watermark_percent: 75.0, // 75%
            max_records_per_flush: 100,
        }
    }
}

/// Start the flush daemon to move data from volatile to persistent storage
///
/// # Arguments
///
/// * `config` - Configuration for the flush daemon
///
/// # Returns
///
/// A join handle for the flush daemon thread
pub fn start_flush_daemon(config: FlushDaemonConfig) -> thread::JoinHandle<()> {
    // Mark the daemon as running
    FLUSH_DAEMON_RUNNING.store(true, Ordering::SeqCst);
    
    // Get buffer references
    let memory_buffer = get_memory_buffer();
    let disk_buffer = get_persistent_buffer();
    
    // Spawn the daemon thread
    thread::Builder::new()
        .name("sherlog-flush-daemon".to_string())
        .spawn(move || {
            let mut batch_size = config.max_records_per_flush;
            
            while FLUSH_DAEMON_RUNNING.load(Ordering::SeqCst) {
                // Check if buffer usage is above high watermark
                let usage = memory_buffer.usage_percent();
                
                // Adjust batch size based on buffer usage for adaptive flushing
                if usage > config.high_watermark_percent {
                    // If usage is high, increase batch size to flush more aggressively
                    batch_size = (config.max_records_per_flush as f32 * 1.5) as usize;
                } else if usage < config.high_watermark_percent / 2.0 {
                    // If usage is low, decrease batch size to reduce I/O overhead
                    batch_size = config.max_records_per_flush / 2;
                } else {
                    // Otherwise use the configured batch size
                    batch_size = config.max_records_per_flush;
                }
                
                // Ensure batch size is at least 1
                batch_size = batch_size.max(1);
                
                // Determine if we should flush
                let should_flush = usage >= config.high_watermark_percent || 
                                   memory_buffer.wait_for_data(Some(config.interval_ms));
                
                if should_flush {
                    // Read from volatile buffer
                    let records = memory_buffer.read();
                    
                    if !records.is_empty() {
                        // Track successful writes
                        let mut success_count = 0;
                        
                        // Write to persistent storage
                        for record in records.iter().take(batch_size) {
                            match disk_buffer.write(record) {
                                Ok(_) => {
                                    success_count += 1;
                                },
                                Err(e) => {
                                    // In production, we might want to handle this error differently
                                    eprintln!("Error writing to persistent buffer: {:?}", e);
                                    // Break on first error to prevent further damage
                                    break;
                                }
                            }
                        }
                        
                        // Only flush if we had successful writes
                        if success_count > 0 {
                            if let Err(e) = disk_buffer.flush() {
                                eprintln!("Error flushing persistent buffer: {:?}", e);
                            }
                        }
                    }
                } else {
                    // No data to flush, sleep for a bit to avoid busy waiting
                    thread::sleep(Duration::from_millis(10));
                }
            }
        })
        .expect("Failed to spawn flush daemon thread")
}

/// Stop the flush daemon
pub fn stop_flush_daemon() {
    FLUSH_DAEMON_RUNNING.store(false, Ordering::SeqCst);
}

/// Write a record to the global volatile buffer
///
/// # Arguments
///
/// * `data` - Data to write
/// * `tag` - Tag for filtering or identifying record type
///
/// # Returns
///
/// `true` if written directly to buffer, `false` if added to overflow queue
pub fn write(data: Vec<u8>, tag: u32) -> bool {
    let record = memory::Record::new(data, tag);
    get_memory_buffer().write(record)
}

/// Reserve space in the global volatile buffer for writing
///
/// # Arguments
///
/// * `size` - Size of the data to be written
/// * `block` - Whether to block if the buffer is full
///
/// # Returns
///
/// Some(Reservation) if space was successfully reserved, None if the buffer is full
/// and `block` is false
pub fn reserve(size: usize, block: bool) -> Option<memory::volatile::Reservation> {
    // Verify size does not exceed slot capacity
    let buffer = get_memory_buffer();
    assert!(size <= buffer.slot_size() - memory::RecordHeader::SIZE, 
        "Data size exceeds maximum slot capacity");
    
    buffer.reserve(size, block)
}

/// Re-exported data types used in the API
pub mod types {
    pub use crate::memory::Record;
    pub use crate::disk::Header;
}
