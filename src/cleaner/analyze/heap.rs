use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;
use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LargeFileEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

impl Ord for LargeFileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse order so BinaryHeap acts as a min-heap
        other.size_bytes.cmp(&self.size_bytes)
    }
}

impl PartialOrd for LargeFileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct LargeFileHeap {
    capacity: usize,
    heap: BinaryHeap<LargeFileEntry>,
}

impl LargeFileHeap {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            heap: BinaryHeap::with_capacity(capacity + 1),
        }
    }

    pub fn push(&mut self, path: PathBuf, size_bytes: u64) {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let entry = LargeFileEntry {
            name,
            path: path.to_string_lossy().to_string(),
            size_bytes,
        };

        self.heap.push(entry);
        if self.heap.len() > self.capacity {
            self.heap.pop();
        }
    }

    pub fn into_sorted_vec(self) -> Vec<LargeFileEntry> {
        let mut list: Vec<LargeFileEntry> = self.heap.into_vec();
        list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        list
    }
}
