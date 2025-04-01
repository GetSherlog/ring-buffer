//! Comprehensive tests for the persistent disk-based ring buffer

use sherlog_ring_buffer::{
    init_persistent_buffer,
    types::Record,
    Cursor,
};
use std::sync::Arc;
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};
use std::fs::{self, File};
use std::io::Read;

// Helper struct to manage temporary test directories
struct TestContext {
    _temp_dir: TempDir,       // Keep the TempDir alive for the test duration
    buffer_path: PathBuf,     // Path to the buffer file
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempdir().unwrap();
        let buffer_path = temp_dir.path().join("test_buffer.dat");
        
        Self {
            _temp_dir: temp_dir,
            buffer_path,
        }
    }
}

/// Test creating a new persistent buffer
#[test]
fn test_create_buffer() {
    let context = TestContext::new();
    
    // Create a new buffer
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Verify the buffer file exists
    assert!(context.buffer_path.exists(), "Buffer file should exist");
    
    // Verify the file size (should include control block)
    let metadata = fs::metadata(&context.buffer_path).unwrap();
    assert!(metadata.len() > 10240, "File size should be at least the requested size");
    
    // Check buffer properties
    assert_eq!(buffer.size(), 10240, "Buffer size should match requested size");
    assert_eq!(buffer.path(), context.buffer_path, "Buffer path should match");
    
    // Get and check the control block
    let control = buffer.control_block();
    assert_eq!(control.buffer_size, 10240, "Control block should have correct size");
    assert_eq!(control.write_pos, 0, "Write position should be 0");
    assert_eq!(control.read_pos, 0, "Read position should be 0");
    assert_eq!(control.wrap_count, 0, "Wrap count should be 0");
    assert!(control.verify_crc(), "Control block CRC should be valid");
}

/// Test basic write and read operations
#[test]
fn test_basic_write_read() {
    let context = TestContext::new();
    
    // Create a buffer
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Create a test record
    let test_data = "Test message".as_bytes().to_vec();
    let tag = 1;
    let record = Record::new(test_data.clone(), tag);
    
    // Write the record
    buffer.write(&record).unwrap();
    
    // Flush to ensure data is written to disk
    buffer.flush().unwrap();
    
    // Read all records
    let records = buffer.read_all().unwrap();
    
    // Verify results
    assert_eq!(records.len(), 1, "Should have read one record");
    assert_eq!(records[0].data, test_data, "Record data should match");
    assert_eq!(records[0].header.tag, tag, "Record tag should match");
    
    // Get the updated control block
    let control = buffer.control_block();
    assert!(control.write_pos > 0, "Write position should be advanced");
    assert!(control.read_pos > 0, "Read position should be advanced");
    assert_eq!(control.read_pos, control.write_pos, "Read should have caught up to write");
}

