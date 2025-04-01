//! JNI bindings for Android integration
//! 
//! This module provides Java Native Interface (JNI) bindings to make the Sherlog Ring Buffer
//! accessible from Android Java/Kotlin code. It includes:
//! 
//! - Initialization and shutdown functions
//! - Writing log records from Java
//! - Cursor management for reading logs
//! - Conversion between Rust and Java data structures
//! 
//! The module also includes a function to generate the corresponding Kotlin interface
//! that should be used on the Android side.

use crate::{
    get_persistent_buffer,
    init_memory_buffer,
    init_persistent_buffer,
    FlushDaemonConfig,
    start_flush_daemon,
    stop_flush_daemon,
    write,
    types::Record,
    Cursor,
};

use jni::JNIEnv;
use jni::objects::{JClass, JString, JByteArray, JObject, JObjectArray, JValue};
use jni::sys::{jint, jlong, jboolean, jobject, jfloat};
use std::thread;
use std::path::PathBuf;

// Global thread handle for the flush daemon
static mut FLUSH_DAEMON_HANDLE: Option<thread::JoinHandle<()>> = None;

/// Initialize the sherlog library, setting up both buffers and starting the flush daemon
///
/// # JNI Signature
/// ```java
/// public static native boolean initialize(String diskPath, int volatileBufferSize,
///                                          int persistentBufferSize, int flushIntervalMs,
///                                          float highWatermarkPercent);
/// ```
#[no_mangle]
pub extern "system" fn Java_com_example_app_Sherlog_initialize(
    mut env: JNIEnv,
    _class: JClass,
    disk_path: JString,
    volatile_buffer_size: jint,
    persistent_buffer_size: jint,
    flush_interval_ms: jint,
    high_watermark_percent: jfloat,
) -> jboolean {
    // Convert the path string
    let path_string: String = env
        .get_string(&disk_path)
        .expect("Invalid path string")
        .into();
    let path = PathBuf::from(path_string);
    
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
    
    // Store the handle
    unsafe {
        FLUSH_DAEMON_HANDLE = Some(handle);
    }
    
    jboolean::from(true)
}

/// Shutdown the sherlog library, stopping the flush daemon
///
/// # JNI Signature
/// ```java
/// public static native void shutdown();
/// ```
#[no_mangle]
pub extern "system" fn Java_com_example_app_Sherlog_shutdown(
    _env: JNIEnv,
    _class: JClass,
) {
    // Stop the flush daemon
    stop_flush_daemon();
    
    // Wait for the daemon to terminate
    unsafe {
        if let Some(handle) = FLUSH_DAEMON_HANDLE.take() {
            let _ = handle.join();
        }
    }
}

/// Write data to the log buffer
///
/// # JNI Signature
/// ```java
/// public static native boolean write(byte[] data, int tag);
/// ```
#[no_mangle]
pub extern "system" fn Java_com_example_app_Sherlog_write(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    tag: jint,
) -> jboolean {
    // Convert the byte array to a Rust vector
    let data_vec = env.convert_byte_array(&data)
        .expect("Failed to convert byte array");
    
    // Write to the buffer
    let success = write(data_vec, tag as u32);
    
    jboolean::from(success)
}

/// Create a new cursor for reading from the persistent buffer
///
/// # JNI Signature
/// ```java
/// public static native long createCursor();
/// ```
#[no_mangle]
pub extern "system" fn Java_com_example_app_Sherlog_createCursor(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    // Get the persistent buffer
    let buffer = get_persistent_buffer();
    
    // Create a new cursor
    let cursor = Box::new(Cursor::new(buffer, None));
    
    // Convert the box to a raw pointer and return it as a jlong
    Box::into_raw(cursor) as jlong
}

/// Read a batch of records from the cursor
///
/// # JNI Signature
/// ```java
/// public static native Object[] readBatch(long cursorPtr, int maxCount);
/// ```
#[no_mangle]
pub extern "system" fn Java_com_example_app_Sherlog_readBatch(
    mut env: JNIEnv,
    _class: JClass,
    cursor_ptr: jlong,
    max_count: jint,
) -> jobject {
    // Convert the jlong back to a Box<Cursor>
    let cursor_ptr = cursor_ptr as *mut Cursor;
    if cursor_ptr.is_null() {
        return std::ptr::null_mut();
    }
    
    let mut cursor = unsafe { Box::from_raw(cursor_ptr) };
    
    // Read a batch of records
    match cursor.read_batch(max_count as usize) {
        Ok(records) => {
            // Don't free the cursor - we're returning it back to Java
            std::mem::forget(cursor);
            
            // Convert the records to a Java array of LogRecord objects
            let record_class = env.find_class("com/example/app/LogRecord")
                .expect("LogRecord class not found");
            
            let records_array = env.new_object_array(
                records.len() as jint, 
                record_class,
                JObject::null(),
            ).expect("Failed to create records array");
            
            for (i, record) in records.iter().enumerate() {
                // Need to handle each record directly here without the helper function
                // since we can't use &mut env and then env in the same scope
                let record_class = env.find_class("com/example/app/LogRecord")
                    .expect("LogRecord class not found");
                
                let data_array = env.byte_array_from_slice(&record.data)
                    .expect("Failed to create byte array");
                
                let record_obj = env.new_object(
                    &record_class,
                    "([BJIJ)V",
                    &[
                        (&data_array).into(),
                        (record.header.crc32 as i64).into(),
                        (record.header.tag as i32).into(),
                        (record.header.timestamp_us as i64).into(),
                    ],
                ).expect("Failed to create LogRecord");
                
                env.set_object_array_element(&records_array, i as jint, record_obj)
                    .expect("Failed to set array element");
            }
            
            // Return the raw jobject
            records_array.into_raw()
        }
        Err(_) => {
            // Don't free the cursor - we're returning it back to Java
            std::mem::forget(cursor);
            std::ptr::null_mut()
        }
    }
}

