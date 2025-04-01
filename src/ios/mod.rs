//! iOS bindings for Swift/Objective-C integration
//! 
//! This module provides Swift-compatible bindings for using the Sherlog Ring Buffer
//! on iOS platforms. It includes:
//! 
//! - Initialization and shutdown functions
//! - Writing log records from Swift
//! - Cursor management for reading logs
//! - Conversion between Rust and Swift data structures
//! 
//! The bindings are generated using the swift-bridge crate to ensure
//! proper memory management and type safety between Rust and Swift.

use crate::{
    get_persistent_buffer,
    init_memory_buffer,
    init_persistent_buffer,
    FlushDaemonConfig,
    start_flush_daemon,
    stop_flush_daemon,
    write,
    Cursor,
};

use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

// Thread handle for the flush daemon
static FLUSH_DAEMON_HANDLE: Lazy<Mutex<Option<JoinHandle<()>>>> = Lazy::new(|| Mutex::new(None));

// Swift bridge definitions
#[cfg(feature = "ios")]
#[swift_bridge::bridge]
mod ffi {
    use crate::memory::Record;
    
    // Opaque Rust type for the Cursor
    #[swift_bridge(swift_name = "SherlongCursor")]
    pub struct RustCursor(pub crate::Cursor);
    
    // Swift-representable log record
    #[swift_bridge(swift_name = "LogRecord")]
    pub struct SwiftLogRecord {
        pub data: Vec<u8>,
        pub crc32: u32,
        pub timestamp_us: u64,
        pub tag: u32,
    }
    
    // Exposed Rust functions
    extern "Rust" {
        #[swift_bridge(swift_name = "initialize")]
        pub fn swift_initialize(
            disk_path: &str, 
            volatile_buffer_size: u32, 
            persistent_buffer_size: u32,
            flush_interval_ms: u32,
            high_watermark_percent: f32
        ) -> bool;
        
        #[swift_bridge(swift_name = "shutdown")]
        pub fn swift_shutdown();
        
        #[swift_bridge(swift_name = "writeLogData")]
        pub fn swift_write(data: &[u8], tag: u32) -> bool;
        
        #[swift_bridge(swift_name = "createCursor")]
        pub fn swift_create_cursor() -> RustCursor;
        
        #[swift_bridge(swift_name = "readNextRecord")]
        pub fn swift_read_next(cursor: &mut RustCursor) -> Option<SwiftLogRecord>;
        
        #[swift_bridge(swift_name = "readBatch")]
        pub fn swift_read_batch(cursor: &mut RustCursor, max_count: u32) -> Vec<SwiftLogRecord>;
        
        #[swift_bridge(swift_name = "resetCursor")]
        pub fn swift_reset_cursor(cursor: &mut RustCursor);
        
        #[swift_bridge(swift_name = "commitCursor")]
        pub fn swift_commit_cursor(cursor: &RustCursor) -> bool;
    }
}

/// Initialize the sherlog library for iOS
pub fn swift_initialize(
    disk_path: &str,
    volatile_buffer_size: u32,
    persistent_buffer_size: u32,
    flush_interval_ms: u32,
    high_watermark_percent: f32,
) -> bool {
    // Convert the path string
    let path = PathBuf::from(disk_path);
    
    // Initialize the buffers
    let _ = init_memory_buffer(volatile_buffer_size as usize);
    let _ = init_persistent_buffer(path, persistent_buffer_size as usize);
    
    // Configure the flush daemon
    let config = FlushDaemonConfig {
        interval_ms: flush_interval_ms as u64,
        high_watermark_percent: high_watermark_percent as f32,
        max_records_per_flush: 100,
    };
    
    // Start the flush daemon
    let handle = start_flush_daemon(config);
    
    // Store the handle in the global mutex
    let mut daemon_handle = FLUSH_DAEMON_HANDLE.lock().unwrap();
    *daemon_handle = Some(handle);
    
    true
}

/// Shutdown the sherlog library
pub fn swift_shutdown() {
    // Stop the flush daemon
    stop_flush_daemon();
    
    // Wait for the daemon to terminate
    let mut daemon_handle = FLUSH_DAEMON_HANDLE.lock().unwrap();
    if let Some(handle) = daemon_handle.take() {
        let _ = handle.join();
    }
}

/// Write data to the log buffer
pub fn swift_write(data: &[u8], tag: u32) -> bool {
    let data_vec = data.to_vec();
    write(data_vec, tag)
}

/// Create a new cursor for reading from the persistent buffer
pub fn swift_create_cursor() -> ffi::RustCursor {
    let buffer = get_persistent_buffer();
    let cursor = Cursor::new(buffer, None);
    ffi::RustCursor(cursor)
}

/// Read the next record from the cursor
pub fn swift_read_next(cursor: &mut ffi::RustCursor) -> Option<ffi::SwiftLogRecord> {
    match cursor.0.next() {
        Ok(Some(record)) => {
            Some(ffi::SwiftLogRecord {
                data: record.data,
                crc32: record.header.crc32,
                timestamp_us: record.header.timestamp_us,
                tag: record.header.tag,
            })
        },
        _ => None,
    }
}