/// Test writing multiple records
#[test]
fn test_multiple_records() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Write several records
    for i in 0..10 {
        let data = format!("Record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    // Flush to disk
    buffer.flush().unwrap();
    
    // Read all records
    let records = buffer.read_all().unwrap();
    
    // Verify results
    assert_eq!(records.len(), 10, "Should have read all 10 records");
    
    for (i, record) in records.iter().enumerate() {
        let expected = format!("Record {}", i).as_bytes().to_vec();
        assert_eq!(record.data, expected, "Record data should match");
        assert_eq!(record.header.tag, i as u32, "Record tag should match");
    }
}

/// Test reopening an existing buffer
#[test]
fn test_reopen_buffer() {
    let context = TestContext::new();
    
    // Create and write to a buffer
    {
        let buffer = init_persistent_buffer(&context.buffer_path, 10240);
        
        // Write a record
        let data = "Persistence test".as_bytes().to_vec();
        let record = Record::new(data, 42);
        buffer.write(&record).unwrap();
        buffer.flush().unwrap();
    }
    
    // Reopen the same buffer file
    let buffer = init_persistent_buffer(&context.buffer_path, 0); // Size is ignored for existing files
    
    // Verify the buffer size is preserved
    assert_eq!(buffer.size(), 10240, "Buffer size should be preserved");
    
    // Read the records
    let records = buffer.read_all().unwrap();
    
    // Verify the data persisted
    assert_eq!(records.len(), 1, "Should have read the persisted record");
    assert_eq!(records[0].data, "Persistence test".as_bytes().to_vec(), "Record data should match");
    assert_eq!(records[0].header.tag, 42, "Record tag should match");
}

/// Test cursor-based reading
#[test]
fn test_cursor_reading() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Write several records
    for i in 0..10 {
        let data = format!("Cursor record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Create a cursor
    let cursor = Cursor::new(buffer.clone(), None);
    
    // Read records one by one
    let mut cursor = cursor;
    let mut count = 0;
    
    while let Ok(Some(record)) = cursor.next() {
        let expected = format!("Cursor record {}", count).as_bytes().to_vec();
        assert_eq!(record.data, expected, "Record data should match");
        count += 1;
    }
    
    assert_eq!(count, 10, "Should have read all 10 records");
    assert!(cursor.at_end(), "Cursor should be at end");
}

/// Test cursor batch reading
#[test]
fn test_cursor_batch_reading() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Write 20 records
    for i in 0..20 {
        let data = format!("Batch record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i % 4);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Create a cursor
    let mut cursor = Cursor::new(buffer.clone(), None);
    
    // Read in batches of 5
    let batch1 = cursor.read_batch(5).unwrap();
    assert_eq!(batch1.len(), 5, "First batch should have 5 records");
    
    let batch2 = cursor.read_batch(5).unwrap();
    assert_eq!(batch2.len(), 5, "Second batch should have 5 records");
    
    let batch3 = cursor.read_batch(5).unwrap();
    assert_eq!(batch3.len(), 5, "Third batch should have 5 records");
    
    let batch4 = cursor.read_batch(5).unwrap();
    assert_eq!(batch4.len(), 5, "Fourth batch should have 5 records");
    
    let batch5 = cursor.read_batch(5).unwrap();
    assert_eq!(batch5.len(), 0, "Fifth batch should be empty");
    
    // Verify all records were read in order
    for i in 0..5 {
        let expected = format!("Batch record {}", i).as_bytes().to_vec();
        assert_eq!(batch1[i].data, expected, "Batch 1 record {} data should match", i);
    }
    
    for i in 0..5 {
        let expected = format!("Batch record {}", i + 5).as_bytes().to_vec();
        assert_eq!(batch2[i].data, expected, "Batch 2 record {} data should match", i);
    }
}

/// Test cursor seeking
#[test]
fn test_cursor_seeking() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Write records
    for i in 0..10 {
        let data = format!("Seek record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Create a cursor and read first 3 records
    let mut cursor = Cursor::new(buffer.clone(), None);
    let _ = cursor.read_batch(3).unwrap();
    
    // Remember the position after 3 records
    let position = cursor.position();
    
    // Read to the end
    while let Ok(Some(_)) = cursor.next() {}
    
    // Seek back to saved position
    cursor.seek_to(position).unwrap();
    
    // Read remaining records
    let remaining = cursor.read_batch(10).unwrap();
    
    // Should be 7 records left
    assert_eq!(remaining.len(), 7, "Should be 7 records after seeking back");
    
    // Verify they're the right records
    for i in 0..7 {
        let expected = format!("Seek record {}", i + 3).as_bytes().to_vec();
        assert_eq!(remaining[i].data, expected, "Record data after seek should match");
    }
}

/// Test cursor position commit
#[test]
fn test_cursor_commit() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Write records
    for i in 0..10 {
        let data = format!("Commit record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Read 5 records with a cursor
    let mut cursor = Cursor::new(buffer.clone(), None);
    let _ = cursor.read_batch(5).unwrap();
    
    // Commit the position
    cursor.commit().unwrap();
    
    // Create a new cursor - it should start from the committed position
    let mut new_cursor = Cursor::new(buffer.clone(), None);
    let remaining = new_cursor.read_batch(10).unwrap();
    
    // Should only read the remaining 5 records
    assert_eq!(remaining.len(), 5, "Should only read 5 records after commit");
    
    // Verify they're the right records
    for i in 0..5 {
        let expected = format!("Commit record {}", i + 5).as_bytes().to_vec();
        assert_eq!(remaining[i].data, expected, "Record data after commit should match");
    }
}

/// Test buffer wrapping
#[test]
fn test_buffer_wrapping() {
    let context = TestContext::new();
    
    // Create a small buffer to force wrapping
    let buffer = init_persistent_buffer(&context.buffer_path, 512);
    
    // Write records until we force a wrap
    for i in 0..20 {
        let data = format!("Wrap record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Check if wrap occurred
    let control = buffer.control_block();
    assert!(control.wrap_count > 0, "Buffer should have wrapped");
    
    // Read all available records
    let mut cursor = Cursor::new(buffer.clone(), None);
    let records = cursor.read_batch(100).unwrap();
    
    // We won't necessarily have all 20 records due to wrapping,
    // but we should have some records and they should be valid
    assert!(!records.is_empty(), "Should have read some records after wrap");
    
    // The first record should be the oldest one that wasn't overwritten
    let first_id = records[0].header.tag as usize;
    
    // Verify records are in sequence
    for (i, record) in records.iter().enumerate() {
        let expected_id = first_id + i;
        if expected_id < 20 { // Only check records that were written
            let expected = format!("Wrap record {}", expected_id).as_bytes().to_vec();
            assert_eq!(record.data, expected, "Record after wrap should match");
        }
    }
}

/// Test error handling for invalid files
#[test]
fn test_error_handling() {
    let context = TestContext::new();
    
    // Create an invalid file
    {
        let mut file = File::create(&context.buffer_path).unwrap();
        file.write_all(&[0u8; 64]).unwrap(); // Just some zeros, not a valid control block
    }
    
    // Trying to open it should fail or trigger recovery
    let result = sherlog_ring_buffer::PersistentRingBuffer::new(&context.buffer_path, 1024);
    assert!(result.is_err(), "Opening invalid file should fail");
}

/// Test resetting read position
#[test]
fn test_reset_read_position() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Write some records
    for i in 0..5 {
        let data = format!("Reset record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Read all records to advance read position
    let _ = buffer.read_all().unwrap();
    
    // Verify read position is at write position
    let control = buffer.control_block();
    assert_eq!(control.read_pos, control.write_pos, "Read position should be at write position");
    
    // Write more records
    for i in 5..10 {
        let data = format!("Reset record {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Reset read position to skip earlier records
    buffer.reset_read_position().unwrap();
    
    // Verify read position is at write position
    let control = buffer.control_block();
    assert_eq!(control.read_pos, control.write_pos, "Read position should be reset to write position");
    
    // Read again - should get no records
    let records = buffer.read_all().unwrap();
    assert_eq!(records.len(), 0, "Should read no records after reset");
}

/// Test reading corrupted records
#[test]
fn test_corrupted_records() {
    let context = TestContext::new();
    let buffer_path = context.buffer_path.clone();
    
    // Create a buffer and write records
    let buffer = init_persistent_buffer(&buffer_path, 1024);
    
    let data = "Good record".as_bytes().to_vec();
    let record = Record::new(data, 1);
    buffer.write(&record).unwrap();
    buffer.flush().unwrap();
    
    // Get the current control block state
    let control = buffer.control_block();
    let write_pos = control.write_pos;
    
    // Manually corrupt the file after the header (Windows needs file to be closed)
    drop(buffer);
    
    // Reopen the file and corrupt it
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&buffer_path)
        .unwrap();
    
    // Seek to the data portion of the record
    file.seek(std::io::SeekFrom::Start(64 + write_pos + 16)).unwrap(); // Skip control block + record header
    
    // Write some garbage
    file.write_all(&[0xFF; 10]).unwrap();
    file.flush().unwrap();
    drop(file);
    
    // Reopen the buffer
    let buffer = init_persistent_buffer(&buffer_path, 0);
    
    // Try to read - should either skip the corrupted record or return an error
    let mut cursor = Cursor::new(buffer.clone(), None);
    let result = cursor.next();
    
    // The implementation may handle corruption in different ways
    // We're just checking that it doesn't panic
    match result {
        Ok(Some(_)) => {
            // If a record is returned, it should be marked as corrupted or repaired
            // This depends on the implementation details
        },
        Ok(None) => {
            // Corrupted record might be skipped
        },
        Err(_) => {
            // Error is also a valid response to corruption
        }
    }
    
    // Write a new good record
    let data = "After corruption".as_bytes().to_vec();
    let record = Record::new(data, 2);
    buffer.write(&record).unwrap();
    buffer.flush().unwrap();
    
    // We should be able to read the new record
    let mut cursor = Cursor::new(buffer.clone(), None);
    cursor.seek_to_end().unwrap(); // Skip any corrupted data
    
    // Now seek back a bit and try to read the newest record
    let control = buffer.control_block();
    let new_pos = control.write_pos - 64; // Approximate size of the new record
    let _ = cursor.seek_to(crate::disk::cursor::Position::new(new_pos, control.wrap_count));
    
    // Read forward
    let mut found_new_record = false;
    while let Ok(Some(record)) = cursor.next() {
        if record.header.tag == 2 && record.data == "After corruption".as_bytes() {
            found_new_record = true;
            break;
        }
    }
    
    assert!(found_new_record, "Should be able to read new record after corruption");
}

/// Test very large records
#[test]
fn test_large_records() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 100 * 1024); // 100KB
    
    // Create records of increasing size
    let sizes = [1, 10, 100, 1000, 10000, 50000];
    
    for (i, &size) in sizes.iter().enumerate() {
        // Generate data of the specified size
        let data = vec![i as u8; size];
        let record = Record::new(data.clone(), i as u32);
        
        // Write it
        buffer.write(&record).unwrap();
    }
    
    buffer.flush().unwrap();
    
    // Read back and verify
    let mut cursor = Cursor::new(buffer.clone(), None);
    let mut count = 0;
    
    while let Ok(Some(record)) = cursor.next() {
        let expected_size = sizes[count];
        let expected_tag = count as u32;
        
        assert_eq!(record.data.len(), expected_size, "Record size should match");
        assert_eq!(record.header.tag, expected_tag, "Record tag should match");
        
        // Verify content for non-empty records
        if expected_size > 0 {
            assert_eq!(record.data[0], count as u8, "First byte should match");
        }
        
        count += 1;
    }
    
    assert_eq!(count, sizes.len(), "Should have read all records");
}

/// Test flush behavior
#[test]
fn test_flush() {
    let context = TestContext::new();
    let buffer = init_persistent_buffer(&context.buffer_path, 10240);
    
    // Get initial file size
    let initial_size = fs::metadata(&context.buffer_path).unwrap().len();
    
    // Write record without flushing
    let data = "Unflushed record".as_bytes().to_vec();
    let record = Record::new(data, 1);
    buffer.write(&record).unwrap();
    
    // The data should still be in the memory map but not necessarily on disk yet
    // So explicitly flush it
    buffer.flush().unwrap();
    
    // Reopen buffer to force reading from disk
    drop(buffer);
    let buffer = init_persistent_buffer(&context.buffer_path, 0);
    
    // Try to read the record
    let records = buffer.read_all().unwrap();
    assert_eq!(records.len(), 1, "Should read the flushed record");
    assert_eq!(records[0].data, "Unflushed record".as_bytes(), "Record data should match");
}

/// Test multiple opens and closes
#[test]
fn test_multiple_opens() {
    let context = TestContext::new();
    let path = context.buffer_path.clone();
    
    // Create and use several buffers in sequence
    for i in 0..5 {
        // Open buffer
        let buffer = init_persistent_buffer(&path, 1024);
        
        // Write a record
        let data = format!("Open {}", i).as_bytes().to_vec();
        let record = Record::new(data, i);
        buffer.write(&record).unwrap();
        buffer.flush().unwrap();
        
        // Close buffer by dropping
        drop(buffer);
    }
    
    // Open one more time to read all records
    let buffer = init_persistent_buffer(&path, 0);
    let records = buffer.read_all().unwrap();
    
    assert_eq!(records.len(), 5, "Should read all records from multiple opens");
    
    for i in 0..5 {
        let expected = format!("Open {}", i).as_bytes().to_vec();
        assert_eq!(records[i].data, expected, "Record from open {} should match", i);
    }
}