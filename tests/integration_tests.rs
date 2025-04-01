//! Integration tests for the full Sherlog Ring Buffer system

use sherlog_ring_buffer::{
    init_memory_buffer,
    init_persistent_buffer,
    FlushDaemonConfig,
    start_flush_daemon,
    stop_flush_daemon,
    write,
    Cursor,
};
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};
use std::time::Duration;
use std::thread;

struct TestContext {
    _temp_dir: TempDir,
    buffer_path: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempdir().unwrap();
        let buffer_path = temp_dir.path().join("test_integration.dat");
        
        Self {
            _temp_dir: temp_dir,
            buffer_path,
        }
    }
}

/// Test the complete flow of initialization, writing, flushing, and reading
#[test]
fn test_full_system_flow() {
    let context = TestContext::new();
    
    // Initialize memory buffer
    let memory_buffer = init_memory_buffer(4096);
    
    // Initialize persistent buffer
    let disk_buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Configure and start flush daemon
    let config = FlushDaemonConfig {
        interval_ms: 100,
        high_watermark_percent: 50.0,
        max_records_per_flush: 10,
    };
    
    let daemon = start_flush_daemon(config);
    
    // Write some records to memory buffer
    for i in 0..10 {
        let data = format!("Integration test record {}", i).as_bytes().to_vec();
        assert!(write(data, i), "Write should succeed");
    }
    
    // Wait for flush daemon to process
    thread::sleep(Duration::from_millis(200));
    
    // Read from disk buffer using cursor
    let mut cursor = Cursor::new(disk_buffer.clone(), None);
    let records = cursor.read_batch(20).unwrap();
    
    // Verify records were flushed to disk
    assert!(!records.is_empty(), "Should have flushed some records to disk");
    
    // Check record contents
    for (i, record) in records.iter().enumerate() {
        let expected = format!("Integration test record {}", i).as_bytes().to_vec();
        assert_eq!(record.data, expected, "Record data should match");
        assert_eq!(record.header.tag, i as u32, "Record tag should match");
    }
    
    // Stop the flush daemon
    stop_flush_daemon();
    let _ = daemon.join();
}

/// Test high throughput with many concurrent writers
#[test]
fn test_high_throughput() {
    let context = TestContext::new();
    
    // Initialize with larger buffers for throughput testing
    let memory_buffer = init_memory_buffer(1024 * 1024); // 1MB
    let disk_buffer = init_persistent_buffer(&context.buffer_path, 10 * 1024 * 1024); // 10MB
    
    // Configure and start flush daemon
    let config = FlushDaemonConfig {
        interval_ms: 50,
        high_watermark_percent: 70.0,
        max_records_per_flush: 1000,
    };
    
    let daemon = start_flush_daemon(config);
    
    // Create many writer threads
    const NUM_THREADS: usize = 4;
    const MSGS_PER_THREAD: usize = 1000;
    
    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|thread_id| {
            thread::spawn(move || {
                for i in 0..MSGS_PER_THREAD {
                    let message = format!("Thread {} - Message {}", thread_id, i);
                    write(message.as_bytes().to_vec(), thread_id as u32);
                    
                    // Occasional sleep to simulate realistic workload
                    if i % 100 == 0 {
                        thread::sleep(Duration::from_micros(10));
                    }
                }
            })
        })
        .collect();
    
    // Wait for all writers to complete
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Wait for daemon to process all messages
    thread::sleep(Duration::from_millis(500));
    
    // Read all messages from disk
    let mut cursor = Cursor::new(disk_buffer.clone(), None);
    let mut record_count = 0;
    let mut thread_counts = vec![0; NUM_THREADS];
    
    while let Ok(Some(record)) = cursor.next() {
        let thread_id = record.header.tag as usize;
        if thread_id < NUM_THREADS {
            thread_counts[thread_id] += 1;
        }
        record_count += 1;
    }
    
    // Verify we got records from all threads
    println!("Total records: {}", record_count);
    println!("Thread counts: {:?}", thread_counts);
    
    // We should have most records, though some may still be in memory buffer
    // or overflow queue, especially under heavy load
    assert!(record_count > NUM_THREADS * MSGS_PER_THREAD / 2, 
            "Should have processed at least half of all records");
    
    // Verify we got records from each thread
    for (i, &count) in thread_counts.iter().enumerate() {
        assert!(count > 0, "Should have records from thread {}", i);
    }
    
    // Stop the flush daemon
    stop_flush_daemon();
    let _ = daemon.join();
}

