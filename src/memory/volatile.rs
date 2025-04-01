//! Volatile MPSC ring buffer implementation for in-memory logging
//! 
//! This module implements a lock-free, multi-producer single-consumer (MPSC) ring buffer
//! designed for high-throughput in-memory logging. Key features include:
//! 
//! - Lock-free writes using atomic operations for multiple concurrent writers
//! - Reserve-Write-Commit pattern to allow efficient, zero-copy writing
//! - Overflow queue to handle buffer-full scenarios
//! - Condition variable for consumer notification
//! - Cache-line padding to prevent false sharing
//! 
//! The implementation uses a three-index approach:
//! - `write_idx`: Where new writes are reserved
//! - `commit_idx`: Which records are visible to consumers
//! - `read_idx`: What has been consumed
//! 
//! When the buffer is full, records are placed in an overflow queue and
//! written to the buffer as space becomes available.

use crate::memory::{AtomicIndex, Record, RecordHeader};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use parking_lot::{Condvar, RwLock};
use std::collections::VecDeque;

/// Lock-free reservation in the ring buffer
pub struct Reservation {
    /// Index where this reservation starts
    start_idx: usize,
    /// Size of the reservation
    size: usize,
    /// Reference to the parent buffer
    buffer: Arc<VolatileRingBuffer>,
    /// Whether this reservation has been committed
    committed: bool,
}

impl Reservation {
    /// Write data to this reservation
    /// 
    /// # Arguments
    /// 
    /// * `header` - Record header with metadata
    /// * `data` - Data to write into the reservation
    /// 
    /// # Panics
    /// 
    /// Panics if data size is larger than the reservation size
    pub fn write(&mut self, header: RecordHeader, data: &[u8]) {
        assert!(data.len() <= self.size - RecordHeader::SIZE, 
                "Data is larger than reservation");
                
        // Write the header
        unsafe {
            let header_ptr = self.buffer.buffer.as_ptr().add(self.start_idx) as *mut RecordHeader;
            *header_ptr = header;
        }
        
        // Write the data
        unsafe {
            let data_ptr = self.buffer.buffer.as_ptr()
                .add(self.start_idx + RecordHeader::SIZE) as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());
        }
    }
    
    /// Commit this reservation, making it visible to consumers
    pub fn commit(mut self) {
        if !self.committed {
            // Update the commit index to make this record visible
            self.buffer.advance_commit_index(self.start_idx, self.size);
            self.committed = true;
            
            // Notify any waiting consumers
            self.buffer.notify_consumer();
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // If dropping without committing, release the space
        if !self.committed {
            // Optional: We could add a "cleanup" mechanism here to mark uncommitted
            // reservations as free space, but for simplicity we'll leave this space
            // as "dead" until it's naturally overwritten
        }
    }
}

/// Multi-producer, single-consumer ring buffer for volatile memory
pub struct VolatileRingBuffer {
    /// Raw buffer storage
    buffer: Box<[u8]>,
    /// Production write index
    write_idx: AtomicIndex,
    /// Consumption read index
    read_idx: AtomicIndex,
    /// Commit index - marking records as ready to be consumed
    commit_idx: AtomicIndex,
    /// Buffer capacity (power of 2)
    capacity: usize,
    /// Condition variable for notifying consumer about new data
    consumer_signal: Arc<(parking_lot::Mutex<bool>, parking_lot::Condvar)>,
    /// Lock for single-consumer reads
    consumer_lock: RwLock<()>,
    /// Queue for writing log entries when buffer is full
    overflow_queue: RwLock<VecDeque<Record>>,
    /// Maximum overflow queue size
    max_overflow: usize,
}

impl VolatileRingBuffer {
    /// Create a new volatile ring buffer with the given capacity
    /// 
    /// Capacity is rounded up to the next power of 2
    pub fn new(capacity: usize) -> Self {
        // Round up to the next power of 2
        let capacity = capacity.next_power_of_two();
        
        // Allocate buffer memory
        let buffer = vec![0u8; capacity].into_boxed_slice();
        
        Self {
            buffer,
            write_idx: AtomicIndex::new(capacity),
            read_idx: AtomicIndex::new(capacity),
            commit_idx: AtomicIndex::new(capacity),
            capacity,
            consumer_signal: Arc::new((parking_lot::Mutex::new(false), parking_lot::Condvar::new())),
            consumer_lock: RwLock::new(()),
            overflow_queue: RwLock::new(VecDeque::new()),
            max_overflow: 1000, // Default max overflow records
        }
    }
    
