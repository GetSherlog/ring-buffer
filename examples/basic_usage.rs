//! Basic usage example for the sherlog ring buffer
//! 
//! This example demonstrates:
//! 1. Initializing the memory and persistent buffers
//! 2. Configuring and starting the flush daemon
//! 3. Writing logs from multiple threads concurrently
//! 4. Reading logs back using a cursor
//! 
//! The example creates both buffers, writes a large number of records from
//! multiple threads with different tags, and then reads them back to verify
//! everything was written correctly. It uses a temporary file for persistence
//! which is cleaned up at the end.

use sherlog_ring_buffer::{
    init_memory_buffer, 
    init_persistent_buffer,
    get_memory_buffer,
    get_persistent_buffer,
    FlushDaemonConfig,
    start_flush_daemon,
    stop_flush_daemon,
    write,
    Cursor,
};

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::path::PathBuf;

// Define some example tags for log records
const TAG_INFO: u32 = 1;
const TAG_WARNING: u32 = 2;
const TAG_ERROR: u32 = 3;

fn main() {
    // Initialize the buffers
    let mem_buffer = init_memory_buffer(1024 * 1024); // 1MB memory buffer
    
    // Create a temporary file for the persistent buffer
    let disk_path = std::env::temp_dir().join("sherlog_example.dat");
    println!("Using persistent buffer at: {:?}", disk_path);
    
    let disk_buffer = init_persistent_buffer(&disk_path, 10 * 1024 * 1024); // 10MB disk buffer
    
    // Configure and start the flush daemon
    let config = FlushDaemonConfig {
        interval_ms: 500, // Flush every 500ms
        high_watermark_percent: 50.0, // Or when buffer is 50% full
        max_records_per_flush: 100,
    };
    
    let daemon = start_flush_daemon(config);
    
    // Write some logs from multiple threads
    let num_threads = 4;
    let logs_per_thread = 1000;
    
    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            thread::spawn(move || {
                for i in 0..logs_per_thread {
                    let tag = match i % 3 {
                        0 => TAG_INFO,
                        1 => TAG_WARNING,
                        _ => TAG_ERROR,
                    };
                    
                    let message = format!("Thread {} - Log {} - Content: {} log message", 
                                         thread_id, i, 
                                         match tag {
                                             TAG_INFO => "Info",
                                             TAG_WARNING => "Warning",
                                             _ => "Error",
                                         });
                    
                    let success = write(message.into_bytes(), tag);
                    
                    // Sleep a bit to simulate real-world logging intervals
                    if i % 100 == 0 {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            })
        })
        .collect();
    
    // Wait for all logging threads to complete
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Sleep a bit to allow the flush daemon to process
    thread::sleep(Duration::from_millis(1000));
    
    // Stop the flush daemon
    stop_flush_daemon();
    daemon.join().unwrap();
    
    // Now read back the logs using a cursor
    println!("Reading logs from persistent storage:");
    read_logs(&disk_buffer);
    
    // Cleanup - delete the temporary file
    std::fs::remove_file(&disk_path).ok();
}

fn read_logs(buffer: &Arc<sherlog_ring_buffer::PersistentRingBuffer>) {
    // Create a cursor for reading
    let mut cursor = Cursor::new(buffer.clone(), None);
    
    // Read logs in batches of 50
    let mut total_count = 0;
    let mut info_count = 0;
    let mut warning_count = 0;
    let mut error_count = 0;
    
    while let Ok(batch) = cursor.read_batch(50) {
        if batch.is_empty() {
            break;
        }
        
        for record in batch {
            total_count += 1;
            
            // Count by tag
            match record.header.tag {
                TAG_INFO => info_count += 1,
                TAG_WARNING => warning_count += 1,
                TAG_ERROR => error_count += 1,
                _ => {}
            }
        }
    }
    
    println!("Read {} records from persistent storage:", total_count);
    println!("  - Info logs: {}", info_count);
    println!("  - Warning logs: {}", warning_count);
    println!("  - Error logs: {}", error_count);
}