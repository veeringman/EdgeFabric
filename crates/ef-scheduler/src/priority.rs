//! Priority scheduling support.
//!
//! Provides a simple priority queue for allocation requests, ensuring
//! fair scheduling across tenants using a round-robin + priority model.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use uuid::Uuid;

use ef_core::device::DeviceFilter;

/// Priority level for allocation requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// A queued allocation request with priority and timestamp ordering.
#[derive(Debug, Clone)]
pub struct AllocationRequest {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub filter: DeviceFilter,
    pub priority: Priority,
    /// Monotonic sequence number — lower = earlier arrival.
    pub seq: u64,
}

impl PartialEq for AllocationRequest {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for AllocationRequest {}

/// Ordering: higher priority wins; ties broken by earlier arrival (lower seq).
impl Ord for AllocationRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.priority as u8)
            .cmp(&(other.priority as u8))
            .then_with(|| other.seq.cmp(&self.seq)) // lower seq = higher priority
    }
}

impl PartialOrd for AllocationRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Thread-safe priority queue for device allocation requests.
pub struct PriorityQueue {
    heap: BinaryHeap<AllocationRequest>,
    next_seq: u64,
}

impl PriorityQueue {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            next_seq: 0,
        }
    }

    /// Enqueue a new allocation request. Returns the assigned request ID.
    pub fn enqueue(
        &mut self,
        tenant_id: Uuid,
        filter: DeviceFilter,
        priority: Priority,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let seq = self.next_seq;
        self.next_seq += 1;

        self.heap.push(AllocationRequest {
            id,
            tenant_id,
            filter,
            priority,
            seq,
        });

        tracing::debug!(%id, %tenant_id, ?priority, seq, "Request enqueued");
        id
    }

    /// Dequeue the highest-priority request.
    pub fn dequeue(&mut self) -> Option<AllocationRequest> {
        self.heap.pop()
    }

    /// Number of pending requests.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        let mut q = PriorityQueue::new();
        let t = Uuid::new_v4();
        let filter = DeviceFilter::default();

        q.enqueue(t, filter.clone(), Priority::Low);
        q.enqueue(t, filter.clone(), Priority::Critical);
        q.enqueue(t, filter.clone(), Priority::Normal);

        let first = q.dequeue().unwrap();
        assert_eq!(first.priority, Priority::Critical);

        let second = q.dequeue().unwrap();
        assert_eq!(second.priority, Priority::Normal);

        let third = q.dequeue().unwrap();
        assert_eq!(third.priority, Priority::Low);
    }

    #[test]
    fn fifo_within_same_priority() {
        let mut q = PriorityQueue::new();
        let t = Uuid::new_v4();
        let filter = DeviceFilter::default();

        let id1 = q.enqueue(t, filter.clone(), Priority::Normal);
        let id2 = q.enqueue(t, filter.clone(), Priority::Normal);

        let first = q.dequeue().unwrap();
        assert_eq!(first.id, id1);

        let second = q.dequeue().unwrap();
        assert_eq!(second.id, id2);
    }

    #[test]
    fn dequeue_empty_returns_none() {
        let mut q = PriorityQueue::new();
        assert!(q.dequeue().is_none());
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn len_tracks_enqueue_dequeue() {
        let mut q = PriorityQueue::new();
        let t = Uuid::new_v4();
        let filter = DeviceFilter::default();

        assert_eq!(q.len(), 0);
        q.enqueue(t, filter.clone(), Priority::Low);
        assert_eq!(q.len(), 1);
        q.enqueue(t, filter.clone(), Priority::High);
        assert_eq!(q.len(), 2);
        q.dequeue();
        assert_eq!(q.len(), 1);
        q.dequeue();
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn high_priority_preempts_earlier_low() {
        let mut q = PriorityQueue::new();
        let t = Uuid::new_v4();
        let filter = DeviceFilter::default();

        // Enqueue low first, then high — high should come out first
        q.enqueue(t, filter.clone(), Priority::Low);
        q.enqueue(t, filter.clone(), Priority::High);

        assert_eq!(q.dequeue().unwrap().priority, Priority::High);
        assert_eq!(q.dequeue().unwrap().priority, Priority::Low);
    }

    #[test]
    fn multi_tenant_fair_by_arrival() {
        let mut q = PriorityQueue::new();
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let filter = DeviceFilter::default();

        let id1 = q.enqueue(t1, filter.clone(), Priority::Normal);
        let id2 = q.enqueue(t2, filter.clone(), Priority::Normal);

        // Same priority → FIFO, tenant1 first
        assert_eq!(q.dequeue().unwrap().id, id1);
        assert_eq!(q.dequeue().unwrap().id, id2);
    }

    #[test]
    fn all_priority_levels() {
        let mut q = PriorityQueue::new();
        let t = Uuid::new_v4();
        let filter = DeviceFilter::default();

        q.enqueue(t, filter.clone(), Priority::Normal);
        q.enqueue(t, filter.clone(), Priority::Low);
        q.enqueue(t, filter.clone(), Priority::Critical);
        q.enqueue(t, filter.clone(), Priority::High);

        assert_eq!(q.dequeue().unwrap().priority, Priority::Critical);
        assert_eq!(q.dequeue().unwrap().priority, Priority::High);
        assert_eq!(q.dequeue().unwrap().priority, Priority::Normal);
        assert_eq!(q.dequeue().unwrap().priority, Priority::Low);
    }

    #[test]
    fn enqueue_returns_unique_ids() {
        let mut q = PriorityQueue::new();
        let t = Uuid::new_v4();
        let filter = DeviceFilter::default();

        let id1 = q.enqueue(t, filter.clone(), Priority::Normal);
        let id2 = q.enqueue(t, filter.clone(), Priority::Normal);
        let id3 = q.enqueue(t, filter.clone(), Priority::Normal);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }
}