    /// Reserve space in the buffer for writing a record
    /// 
    /// # Arguments
    /// 
    /// * `size` - Size of the data to be written (not including header)
    /// * `block` - Whether to block if the buffer is full
    /// 
    /// # Returns
    /// 
    /// Some(Reservation) if space was successfully reserved, None if the buffer is full
    /// and `block` is false
    pub fn reserve(&self, size: usize, block: bool) -> Option<Reservation> {
        let total_size = size + RecordHeader::SIZE;
        assert!(total_size <= self.capacity / 2, 
                "Record too large for buffer - must be <= half capacity");
        
        // Try to reserve space
        loop {
            let write = self.write_idx.load(Ordering::Relaxed);
            let read = self.read_idx.load(Ordering::Acquire);
            
            // Calculate available space
            let available = if write >= read {
                self.capacity - (write - read)
            } else {
                read - write
            };
            
            // Check if we have enough space
            if available <= total_size {
                // Not enough space
                if !block {
                    return None;
                }
                
                // Wait for space to become available
                std::thread::yield_now();
                continue;
            }
            
            // Try to reserve space by advancing write_idx
            let new_write = self.write_idx.wrap(write + total_size);
            match self.write_idx.compare_exchange(
                write, new_write, 
                Ordering::Release, Ordering::Relaxed
            ) {
                Ok(_) => {
                    // Successfully reserved space
                    return Some(Reservation {
                        start_idx: write,
                        size: total_size,
                        buffer: Arc::new(self.clone()),
                        committed: false,
                    });
                }
                Err(_) => {
                    // Another thread reserved space first, try again
                    std::thread::yield_now();
                    continue;
                }
            }
        }
    }
    
    /// Write a record to the buffer, either directly or to the overflow queue
    /// 
    /// # Arguments
    /// 
    /// * `record` - The record to write
    /// 
    /// # Returns
    /// 
    /// True if the record was written directly to the buffer, false if it was queued
    pub fn write(&self, record: Record) -> bool {
        // Try to reserve space
        if let Some(mut reservation) = self.reserve(record.data.len(), false) {
            // Write directly to the buffer
            reservation.write(record.header, &record.data);
            reservation.commit();
            true
        } else {
            // Buffer is full, try to add to overflow queue
            let mut queue = self.overflow_queue.write();
            if queue.len() < self.max_overflow {
                queue.push_back(record);
            } else {
                // Both buffer and overflow queue are full, drop the oldest overflow record
                if !queue.is_empty() {
                    queue.pop_front();
                    queue.push_back(record);
                }
                // If queue is somehow empty (race condition), just drop the record
            }
            false
        }
    }
    
    /// Advance the commit index to make a record visible to consumers
    fn advance_commit_index(&self, start_idx: usize, size: usize) {
        let mut current_commit = self.commit_idx.load(Ordering::Relaxed);
        let new_commit = self.commit_idx.wrap(start_idx + size);
        
        // Keep trying until we successfully update the commit index
        while let Err(actual) = self.commit_idx.compare_exchange(
            current_commit, new_commit,
            Ordering::Release, Ordering::Relaxed
        ) {
            current_commit = actual;
        }
    }
    
    /// Notify the consumer that new data is available
    fn notify_consumer(&self) {
        let (lock, cvar) = &*self.consumer_signal;
        let mut ready = lock.lock();
        *ready = true;
        cvar.notify_one();
    }
    
    /// Wait for new data to be available
    /// 
    /// # Arguments
    /// 
    /// * `timeout_ms` - Maximum time to wait in milliseconds, or None to wait indefinitely
    pub fn wait_for_data(&self, timeout_ms: Option<u64>) -> bool {
        let (lock, cvar) = &*self.consumer_signal;
        let mut ready = lock.lock();
        
        if *ready {
            *ready = false;
            return true;
        }
        
        match timeout_ms {
            Some(timeout) => {
                // parking_lot's wait_for returns true if notified, false if timed out
                let was_notified = !cvar.wait_for(&mut ready, std::time::Duration::from_millis(timeout)).timed_out();
                *ready = false;
                was_notified
            }
            None => {
                cvar.wait(&mut ready);
                *ready = false;
                true
            }
        }
    }
    
    /// Read records from the buffer, advancing the read index
    /// 
    /// # Returns
    /// 
    /// A vector of records read from the buffer
    pub fn read(&self) -> Vec<Record> {
        // Acquire the consumer lock to ensure single-consumer semantics
        let _guard = self.consumer_lock.write();
        
        let mut records = Vec::new();
        
        // Get the current read and commit indices
        let read = self.read_idx.load(Ordering::Relaxed);
        let commit = self.commit_idx.load(Ordering::Acquire);
        
        // No data to read
        if read == commit {
            return records;
        }
        
        let mut current_read = read;
        
        // Read committed records
        while current_read != commit {
            // Read the record header
            let header = unsafe {
                *(self.buffer.as_ptr().add(current_read) as *const RecordHeader)
            };
            
            // Read the record data
            let data_size = header.size as usize;
            let mut data = vec![0u8; data_size];
            
            unsafe {
                let data_ptr = self.buffer.as_ptr().add(current_read + RecordHeader::SIZE);
                std::ptr::copy_nonoverlapping(data_ptr, data.as_mut_ptr(), data_size);
            }
            
            // Verify CRC
            if header.verify_crc(&data) {
                records.push(Record { header, data });
            } else {
                // CRC mismatch, record might be corrupted
                // In production, we might want to log this error
            }
            
            // Advance read pointer
            current_read = self.read_idx.wrap(current_read + RecordHeader::SIZE + data_size);
        }
        
        // Update the read index
        self.read_idx.store(current_read, Ordering::Release);
        
        // Process any overflow queue entries now that we have space
        {
            let mut queue = self.overflow_queue.write();
            while let Some(record) = queue.pop_front() {
                if let Some(mut reservation) = self.reserve(record.data.len(), false) {
                    reservation.write(record.header, &record.data);
                    reservation.commit();
                } else {
                    // Still no space, put it back and stop
                    queue.push_front(record);
                    break;
                }
            }
        }
        
        records
    }
    
