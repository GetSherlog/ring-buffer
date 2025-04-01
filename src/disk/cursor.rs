//! Cursor implementation for reading from the persistent ring buffer
//! 
//! This module provides a cursor-based interface for reading from the persistent
//! ring buffer. The cursor maintains position state, allowing for:
//! 
//! - Partial reads from the buffer (read some records, then continue later)
//! - Batched reading for efficient consumption
//! - Seeking to specific positions within the buffer
//! - Position tracking across buffer wrap-arounds
//! - Committing read positions back to the buffer
//! 
//! The cursor is designed to handle the complexities of a ring buffer, including
//! wrap-around conditions and the potential for data to be overwritten by new
//! writes if not consumed quickly enough.

use crate::disk::{persistent::PersistentRingBuffer, persistent::Result};
use crate::memory::Record;
use std::sync::Arc;

/// A position in the persistent ring buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Position in the buffer
    pub offset: u64,
    /// Wrap count when this position was created
    pub wrap_count: u64,
}

impl Position {
    /// Create a new position
    pub fn new(offset: u64, wrap_count: u64) -> Self {
        Self { offset, wrap_count }
    }
    
    /// Create a position from the beginning of the buffer
    pub fn begin() -> Self {
        Self { offset: 0, wrap_count: 0 }
    }
}

/// A cursor for reading records from the persistent ring buffer
pub struct Cursor {
    /// Reference to the buffer
    buffer: Arc<PersistentRingBuffer>,
    /// Current position in the buffer
    position: Position,
    /// Whether the cursor has reached the end of available data
    at_end: bool,
}

impl Cursor {
    /// Create a new cursor for reading from the buffer
    /// 
    /// # Arguments
    /// 
    /// * `buffer` - Reference to the persistent ring buffer
    /// * `position` - Starting position, or None to start from the current read position
    pub fn new(buffer: Arc<PersistentRingBuffer>, position: Option<Position>) -> Self {
        let control = buffer.control_block();
        
        let position = position.unwrap_or_else(|| {
            Position::new(control.read_pos, control.wrap_count)
        });
        
        // Check if the cursor is already at the end
        let at_end = position.offset == control.write_pos 
            && position.wrap_count == control.wrap_count;
        
        Self {
            buffer,
            position,
            at_end,
        }
    }
    
    /// Get the current position
    pub fn position(&self) -> Position {
        self.position
    }
    
    /// Check if the cursor has reached the end of available data
    pub fn at_end(&self) -> bool {
        self.at_end
    }
    
    /// Read the next record from the buffer
    pub fn next(&mut self) -> Result<Option<Record>> {
        if self.at_end {
            return Ok(None);
        }
        
        // Read the record at the current position
        match self.buffer.read_at(self.position.offset)? {
            Some((record, size)) => {
                // Advance the cursor
                self.position.offset += size as u64;
                
                // Check if we've reached the end
                let control = self.buffer.control_block();
                self.at_end = self.position.offset == control.write_pos
                    && self.position.wrap_count == control.wrap_count;
                
                Ok(Some(record))
            }
            None => {
                // No valid record found, advance past the header size and try again
                self.position.offset += crate::disk::Header::SIZE as u64;
                
                // Check if we've reached the end
                let control = self.buffer.control_block();
                self.at_end = self.position.offset >= control.write_pos
                    && self.position.wrap_count >= control.wrap_count;
                
                // If we're at the end, return None
                if self.at_end {
                    Ok(None)
                } else {
                    // Otherwise, try to read the next record
                    self.next()
                }
            }
        }
    }
    
    /// Read up to `max_count` records from the buffer
    /// 
    /// # Arguments
    /// 
    /// * `max_count` - Maximum number of records to read
    pub fn read_batch(&mut self, max_count: usize) -> Result<Vec<Record>> {
        let mut records = Vec::with_capacity(max_count);
        
        for _ in 0..max_count {
            match self.next()? {
                Some(record) => records.push(record),
                None => break,
            }
        }
        
        Ok(records)
    }
    
    /// Skip forward to the given position
    /// 
    /// # Arguments
    /// 
    /// * `position` - Position to skip to
    pub fn seek_to(&mut self, position: Position) -> Result<()> {
        // Get current control block to check if position is valid
        let control = self.buffer.control_block();
        
        // Check if position is in the future
        if position.wrap_count > control.wrap_count || 
           (position.wrap_count == control.wrap_count && position.offset > control.write_pos) {
            // Position is in the future, clamp to current end
            self.position = Position::new(control.write_pos, control.wrap_count);
            self.at_end = true;
        } else if position.wrap_count < control.wrap_count {
            // Position is in a previous wrap, data might be overwritten
            // Start from the beginning of the current wrap
            self.position = Position::new(0, control.wrap_count);
            self.at_end = false;
        } else {
            // Position is in the current wrap, use it directly
            self.position = position;
            self.at_end = position.offset == control.write_pos;
        }
        
        Ok(())
    }
    
