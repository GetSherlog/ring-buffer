//! Volatile MPSC ring buffer implementation for in-memory logging
//! 
//! This module implements a lock-free, multi-producer single-consumer (MPSC) ring buffer
//! designed for high-throughput in-memory logging. Key features include:
//! 
//! - Lock-free writes using atomic operations for multiple concurrent writers
//! - Reserve-Write-Commit pattern to allow efficient, zero-copy writing
//! - Slot-based architecture for precise memory management
//! - Sequence-based indices for better wrapping behavior
//! - Overflow handling with bounded retries
//! - Condition variable for consumer notification
//! - Cache-line padding to prevent false sharing
//! 
//! The implementation uses a sequence-based numbering approach with slots that
//! transition through different states from empty to ready to consumed. This provides
//! better memory ordering guarantees and clearer ownership semantics.

use crate::memory::{AtomicIndex, Record, RecordHeader, Slot, SlotState};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use parking_lot::{Condvar, RwLock};
use std::collections::VecDeque;
use std::time::Duration;
use crossbeam_utils::CachePadded;

/// Lock-free reservation in the ring buffer
pub struct Reservation {
    /// Slot index for this reservation
    slot_idx: usize,
    /// Sequence number for this reservation
    seq: usize,
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
                
        // Get a reference to the slot's data
        let data_ptr = self.buffer.get_slot_data_ptr(self.slot_idx);
                
        // Write the header
        unsafe {
            let header_ptr = data_ptr as *mut RecordHeader;
            *header_ptr = header;
        }
        
        // Write the data
        unsafe {
            let data_ptr = (data_ptr as *mut u8).add(RecordHeader::SIZE);
            std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());
        }
    }
    
    /// Commit this reservation, making it visible to consumers
    pub fn commit(mut self) {
        if !self.committed {
            // Mark the slot as ready
            let slot = &self.buffer.slots[self.slot_idx];
            
            // Transition from Reserved to Ready (this is atomic)
            if slot.try_transition(SlotState::Reserved, SlotState::Ready, Ordering::Release) {
                self.committed = true;
                
                // Update sequence to help downstream consumers
                slot.set_seq(self.seq, Ordering::Release);
                
                // Notify any waiting consumers
                self.buffer.notify_consumer();
            }
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // If dropping without committing, release the space
        if !self.committed {
            let slot = &self.buffer.slots[self.slot_idx];
            
            // Try to transition back to Empty
            slot.try_transition(SlotState::Reserved, SlotState::Empty, Ordering::Release);
        }
    }
}

/// Multi-producer, single-consumer ring buffer for volatile memory
pub struct VolatileRingBuffer {
    /// Slots for data storage
    slots: Box<[Slot]>,
    /// Actual data storage
    data: Box<[u8]>,
    /// Production sequence (indicates next slot to produce)
    prod_seq: CachePadded<AtomicIndex>,
    /// Consumption sequence (indicates next slot to consume)
    cons_seq: CachePadded<AtomicIndex>,
    /// Buffer capacity (power of 2)
    capacity: usize,
    /// Size of each slot
    slot_size: usize,
    /// Condition variable for notifying consumer about new data
    consumer_signal: Arc<(parking_lot::Mutex<bool>, parking_lot::Condvar)>,
    /// Lock for single-consumer reads
    consumer_lock: RwLock<()>,
    /// Queue for writing log entries when buffer is full
    overflow_queue: RwLock<VecDeque<Record>>,
    /// Maximum overflow queue size
    max_overflow: usize,
    /// Maximum number of spins before fallback
    max_spins: usize,
}