    /// Get the current buffer usage as a percentage
    pub fn usage_percent(&self) -> f32 {
        let write = self.write_idx.load(Ordering::Relaxed);
        let read = self.read_idx.load(Ordering::Relaxed);
        
        let used = if write >= read {
            write - read
        } else {
            self.capacity - read + write
        };
        
        (used as f32 / self.capacity as f32) * 100.0
    }
    
    /// Get the current buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

// Implement Clone manually to avoid cloning the buffer
impl Clone for VolatileRingBuffer {
    fn clone(&self) -> Self {
        Self {
            buffer: vec![0u8; self.capacity].into_boxed_slice(),
            write_idx: AtomicIndex::new(self.capacity),
            read_idx: AtomicIndex::new(self.capacity),
            commit_idx: AtomicIndex::new(self.capacity),
            capacity: self.capacity,
            consumer_signal: self.consumer_signal.clone(),
            consumer_lock: RwLock::new(()),
            overflow_queue: RwLock::new(VecDeque::new()),
            max_overflow: self.max_overflow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;
    
    #[test]
    fn test_basic_write_read() {
        let buffer = VolatileRingBuffer::new(1024);
        
        let data = b"test message".to_vec();
        let record = Record::new(data, 1);
        
        assert!(buffer.write(record));
        
        let records = buffer.read();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].data, b"test message");
        assert_eq!(records[0].header.tag, 1);
    }
    
    #[test]
    fn test_multi_thread_write() {
        const NUM_THREADS: usize = 4;
        const MSGS_PER_THREAD: usize = 100;
        
        let buffer = Arc::new(VolatileRingBuffer::new(4096));
        let barrier = Arc::new(Barrier::new(NUM_THREADS + 1)); // +1 for the main thread
        
        let mut handles = vec![];
        
        for t in 0..NUM_THREADS {
            let buffer_clone = buffer.clone();
            let barrier_clone = barrier.clone();
            
            let handle = thread::spawn(move || {
                barrier_clone.wait(); // Synchronize start
                
                for i in 0..MSGS_PER_THREAD {
                    let message = format!("Thread {} message {}", t, i);
                    let data = message.as_bytes().to_vec();
                    let record = Record::new(data, t as u32);
                    
                    buffer_clone.write(record);
                }
            });
            
            handles.push(handle);
        }
        
        barrier.wait(); // Start all threads
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Read all messages
        let records = buffer.read();
        assert_eq!(records.len(), NUM_THREADS * MSGS_PER_THREAD);
        
        // Verify all expected messages were received
        let mut counts = vec![0; NUM_THREADS];
        
        for record in records {
            let thread_id = record.header.tag as usize;
            counts[thread_id] += 1;
        }
        
        for count in counts {
            assert_eq!(count, MSGS_PER_THREAD);
        }
    }
    
    #[test]
    fn test_reserve_write_commit() {
        let buffer = VolatileRingBuffer::new(1024);
        
        // Reserve space
        let mut reservation = buffer.reserve(10, true).unwrap();
        
        // Create header and data
        let data = b"test data".to_vec();
        let mut header = RecordHeader::new(data.len() as u32, 123);
        header.update_crc(&data);
        
        // Write and commit
        reservation.write(header, &data);
        reservation.commit();
        
        // Read back
        let records = buffer.read();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].data, b"test data");
        assert_eq!(records[0].header.tag, 123);
    }
    
    #[test]
    fn test_buffer_overflow() {
        // Small buffer that can hold only a few records
        let buffer = VolatileRingBuffer::new(64);
        
        // Fill the buffer
        for i in 0..20 {
            let data = format!("message {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(record);
        }
        
        // Read everything back
        let records = buffer.read();
        
        // Some records might be in the overflow queue and some in the buffer
        // The exact number depends on sizes, but we should have at least a few
        assert!(records.len() > 0);
        
        // After reading, the overflow queue should be processed
        // Writing more records should succeed now
        for i in 0..5 {
            let data = format!("new message {}", i).into_bytes();
            let record = Record::new(data, 100 + i);
            assert!(buffer.write(record));
        }
        
        let records = buffer.read();
        assert_eq!(records.len(), 5);
    }
}