//! Comprehensive tests for the in-memory volatile ring buffer

use sherlog_ring_buffer::{
    init_memory_buffer,
    get_memory_buffer,
    write,
    reserve,
    types::RecordHeader,
};
use std::sync::{Arc, Barrier};
use std::thread;

/// Test basic write and read functionality of the memory buffer
#[test]
fn test_basic_write_read() {
    // Initialize a small buffer for testing
    let buffer = init_memory_buffer(4096);
    
    // Simple message
    let message = "Hello, world!";
    let tag = 1;
    
    // Write to the buffer
    let success = write(message.as_bytes().to_vec(), tag);
    assert!(success, "Write operation should succeed");
    
    // Read from the buffer
    let records = buffer.read();
    
    // Verify the results
    assert_eq!(records.len(), 1, "Should have read one record");
    assert_eq!(records[0].data, message.as_bytes(), "Record data should match written data");
    assert_eq!(records[0].header.tag, tag, "Record tag should match written tag");
    
    // Verify timestamp is recent
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;
    
    assert!(records[0].header.timestamp_us <= now, "Timestamp should be before or equal to now");
    assert!(records[0].header.timestamp_us > now - 1_000_000, "Timestamp should be within the last second");
}

/// Test the reserve-write-commit pattern
#[test]
fn test_reserve_write_commit() {
    // Initialize a small buffer for testing
    let buffer = init_memory_buffer(4096);
    
    // Test data
    let data = "Test message for reservation".as_bytes();
    let tag = 2;
    
    // Reserve space
    let reservation = reserve(data.len(), true);
    assert!(reservation.is_some(), "Reservation should succeed");
    
    if let Some(mut reservation) = reservation {
        // Create header
        let mut header = RecordHeader::new(data.len() as u32, tag);
        header.update_crc(data);
        
        // Write and commit
        reservation.write(header, data);
        reservation.commit();
        
        // Read back
        let records = buffer.read();
        assert_eq!(records.len(), 1, "Should have read one record");
        assert_eq!(records[0].data, data, "Record data should match written data");
        assert_eq!(records[0].header.tag, tag, "Record tag should match written tag");
        assert_eq!(records[0].header.crc32, header.crc32, "CRC should match");
    }
}

/// Test CRC integrity checking
#[test]
fn test_crc_integrity() {
    // Initialize a buffer
    let buffer = init_memory_buffer(4096);
    
    // Create a record with known CRC
    let data = "Integrity test".as_bytes();
    let tag = 3;
    
    // Create a record with proper CRC
    let mut header = RecordHeader::new(data.len() as u32, tag);
    header.update_crc(data);
    
    // Use reservation to write directly
    let reservation = reserve(data.len(), true).unwrap();
    let mut reservation = reservation;
    reservation.write(header, data);
    reservation.commit();
    
    // Read back and verify
    let records = buffer.read();
    assert_eq!(records.len(), 1, "Should have read one record");
    
    // Verify CRC checking works
    assert!(records[0].header.verify_crc(&records[0].data), "CRC verification should pass");
    
    // Create a corrupted record
    let data = "Corrupted data".as_bytes();
    let tag = 4;
    
    // Create header but with wrong CRC (don't update it)
    let header = RecordHeader::new(data.len() as u32, tag); 
    // CRC will be 0, which should fail verification
    
    // Write it directly
    let reservation = reserve(data.len(), true).unwrap();
    let mut reservation = reservation;
    reservation.write(header, data);
    reservation.commit();
    
    // Read back
    let records = buffer.read();
    assert_eq!(records.len(), 1, "Should have read one record");
    
    // The record is still read because we're testing the public API
    // But internally, it should have detected the CRC mismatch
    assert!(!header.verify_crc(data), "CRC verification should fail for corrupted data");
}