impl VolatileRingBuffer {
    /// Create a new volatile ring buffer with the given capacity
    /// 
    /// Capacity is rounded up to the next power of 2
    pub fn new(capacity: usize) -> Self {
        // Round up to the next power of 2
        let num_slots = capacity.next_power_of_two();
        
        // Default each slot to have 256 bytes
        let slot_size = 256;
        let total_data_size = num_slots * slot_size;
        
        // Allocate slots and data storage
        let mut slots = Vec::with_capacity(num_slots);
        for _ in 0..num_slots {
            slots.push(Slot::new());
        }
        
        // Allocate data buffer
        let data = vec![0u8; total_data_size].into_boxed_slice();
        
        Self {
            slots: slots.into_boxed_slice(),
            data,
            prod_seq: CachePadded::new(AtomicIndex::new(num_slots)),
            cons_seq: CachePadded::new(AtomicIndex::new(num_slots)),
            capacity: num_slots,
            slot_size,
            consumer_signal: Arc::new((parking_lot::Mutex::new(false), parking_lot::Condvar::new())),
            consumer_lock: RwLock::new(()),
            overflow_queue: RwLock::new(VecDeque::new()),
            max_overflow: 1000, // Default max overflow records
            max_spins: 100,     // Default max spins
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
        
        // Make sure the record fits in a slot
        assert!(total_size <= self.slot_size, 
                "Record too large for buffer slot - must be <= slot size");
        
        // Try to reserve space with bounded spins
        let mut spins = 0;
        while spins < self.max_spins {
            // Get current production sequence
            let prod_seq = self.prod_seq.load(Ordering::Relaxed);
            
            // Calculate the physical slot index
            let slot_idx = self.prod_seq.physical_index(prod_seq);
            let slot = &self.slots[slot_idx];
            
            // Check if this slot is available (Empty)
            match slot.state(Ordering::Acquire) {
                SlotState::Empty => {
                    // Try to transition to Reserved
                    if slot.try_transition(SlotState::Empty, SlotState::Reserved, Ordering::Acquire) {
                        // Update production sequence
                        let next_seq = prod_seq.wrapping_add(1);
                        self.prod_seq.store(next_seq, Ordering::Release);
                        
                        // Set the sequence number in the slot
                        slot.set_seq(prod_seq, Ordering::Release);
                        
                        // Return the reservation
                        return Some(Reservation {
                            slot_idx,
                            seq: prod_seq,
                            size: total_size,
                            buffer: Arc::new(self.clone()),
                            committed: false,
                        });
                    }
                }
                _ => {
                    // Slot is not empty, try incremental backoff
                    if spins < 10 {
                        std::hint::spin_loop();
                    } else if spins < 20 {
                        std::thread::yield_now();
                    } else {
                        std::thread::sleep(Duration::from_micros(1));
                    }
                }
            }
            
            spins += 1;
            
            // If not blocking and we've already retried, give up
            if !block && spins > 10 {
                return None;
            }
        }
        
        // All retries failed - either return None or wait and try again
        if block {
            // Sleep a bit then recurse
            std::thread::sleep(Duration::from_millis(1));
            self.reserve(size, block)
        } else {
            None
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
    
    /// Get pointer to a slot's data
    fn get_slot_data_ptr(&self, slot_idx: usize) -> *mut u8 {
        let offset = slot_idx * self.slot_size;
        unsafe { self.data.as_ptr().add(offset) as *mut u8 }
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
                let was_notified = !cvar.wait_for(&mut ready, Duration::from_millis(timeout)).timed_out();
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
        
        // Get current consumer sequence
        let mut cons_seq = self.cons_seq.load(Ordering::Relaxed);
        
        // Process ready slots
        let mut processed = 0;
        
        // To prevent infinite loops, limit the number of slots we'll check
        let max_slots_to_check = self.capacity;
        
        for _ in 0..max_slots_to_check {
            // Get the physical slot index
            let slot_idx = self.cons_seq.physical_index(cons_seq);
            let slot = &self.slots[slot_idx];
            
            // Check if the slot is ready to read
            if slot.state(Ordering::Acquire) == SlotState::Ready && slot.seq(Ordering::Acquire) == cons_seq {
                // Read the record header
                let data_ptr = self.get_slot_data_ptr(slot_idx);
                let header = unsafe {
                    *(data_ptr as *const RecordHeader)
                };
                
                // Read the record data
                let data_size = header.size as usize;
                let mut data = vec![0u8; data_size];
                
                unsafe {
                    let src_ptr = data_ptr.add(RecordHeader::SIZE);
                    std::ptr::copy_nonoverlapping(src_ptr, data.as_mut_ptr(), data_size);
                }
                
                // Verify CRC
                if header.verify_crc(&data) {
                    records.push(Record { header, data });
                }
                
                // Mark slot as consumed
                slot.set_state(SlotState::Consumed, Ordering::Release);
                
                // Advance consumer sequence
                cons_seq = cons_seq.wrapping_add(1);
                processed += 1;
                
                // Recycle the slot by marking it empty
                // Note: For MPSC, we can safely recycle right away
                slot.set_state(SlotState::Empty, Ordering::Release);
                
            } else {
                // No more ready records
                break;
            }
        }
        
        // Update consumer sequence if we processed any records
        if processed > 0 {
            self.cons_seq.store(cons_seq, Ordering::Release);
        }
        
        // Process any overflow queue entries now that we have space
        if processed > 0 {
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
        let prod_seq = self.prod_seq.load(Ordering::Relaxed);
        let cons_seq = self.cons_seq.load(Ordering::Relaxed);
        
        // Calculate how many slots are in use (this works even with wrapping)
        let used = (prod_seq.wrapping_sub(cons_seq)) as f32;
        let capacity = self.capacity as f32;
        
        (used / capacity) * 100.0
    }
    
    /// Get the current buffer capacity in slots
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    
    /// Get the size of each slot in bytes
    pub fn slot_size(&self) -> usize {
        self.slot_size
    }
}

impl Clone for VolatileRingBuffer {
    fn clone(&self) -> Self {
        // Create a new empty buffer with the same capacity and slot size
        let slot_size = self.slot_size;
        let num_slots = self.capacity;
        let total_data_size = num_slots * slot_size;
        
        // Allocate slots and data storage
        let mut slots = Vec::with_capacity(num_slots);
        for _ in 0..num_slots {
            slots.push(Slot::new());
        }
        
        // Allocate data buffer
        let data = vec![0u8; total_data_size].into_boxed_slice();
        
        Self {
            slots: slots.into_boxed_slice(),
            data,
            prod_seq: CachePadded::new(AtomicIndex::new(num_slots)),
            cons_seq: CachePadded::new(AtomicIndex::new(num_slots)),
            capacity: self.capacity,
            slot_size: self.slot_size,
            consumer_signal: self.consumer_signal.clone(),
            consumer_lock: RwLock::new(()),
            overflow_queue: RwLock::new(VecDeque::new()),
            max_overflow: self.max_overflow,
            max_spins: self.max_spins,
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
        let buffer = VolatileRingBuffer::new(4);
        
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
    
    #[test]
    fn test_sequence_wrapping() {
        // Test that sequence numbers wrap around correctly
        let buffer = VolatileRingBuffer::new(4); // Small buffer to force wrapping
        
        // Write enough records to wrap around the sequence space multiple times
        for i in 0..20 {
            let data = format!("wrap {}", i).into_bytes();
            let record = Record::new(data, i);
            buffer.write(record);
            
            // Read after each write to keep the buffer from filling up
            buffer.read();
        }
        
        // Verify the buffer is still working
        let data = b"final test".to_vec();
        let record = Record::new(data, 999);
        assert!(buffer.write(record));
        
        let records = buffer.read();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].data, b"final test");
        assert_eq!(records[0].header.tag, 999);
    }
}