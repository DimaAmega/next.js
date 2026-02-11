use std::mem::take;

use crate::{
    FamilyKind, ValueBuffer,
    collector_entry::{
        CollectorEntry, CollectorEntryValue, EntryKey, TINY_VALUE_THRESHOLD, sort_and_dedup,
    },
    constants::{
        DATA_THRESHOLD_PER_INITIAL_FILE, MAX_ENTRIES_PER_INITIAL_FILE, MAX_SMALL_VALUE_SIZE,
    },
    key::{StoreKey, hash_key},
    value_block_count_tracker::ValueBlockCountTracker,
};

/// A collector accumulates entries that should be eventually written to a file. It keeps track of
/// count and size of the entries to decide when it's "full". Accessing the entries sorts them.
pub struct Collector<K: StoreKey, const SIZE_SHIFT: usize = 0> {
    total_key_size: usize,
    total_value_size: usize,
    value_block_tracker: ValueBlockCountTracker,
    entries: Vec<CollectorEntry<K>>,
}

impl<K: StoreKey, const SIZE_SHIFT: usize> Collector<K, SIZE_SHIFT> {
    /// Creates a new collector. Note that this allocates the full capacity for the entries.
    pub fn new() -> Self {
        Self {
            total_key_size: 0,
            total_value_size: 0,
            value_block_tracker: ValueBlockCountTracker::new(),
            entries: Vec::with_capacity(MAX_ENTRIES_PER_INITIAL_FILE >> SIZE_SHIFT),
        }
    }

    /// Returns true if the collector has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns true if the collector is full.
    pub fn is_full(&self) -> bool {
        self.entries.len() >= MAX_ENTRIES_PER_INITIAL_FILE >> SIZE_SHIFT
            || self.total_key_size + self.total_value_size
                > DATA_THRESHOLD_PER_INITIAL_FILE >> SIZE_SHIFT
            || self.value_block_tracker.is_full()
    }

    /// Adds a normal key-value pair to the collector.
    pub fn put(&mut self, key: K, value: ValueBuffer) {
        let key = EntryKey {
            hash: hash_key(&key),
            data: key,
        };
        let value = if value.len() > MAX_SMALL_VALUE_SIZE {
            CollectorEntryValue::Medium {
                value: value.into_boxed_slice(),
            }
        } else if value.len() <= TINY_VALUE_THRESHOLD {
            let slice: &[u8] = &value;
            let mut arr = [0u8; TINY_VALUE_THRESHOLD];
            arr[..slice.len()].copy_from_slice(slice);
            CollectorEntryValue::Tiny {
                value: arr,
                len: slice.len() as u8,
            }
        } else {
            CollectorEntryValue::Small {
                value: value.into_boxed_slice(),
            }
        };
        self.total_key_size += key.len();
        self.total_value_size += value.len();
        self.value_block_tracker
            .track(value.is_medium_value(), value.small_value_size());
        self.entries.push(CollectorEntry { key, value });
    }

    /// Adds a blob key-value pair to the collector.
    pub fn put_blob(&mut self, key: K, blob: u32) {
        let key = EntryKey {
            hash: hash_key(&key),
            data: key,
        };
        self.total_key_size += key.len();
        self.entries.push(CollectorEntry {
            key,
            value: CollectorEntryValue::Large { blob },
        });
    }

    /// Adds a tombstone pair to the collector.
    pub fn delete(&mut self, key: K) {
        let key = EntryKey {
            hash: hash_key(&key),
            data: key,
        };
        self.total_key_size += key.len();
        self.entries.push(CollectorEntry {
            key,
            value: CollectorEntryValue::Deleted,
        });
    }

    /// Adds an entry from another collector to this collector.
    pub fn add_entry(&mut self, entry: CollectorEntry<K>) {
        self.total_key_size += entry.key.len();
        self.total_value_size += entry.value.len();
        self.value_block_tracker.track(
            entry.value.is_medium_value(),
            entry.value.small_value_size(),
        );
        self.entries.push(entry);
    }

    /// Sorts and deduplicates entries according to the family kind, returning the entries
    /// in (key, value) order suitable for SST storage.
    ///
    /// For `SingleValue`: only the last entry per key is kept (latest write wins).
    /// For `MultiValue`: deletes discard all prior entries for that key within this batch,
    /// but the tombstone itself is kept to shadow older SSTs. Duplicate values are also removed.
    pub fn sorted(&mut self, kind: FamilyKind) -> (&[CollectorEntry<K>], usize) {
        let dropped = sort_and_dedup::<false, _>(&mut self.entries, kind);
        self.total_key_size -= dropped.key_size;
        self.total_value_size -= dropped.value_size;
        (&self.entries, self.total_key_size)
    }

    /// Clears the collector.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_key_size = 0;
        self.total_value_size = 0;
        self.value_block_tracker.reset();
    }

    /// Drains all entries from the collector in un-sorted order. This can be used to move the
    /// entries into another collector.
    pub fn drain(&mut self) -> impl Iterator<Item = CollectorEntry<K>> + '_ {
        self.total_key_size = 0;
        self.total_value_size = 0;
        self.value_block_tracker.reset();
        self.entries.drain(..)
    }

    /// Clears the collector and drops the capacity
    pub fn drop_contents(&mut self) {
        drop(take(&mut self.entries));
        self.total_key_size = 0;
        self.total_value_size = 0;
        self.value_block_tracker.reset();
    }
}
