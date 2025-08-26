use crossbeam::queue::SegQueue;
use crossbeam::utils::Backoff;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::sync::Arc;
use dashmap::DashMap;

/// Lock-free duplicate checker using atomic operations
pub struct LockFreeDuplicateChecker {
    seen_passwords: Arc<DashMap<String, AtomicUsize>>,
    duplicate_count: Arc<AtomicUsize>,
    run_number: Arc<AtomicUsize>,
    enabled: Arc<AtomicBool>,
}

impl LockFreeDuplicateChecker {
    pub fn new() -> Self {
        Self {
            seen_passwords: Arc::new(DashMap::new()),
            duplicate_count: Arc::new(AtomicUsize::new(0)),
            run_number: Arc::new(AtomicUsize::new(0)),
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn is_duplicate(&self, password: &str) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }

        let current_run = self.run_number.load(Ordering::Acquire);
        
        // Use compare-and-swap for atomic duplicate detection
        match self.seen_passwords.entry(password.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(entry) => {
                let prev_run = entry.get().load(Ordering::Acquire);
                if prev_run == current_run {
                    // Already seen in this run
                    true
                } else {
                    // Update to current run and allow
                    entry.get().store(current_run, Ordering::Release);
                    false
                }
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                // First time seeing this password
                entry.insert(AtomicUsize::new(current_run));
                false
            }
        }
    }

    pub fn next_run(&self) {
        self.run_number.fetch_add(1, Ordering::AcqRel);
    }

    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    pub fn get_duplicate_count(&self) -> usize {
        self.duplicate_count.load(Ordering::Acquire)
    }
}

/// Lock-free work queue for distributing password generation tasks
pub struct WorkQueue<T> {
    queue: SegQueue<T>,
    active_workers: AtomicUsize,
    total_items: AtomicUsize,
    completed_items: AtomicUsize,
}

impl<T> WorkQueue<T> {
    pub fn new() -> Self {
        Self {
            queue: SegQueue::new(),
            active_workers: AtomicUsize::new(0),
            total_items: AtomicUsize::new(0),
            completed_items: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, item: T) {
        self.queue.push(item);
        self.total_items.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pop(&self) -> Option<T> {
        self.queue.pop()
    }

    pub fn register_worker(&self) {
        self.active_workers.fetch_add(1, Ordering::Relaxed);
    }

    pub fn unregister_worker(&self) {
        self.active_workers.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn mark_completed(&self) {
        self.completed_items.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.queue.is_empty() && self.active_workers.load(Ordering::Relaxed) == 0
    }

    pub fn progress(&self) -> (usize, usize) {
        (
            self.completed_items.load(Ordering::Relaxed),
            self.total_items.load(Ordering::Relaxed)
        )
    }
}

/// Lock-free result collector with batching
pub struct ResultCollector {
    results: SegQueue<String>,
    batch_size: usize,
    current_batch: Arc<std::sync::Mutex<Vec<String>>>, // Small critical section
    total_collected: AtomicUsize,
}

impl ResultCollector {
    pub fn new(batch_size: usize) -> Self {
        Self {
            results: SegQueue::new(),
            batch_size,
            current_batch: Arc::new(std::sync::Mutex::new(Vec::with_capacity(batch_size))),
            total_collected: AtomicUsize::new(0),
        }
    }

    pub fn add_result(&self, password: String) {
        // Try to add to current batch first
        if let Ok(mut batch) = self.current_batch.try_lock() {
            batch.push(password);
            if batch.len() >= self.batch_size {
                // Flush batch to main queue
                for item in batch.drain(..) {
                    self.results.push(item);
                }
                self.total_collected.fetch_add(self.batch_size, Ordering::Relaxed);
            }
        } else {
            // Fallback to direct queue insertion
            self.results.push(password);
            self.total_collected.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn collect_all(&self) -> Vec<String> {
        // Flush any remaining batch items
        if let Ok(mut batch) = self.current_batch.lock() {
            for item in batch.drain(..) {
                self.results.push(item);
            }
        }

        let mut all_results = Vec::new();
        while let Some(result) = self.results.pop() {
            all_results.push(result);
        }
        all_results
    }

    pub fn count(&self) -> usize {
        self.total_collected.load(Ordering::Relaxed)
    }
}

/// High-performance work-stealing scheduler
pub struct WorkStealingScheduler<T> {
    worker_queues: Vec<SegQueue<T>>,
    global_queue: SegQueue<T>,
    worker_count: usize,
    steal_attempts: AtomicUsize,
}

impl<T> WorkStealingScheduler<T> {
    pub fn new(worker_count: usize) -> Self {
        let worker_queues = (0..worker_count)
            .map(|_| SegQueue::new())
            .collect();

        Self {
            worker_queues,
            global_queue: SegQueue::new(),
            worker_count,
            steal_attempts: AtomicUsize::new(0),
        }
    }

    pub fn submit_work(&self, work: T, preferred_worker: Option<usize>) {
        if let Some(worker_id) = preferred_worker {
            if worker_id < self.worker_count {
                self.worker_queues[worker_id].push(work);
                return;
            }
        }
        self.global_queue.push(work);
    }

    pub fn get_work(&self, worker_id: usize) -> Option<T> {
        // Try local queue first
        if let Some(work) = self.worker_queues[worker_id].pop() {
            return Some(work);
        }

        // Try global queue
        if let Some(work) = self.global_queue.pop() {
            return Some(work);
        }

        // Try work stealing from other workers
        self.steal_attempts.fetch_add(1, Ordering::Relaxed);
        let backoff = Backoff::new();
        
        for _ in 0..self.worker_count {
            let victim = fastrand::usize(..self.worker_count);
            if victim != worker_id {
                if let Some(work) = self.worker_queues[victim].pop() {
                    return Some(work);
                }
            }
            backoff.snooze();
        }

        None
    }

    pub fn steal_statistics(&self) -> usize {
        self.steal_attempts.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_lockfree_duplicate_checker() {
        let checker = LockFreeDuplicateChecker::new();
        
        // First occurrence should not be duplicate
        assert!(!checker.is_duplicate("password123"));
        
        // Second occurrence should be duplicate
        assert!(checker.is_duplicate("password123"));
        
        // Different password should not be duplicate
        assert!(!checker.is_duplicate("different"));
    }

    #[test]
    fn test_work_queue_concurrent() {
        let queue = Arc::new(WorkQueue::new());
        let queue_clone = queue.clone();
        
        // Producer thread
        let producer = thread::spawn(move || {
            for i in 0..100 {
                queue_clone.push(i);
            }
        });
        
        // Consumer thread
        let consumer = thread::spawn(move || {
            let mut consumed = 0;
            while consumed < 100 {
                if let Some(_item) = queue.pop() {
                    consumed += 1;
                    queue.mark_completed();
                }
                thread::sleep(Duration::from_micros(1));
            }
            consumed
        });
        
        producer.join().unwrap();
        let consumed_count = consumer.join().unwrap();
        assert_eq!(consumed_count, 100);
    }

    #[test]
    fn test_work_stealing_scheduler() {
        let scheduler = WorkStealingScheduler::new(4);
        
        // Submit work items
        for i in 0..100 {
            scheduler.submit_work(i, Some(i % 4));
        }
        
        // Workers should be able to get work
        let mut total_work = 0;
        for worker_id in 0..4 {
            while let Some(_work) = scheduler.get_work(worker_id) {
                total_work += 1;
            }
        }
        
        assert_eq!(total_work, 100);
    }
}