/// Read a batch of records from the cursor
pub fn swift_read_batch(cursor: &mut ffi::RustCursor, max_count: u32) -> Vec<ffi::SwiftLogRecord> {
    match cursor.0.read_batch(max_count as usize) {
        Ok(records) => {
            records.into_iter().map(|record| {
                ffi::SwiftLogRecord {
                    data: record.data,
                    crc32: record.header.crc32,
                    timestamp_us: record.header.timestamp_us,
                    tag: record.header.tag,
                }
            }).collect()
        },
        Err(_) => Vec::new(),
    }
}

/// Reset the cursor to the end of the buffer
pub fn swift_reset_cursor(cursor: &mut ffi::RustCursor) {
    let _ = cursor.0.seek_to_end();
}

/// Commit the cursor position to the buffer
pub fn swift_commit_cursor(cursor: &ffi::RustCursor) -> bool {
    match cursor.0.commit() {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Example Swift interface
pub fn generate_swift_interface() -> String {
    r#"import Foundation

/**
 * LogRecord represents a single entry in the log buffer
 */
public struct LogRecord {
    /// Raw binary data for the log record
    public let data: Data
    
    /// CRC32 checksum of the data
    public let crc32: UInt32
    
    /// Timestamp in microseconds since epoch
    public let timestampUs: UInt64
    
    /// Tag value for categorizing records
    public let tag: UInt32
    
    /// Convenience property to access the data as a UTF8 string
    public var message: String? {
        return String(data: data, encoding: .utf8)
    }
}

/**
 * SherlongLogger provides access to the Rust-based ring buffer logging system
 */
public class SherlongLogger {
    
    /// Singleton instance
    public static let shared = SherlongLogger()
    
    private init() {}
    
    /**
     * Initialize the logging system
     *
     * - Parameters:
     *   - diskPath: Path to store the persistent log file
     *   - volatileBufferSize: Size of the in-memory buffer in bytes
     *   - persistentBufferSize: Size of the persistent buffer in bytes
     *   - flushIntervalMs: How often to flush from memory to disk (milliseconds)
     *   - highWatermarkPercent: Buffer usage percentage to trigger immediate flush
     * - Returns: True if initialization was successful
     */
    public func initialize(
        diskPath: String,
        volatileBufferSize: UInt32 = 1_048_576, // 1MB
        persistentBufferSize: UInt32 = 10_485_760, // 10MB
        flushIntervalMs: UInt32 = 1000,
        highWatermarkPercent: Float = 75.0
    ) -> Bool {
        return SherlongBindings.initialize(
            diskPath,
            volatileBufferSize: volatileBufferSize,
            persistentBufferSize: persistentBufferSize,
            flushIntervalMs: flushIntervalMs,
            highWatermarkPercent: highWatermarkPercent
        )
    }
    
    /**
     * Shutdown the logging system
     */
    public func shutdown() {
        SherlongBindings.shutdown()
    }
    
    /**
     * Write a string message to the log
     *
     * - Parameters:
     *   - message: The message to log
     *   - tag: A tag value for filtering/identifying log records
     * - Returns: True if the write was successful
     */
    public func write(message: String, tag: UInt32) -> Bool {
        guard let data = message.data(using: .utf8) else {
            return false
        }
        return SherlongBindings.writeLogData(data, tag: tag)
    }
    
    /**
     * Write raw data to the log
     *
     * - Parameters:
     *   - data: The binary data to write
     *   - tag: A tag value for filtering/identifying log records
     * - Returns: True if the write was successful
     */
    public func write(data: Data, tag: UInt32) -> Bool {
        return SherlongBindings.writeLogData(data, tag: tag)
    }
    
    /**
     * Read all available logs
     *
     * - Returns: Array of LogRecord objects
     */
    public func readAllLogs() -> [LogRecord] {
        var cursor = SherlongBindings.createCursor()
        var records = [LogRecord]()
        
        while let record = SherlongBindings.readNextRecord(&cursor) {
            records.append(record)
        }
        
        _ = SherlongBindings.commitCursor(cursor)
        return records
    }
    
    /**
     * Read logs with a cursor, allowing for batched reading
     *
     * - Parameter block: Closure that receives a LogCursor to read with
     */
    public func withCursor(_ block: (LogCursor) -> Void) {
        let rustCursor = SherlongBindings.createCursor()
        let cursor = LogCursor(rustCursor: rustCursor)
        block(cursor)
    }
}

/**
 * LogCursor provides a way to read logs incrementally
 */
public class LogCursor {
    private var rustCursor: SherlongCursor
    
    internal init(rustCursor: SherlongCursor) {
        self.rustCursor = rustCursor
    }
    
    deinit {
        commit()
    }
    
    /**
     * Read the next record from the cursor
     *
     * - Returns: The next LogRecord, or nil if at the end
     */
    public func next() -> LogRecord? {
        return SherlongBindings.readNextRecord(&rustCursor)
    }
    
    /**
     * Read a batch of records from the cursor
     *
     * - Parameter maxCount: Maximum number of records to read
     * - Returns: Array of LogRecord objects
     */
    public func readBatch(maxCount: UInt32) -> [LogRecord] {
        return SherlongBindings.readBatch(&rustCursor, maxCount: maxCount)
    }
    
    /**
     * Reset the cursor to the end of the buffer
     */
    public func reset() {
        SherlongBindings.resetCursor(&rustCursor)
    }
    
    /**
     * Commit the cursor position to the buffer
     *
     * - Returns: True if the commit was successful
     */
    @discardableResult
    public func commit() -> Bool {
        return SherlongBindings.commitCursor(rustCursor)
    }
}
"#.to_string()
}