/// Close a cursor
///
/// # JNI Signature
/// ```java
/// public static native void closeCursor(long cursorPtr);
/// ```
#[no_mangle]
pub extern "system" fn Java_com_example_app_Sherlog_closeCursor(
    _env: JNIEnv,
    _class: JClass,
    cursor_ptr: jlong,
) {
    // Convert the jlong back to a Box<Cursor> and drop it
    let cursor_ptr = cursor_ptr as *mut Cursor;
    if !cursor_ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(cursor_ptr);
        }
    }
}


/// Generate a sample Kotlin interface class for using the JNI functions
pub fn generate_kotlin_interface() -> String {
    r#"
    package com.example.app
    
    /**
     * A record in the log buffer
     */
    data class LogRecord(
        val data: ByteArray,
        val crc32: Long,
        val tag: Int,
        val timestampUs: Long
    ) {
        override fun equals(other: Any?): Boolean {
            if (this === other) return true
            if (javaClass != other?.javaClass) return false
    
            other as LogRecord
    
            if (!data.contentEquals(other.data)) return false
            if (crc32 != other.crc32) return false
            if (tag != other.tag) return false
            if (timestampUs != other.timestampUs) return false
    
            return true
        }
    
        override fun hashCode(): Int {
            var result = data.contentHashCode()
            result = 31 * result + crc32.hashCode()
            result = 31 * result + tag
            result = 31 * result + timestampUs.hashCode()
            return result
        }
    }
    
    /**
     * Interface to the Sherlog native library
     */
    object Sherlog {
        init {
            System.loadLibrary("sherlog_ring_buffer")
        }
    
        /**
         * Initialize the logging system
         * 
         * @param diskPath Path to store the persistent log file
         * @param volatileBufferSize Size of the in-memory buffer in bytes
         * @param persistentBufferSize Size of the persistent buffer in bytes
         * @param flushIntervalMs How often to flush from memory to disk (milliseconds)
         * @param highWatermarkPercent Buffer usage percentage to trigger immediate flush
         * @return true if initialization was successful
         */
        external fun initialize(
            diskPath: String,
            volatileBufferSize: Int,
            persistentBufferSize: Int,
            flushIntervalMs: Int,
            highWatermarkPercent: Float
        ): Boolean
    
        /**
         * Shutdown the logging system
         */
        external fun shutdown()
    
        /**
         * Write data to the log
         * 
         * @param data The bytes to write
         * @param tag A tag value for filtering/identifying log records
         * @return true if the write was successful
         */
        external fun write(data: ByteArray, tag: Int): Boolean
    
        /**
         * Create a cursor for reading from the persistent buffer
         * 
         * @return A pointer to the cursor (handle)
         */
        external fun createCursor(): Long
    
        /**
         * Read a batch of records from the cursor
         * 
         * @param cursorPtr Pointer to the cursor
         * @param maxCount Maximum number of records to read
         * @return Array of LogRecord objects, or null if an error occurred
         */
        external fun readBatch(cursorPtr: Long, maxCount: Int): Array<LogRecord>?
    
        /**
         * Close and free the cursor
         * 
         * @param cursorPtr Pointer to the cursor
         */
        external fun closeCursor(cursorPtr: Long)
        
        /**
         * Helper method to use the cursor with Kotlin's 'use' pattern
         */
        inline fun <T> withCursor(block: (Long) -> T): T {
            val cursor = createCursor()
            try {
                return block(cursor)
            } finally {
                closeCursor(cursor)
            }
        }
        
        /**
         * Example of how to read all logs
         */
        fun readAllLogs(): List<LogRecord> {
            val result = mutableListOf<LogRecord>()
            
            withCursor { cursor ->
                var batch: Array<LogRecord>?
                do {
                    batch = readBatch(cursor, 100)
                    batch?.let { result.addAll(it) }
                } while (batch != null && batch.isNotEmpty())
            }
            
            return result
        }
    }
    "#.to_string()
}