    /// Skip to the end of available data
    pub fn seek_to_end(&mut self) -> Result<()> {
        let control = self.buffer.control_block();
        self.position = Position::new(control.write_pos, control.wrap_count);
        self.at_end = true;
        Ok(())
    }
    
    /// Commit the current position as the new read position in the buffer
    pub fn commit(&self) -> Result<()> {
        // Only if we're using the buffer's read position
        let mut control = self.buffer.control_block();
        
        // Update read position if our position is valid and ahead of the current read position
        if self.position.wrap_count == control.wrap_count && 
           self.position.offset > control.read_pos && 
           self.position.offset <= control.write_pos {
            control.read_pos = self.position.offset;
            
            // Write the updated control block
            let buffer_ref = Arc::as_ref(&self.buffer);
            buffer_ref.reset_read_position()?;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::memory::Record;
    
    #[test]
    fn test_cursor_basics() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cursor_test.dat");
        
        let buffer = Arc::new(PersistentRingBuffer::new(&path, 1024).unwrap());
        
        // Write some records
        for i in 0..5 {
            let data = format!("test {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(&record).unwrap();
        }
        
        // Create a cursor
        let mut cursor = Cursor::new(buffer.clone(), None);
        
        // Read all records
        let mut records = Vec::new();
        while let Some(record) = cursor.next().unwrap() {
            records.push(record);
        }
        
        assert_eq!(records.len(), 5);
        assert!(cursor.at_end());
        
        // Verify record contents
        for (i, record) in records.iter().enumerate() {
            let expected = format!("test {}", i);
            assert_eq!(record.data, expected.as_bytes());
        }
    }
    
    #[test]
    fn test_cursor_batch_read() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("batch_test.dat");
        
        let buffer = Arc::new(PersistentRingBuffer::new(&path, 1024).unwrap());
        
        // Write 10 records
        for i in 0..10 {
            let data = format!("batch {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(&record).unwrap();
        }
        
        // Create a cursor
        let mut cursor = Cursor::new(buffer.clone(), None);
        
        // Read in batches of 3
        let batch1 = cursor.read_batch(3).unwrap();
        assert_eq!(batch1.len(), 3);
        
        let batch2 = cursor.read_batch(3).unwrap();
        assert_eq!(batch2.len(), 3);
        
        let batch3 = cursor.read_batch(5).unwrap(); // Ask for 5, but only 4 remain
        assert_eq!(batch3.len(), 4);
        
        assert!(cursor.at_end());
        
        // Verify all records were read in the correct order
        for i in 0..3 {
            let expected = format!("batch {}", i);
            assert_eq!(batch1[i].data, expected.as_bytes());
        }
        
        for i in 0..3 {
            let expected = format!("batch {}", i + 3);
            assert_eq!(batch2[i].data, expected.as_bytes());
        }
        
        for i in 0..4 {
            let expected = format!("batch {}", i + 6);
            assert_eq!(batch3[i].data, expected.as_bytes());
        }
    }
    
    #[test]
    fn test_cursor_seek() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("seek_test.dat");
        
        let buffer = Arc::new(PersistentRingBuffer::new(&path, 1024).unwrap());
        
        // Write 5 records
        for i in 0..5 {
            let data = format!("seek {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(&record).unwrap();
        }
        
        let mut cursor = Cursor::new(buffer.clone(), None);
        
        // Read 2 records and remember the position
        cursor.next().unwrap();
        cursor.next().unwrap();
        let pos = cursor.position();
        
        // Read the rest
        while cursor.next().unwrap().is_some() {}
        
        // Seek back to the saved position
        cursor.seek_to(pos).unwrap();
        assert!(!cursor.at_end());
        
        // Read from this position
        let records = cursor.read_batch(10).unwrap();
        assert_eq!(records.len(), 3); // Should be 3 records left
        
        // Verify the records
        for i in 0..3 {
            let expected = format!("seek {}", i + 2);
            assert_eq!(records[i].data, expected.as_bytes());
        }
    }
    
    #[test]
    fn test_cursor_commit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("commit_test.dat");
        
        let buffer = Arc::new(PersistentRingBuffer::new(&path, 1024).unwrap());
        
        // Write 5 records
        for i in 0..5 {
            let data = format!("commit {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(&record).unwrap();
        }
        
        // Read 3 records with a cursor
        let mut cursor = Cursor::new(buffer.clone(), None);
        cursor.read_batch(3).unwrap();
        
        // Commit the position
        cursor.commit().unwrap();
        
        // Create a new cursor - it should start from the committed position
        let mut new_cursor = Cursor::new(buffer.clone(), None);
        let records = new_cursor.read_batch(10).unwrap();
        
        // Should only read the last 2 records
        assert_eq!(records.len(), 2);
        
        // Verify the records
        for i in 0..2 {
            let expected = format!("commit {}", i + 3);
            assert_eq!(records[i].data, expected.as_bytes());
        }
    }
}