/// Test the system's behavior under memory pressure
#[test]
fn test_memory_pressure() {
    let context = TestContext::new();
    
    // Initialize with a small memory buffer to trigger overflow
    let memory_buffer = init_memory_buffer(1024); // Just 1KB
    let disk_buffer = init_persistent_buffer(&context.buffer_path, 1024 * 1024); // 1MB
    
    // Configure flush daemon with longer interval to test overflow behavior
    let config = FlushDaemonConfig {
        interval_ms: 200, // Longer interval
        high_watermark_percent: 80.0,
        max_records_per_flush: 10,
    };
    
    let daemon = start_flush_daemon(config);
    
    // Write many records to fill the small buffer
    const NUM_RECORDS: usize = 100;
    
    for i in 0..NUM_RECORDS {
        let data = format!("Overflow test record {}", i).as_bytes().to_vec();
        write(data, i as u32);
    }
    
    // Wait for flush daemon to process
    thread::sleep(Duration::from_millis(500));
    
    // Read from disk buffer
    let mut cursor = Cursor::new(disk_buffer.clone(), None);
    let records = cursor.read_batch(NUM_RECORDS).unwrap();
    
    // We should have some records, but likely not all due to overflow
    println!("Records flushed to disk: {}", records.len());
    assert!(!records.is_empty(), "Should have flushed some records");
    
    // The records we do have should be valid
    for record in &records {
        let id = record.header.tag as usize;
        if id < NUM_RECORDS {
            let expected = format!("Overflow test record {}", id).as_bytes().to_vec();
            assert_eq!(record.data, expected, "Record data should match");
        }
    }
    
    // Stop the flush daemon
    stop_flush_daemon();
    let _ = daemon.join();
}

/// Test recovery after simulated crash
#[test]
fn test_crash_recovery() {
    let context = TestContext::new();
    let path = context.buffer_path.clone();
    
    // First session - initialize and write
    {
        let memory_buffer = init_memory_buffer(4096);
        let disk_buffer = init_persistent_buffer(&path, 10240);
        let config = FlushDaemonConfig::default();
        let daemon = start_flush_daemon(config);
        
        // Write some records
        for i in 0..5 {
            let data = format!("Pre-crash record {}", i).as_bytes().to_vec();
            write(data, i);
        }
        
        // Wait for flush
        thread::sleep(Duration::from_millis(200));
        
        // Simulate crash by not properly shutting down
        // (just let everything drop)
    }
    
    // Second session - recover and continue
    {
        let memory_buffer = init_memory_buffer(4096);
        let disk_buffer = init_persistent_buffer(&path, 0); // Reuse existing file
        let config = FlushDaemonConfig::default();
        let daemon = start_flush_daemon(config);
        
        // Read what was persisted before the crash
        let mut cursor = Cursor::new(disk_buffer.clone(), None);
        let pre_crash_records = cursor.read_batch(10).unwrap();
        
        // We should have the pre-crash records
        assert!(!pre_crash_records.is_empty(), "Should recover pre-crash records");
        
        // Write more records after recovery
        for i in 0..5 {
            let data = format!("Post-crash record {}", i).as_bytes().to_vec();
            write(data, i + 10); // Different tags
        }
        
        // Wait for flush
        thread::sleep(Duration::from_millis(200));
        
        // Properly shut down this time
        stop_flush_daemon();
        let _ = daemon.join();
    }
    
    // Third session - verify all records
    {
        let disk_buffer = init_persistent_buffer(&path, 0);
        let mut cursor = Cursor::new(disk_buffer.clone(), None);
        let all_records = cursor.read_batch(20).unwrap();
        
        // Count pre-crash and post-crash records
        let mut pre_crash = 0;
        let mut post_crash = 0;
        
        for record in &all_records {
            let tag = record.header.tag;
            if tag < 10 {
                pre_crash += 1;
            } else {
                post_crash += 1;
            }
        }
        
        // We should have both types of records
        assert!(pre_crash > 0, "Should have pre-crash records");
        assert!(post_crash > 0, "Should have post-crash records");
    }
}

/// Test very large record handling
#[test]
fn test_large_records() {
    let context = TestContext::new();
    
    // Initialize with sufficient buffer sizes
    let memory_buffer = init_memory_buffer(1024 * 1024); // 1MB
    let disk_buffer = init_persistent_buffer(&context.buffer_path, 10 * 1024 * 1024); // 10MB
    
    let config = FlushDaemonConfig::default();
    let daemon = start_flush_daemon(config);
    
    // Write records of various sizes
    let sizes = [10, 100, 1000, 10000, 100000, 400000]; // Up to ~400KB
    
    for (i, &size) in sizes.iter().enumerate() {
        // Generate data of the specified size
        let data = vec![i as u8; size];
        
        // Write it
        write(data, i as u32);
        
        // For larger records, give the daemon time to flush
        if size > 10000 {
            thread::sleep(Duration::from_millis(50));
        }
    }
    
    // Wait for flush daemon to process all
    thread::sleep(Duration::from_millis(500));
    
    // Read and verify records
    let mut cursor = Cursor::new(disk_buffer.clone(), None);
    let mut verified_sizes = vec![false; sizes.len()];
    
    while let Ok(Some(record)) = cursor.next() {
        let tag = record.header.tag as usize;
        if tag < sizes.len() {
            let expected_size = sizes[tag];
            if record.data.len() == expected_size {
                verified_sizes[tag] = true;
            }
        }
    }
    
    // Check that we've verified each size
    // Note: The largest sizes might not fit in the buffer and may be dropped
    for (i, &verified) in verified_sizes.iter().enumerate() {
        if sizes[i] <= memory_buffer.capacity() / 2 {
            assert!(verified, "Record of size {} should be processed", sizes[i]);
        }
    }
    
    // Stop the flush daemon
    stop_flush_daemon();
    let _ = daemon.join();
}