/// Test multiple concurrent writers
#[test]
fn test_concurrent_writers() {
    const NUM_THREADS: usize = 8;
    const MSGS_PER_THREAD: usize = 100;
    
    // Initialize a larger buffer for concurrent testing
    let buffer = init_memory_buffer(1024 * 1024); // 1MB
    
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let mut handles = Vec::new();
    
    for t in 0..NUM_THREADS {
        let barrier_clone = barrier.clone();
        
        let handle = thread::spawn(move || {
            // Wait for all threads to be ready
            barrier_clone.wait();
            
            for i in 0..MSGS_PER_THREAD {
                let message = format!("Thread {} - Message {}", t, i);
                write(message.as_bytes().to_vec(), t as u32);
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Read all messages
    let records = buffer.read();
    
    // We should have NUM_THREADS * MSGS_PER_THREAD records
    assert_eq!(records.len(), NUM_THREADS * MSGS_PER_THREAD, 
               "Should have read all messages from all threads");
    
    // Verify all messages from all threads exist
    let mut counts = vec![0; NUM_THREADS];
    
    for record in records {
        let thread_id = record.header.tag as usize;
        assert!(thread_id < NUM_THREADS, "Thread ID should be valid");
        counts[thread_id] += 1;
    }
    
    // Each thread should have written exactly MSGS_PER_THREAD messages
    for (i, &count) in counts.iter().enumerate() {
        assert_eq!(count, MSGS_PER_THREAD, 
                   "Thread {} should have written {} messages", i, MSGS_PER_THREAD);
    }
}

/// Test buffer overflow handling
#[test]
fn test_buffer_overflow() {
    // Use a small buffer to trigger overflow
    let buffer = init_memory_buffer(1024); // 1KB
    
    // First, write a large number of small messages to fill the buffer
    for i in 0..50 {
        // Each message is around 20-30 bytes
        let message = format!("Filling message {}", i);
        write(message.as_bytes().to_vec(), 1);
    }
    
    // Now read the buffer to process everything
    let initial_records = buffer.read();
    let initial_count = initial_records.len();
    
    // Write more messages - some may go to overflow queue
    for i in 0..30 {
        let message = format!("Overflow test message {}", i);
        write(message.as_bytes().to_vec(), 2);
    }
    
    // Read again - the overflow queue should be processed
    let records = buffer.read();
    
    // We should have read something - exact count depends on buffer size
    assert!(records.len() > 0, "Should have read some records");
    
    // Assert that all messages with tag 2 have sequential numbers
    let tag2_records: Vec<_> = records.iter()
        .filter(|r| r.header.tag == 2)
        .map(|r| String::from_utf8_lossy(&r.data).to_string())
        .collect();
    
    for (i, msg) in tag2_records.iter().enumerate() {
        assert!(msg.contains(&format!("Overflow test message {}", i)), 
                "Messages should be in order: {}", msg);
    }
}

/// Test usage percentage calculation
#[test]
fn test_usage_percentage() {
    // Initialize a buffer with known size
    let buffer = init_memory_buffer(1024); // 1KB
    
    // Initially should be empty
    assert_eq!(buffer.usage_percent(), 0.0, "Empty buffer should have 0% usage");
    
    // Write some data to fill about 50% of the buffer
    for _ in 0..20 {
        write("12345678901234567890".as_bytes().to_vec(), 1); // 20 bytes each
    }
    
    // Check usage - should be approximately 50%
    // The exact value will depend on header sizes and alignment
    let usage = buffer.usage_percent();
    assert!(usage > 30.0, "Usage should be significant");
    assert!(usage < 70.0, "Usage should not exceed expected range");
    
    // Read everything
    buffer.read();
    
    // Usage should drop to 0% after reading
    assert_eq!(buffer.usage_percent(), 0.0, "After reading, usage should be 0%");
}

/// Test non-blocking reservation
#[test]
fn test_nonblocking_reservation() {
    // Create a small buffer
    let buffer = init_memory_buffer(256);
    
    // Fill most of the buffer
    for _ in 0..5 {
        write("1234567890".as_bytes().to_vec(), 1); // 10 bytes each + header
    }
    
    // Try a non-blocking reservation for a larger message
    let large_reservation = reserve(200, false);
    
    // Should fail due to insufficient space
    assert!(large_reservation.is_none(), "Large reservation should fail when buffer is nearly full");
    
    // Now try a blocking reservation from another thread
    let handle = thread::spawn(move || {
        // First trigger a buffer read in main thread
        thread::sleep(std::time::Duration::from_millis(100));
        
        // This should block until the main thread reads the buffer
        let reservation = reserve(200, true);
        assert!(reservation.is_some(), "Blocking reservation should eventually succeed");
    });
    
    // Wait a bit for the thread to start
    thread::sleep(std::time::Duration::from_millis(50));
    
    // Read the buffer to free up space
    buffer.read();
    
    // Wait for the thread to complete
    handle.join().unwrap();
}

/// Test consumer notification and waiting
#[test]
fn test_consumer_notification() {
    // Initialize buffer
    let buffer = init_memory_buffer(4096);
    
    // Start a reader thread that waits for data
    let buffer_clone = get_memory_buffer();
    
    let handle = thread::spawn(move || {
        // Wait for data with timeout
        let notified = buffer_clone.wait_for_data(Some(500));
        assert!(notified, "Consumer should be notified when data is written");
        
        // Read the data
        let records = buffer_clone.read();
        assert_eq!(records.len(), 1, "Should have read one record");
        assert_eq!(records[0].header.tag, 42, "Record tag should match");
    });
    
    // Give the thread time to start waiting
    thread::sleep(std::time::Duration::from_millis(50));
    
    // Write data which should notify the consumer
    write("Notification test".as_bytes().to_vec(), 42);
    
    // Wait for reader thread to complete
    handle.join().unwrap();
}

/// Test writing a variety of data sizes
#[test]
fn test_various_data_sizes() {
    // Initialize buffer
    let buffer = init_memory_buffer(8192);
    
    // Test with different data sizes
    let sizes = [0, 1, 10, 100, 1000, 3000];
    
    for (i, &size) in sizes.iter().enumerate() {
        // Create data of the specified size
        let data = vec![i as u8; size];
        
        // Write the data
        let success = write(data.clone(), i as u32);
        assert!(success, "Writing {} bytes should succeed", size);
    }
    
    // Read back and verify
    let records = buffer.read();
    assert_eq!(records.len(), sizes.len(), "Should have read all records");
    
    for (i, record) in records.iter().enumerate() {
        let expected_size = sizes[i];
        assert_eq!(record.data.len(), expected_size, "Record size should match");
        
        // For non-empty records, verify content
        if expected_size > 0 {
            assert_eq!(record.data[0], i as u8, "Data content should match");
        }
    }
}

/// Test edge cases for buffer capacity
#[test]
fn test_capacity_edge_cases() {
    // Test with small power-of-2 capacity
    let small_buffer = init_memory_buffer(16);
    assert_eq!(small_buffer.capacity(), 16, "Capacity should be 16");
    
    // Test with non-power-of-2 capacity (should be rounded up)
    let non_pow2_buffer = init_memory_buffer(100);
    assert_eq!(non_pow2_buffer.capacity(), 128, "Capacity should be rounded to 128");
    
    // Test with odd number
    let odd_buffer = init_memory_buffer(333);
    assert_eq!(odd_buffer.capacity(), 512, "Capacity should be rounded to 512");
    
    // Test zero capacity (should use minimum size)
    let zero_buffer = init_memory_buffer(0);
    assert!(zero_buffer.capacity() > 0, "Capacity should be at least 1");
    
    // Test maximum record size constraint
    let buffer = init_memory_buffer(1024);
    
    // Record size must be <= half the buffer capacity
    let large_reservation = reserve(buffer.capacity() / 2, false);
    assert!(large_reservation.is_some(), "Should allow reservation up to half the buffer size");
    
    let too_large_reservation = reserve(buffer.capacity() / 2 + 1, false);
    assert!(too_large_reservation.is_none(), "Should reject reservation larger than half the buffer size");
}