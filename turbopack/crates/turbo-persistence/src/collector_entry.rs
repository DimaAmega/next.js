use std::cmp::Ordering;

use crate::{
    FamilyKind,
    constants::MAX_INLINE_VALUE_SIZE,
    key::StoreKey,
    static_sorted_file_builder::{Entry, EntryValue},
};

/// Trait for entries that can be sorted and deduplicated for SST storage.
///
/// Both write-path (`CollectorEntry`) and merge-path (`LookupEntry`) implement this
/// so the sort+dedup algorithms can be shared.
pub trait SortableEntry: Ord {
    /// Compare only the key portion of two entries.
    fn cmp_key(&self, other: &Self) -> Ordering;
    /// Returns true if this entry is a deletion tombstone.
    fn is_deleted(&self) -> bool;
    /// Returns (key_size, value_size) for this entry.
    fn entry_size(&self) -> (usize, usize);
}

/// Cumulative sizes of entries removed by `sort_and_dedup`.
pub struct DroppedSize {
    pub key_size: usize,
    pub value_size: usize,
}

/// Sorts and deduplicates entries according to the family kind, suitable for SST storage.
///
/// - **SingleValue**: keeps only the last entry per key (latest write wins).
/// - **MultiValue**: deletes discard all prior entries for that key, the tombstone itself is kept.
///   Duplicate (key, value) pairs are removed. Final order is (key, value).
///
/// If `ALREADY_SORTED_BY_KEY` is true, skips the initial key sort (e.g. entries from MergeIter
/// are already in key order).
pub fn sort_and_dedup<const ALREADY_SORTED_BY_KEY: bool, E: SortableEntry>(
    entries: &mut Vec<E>,
    kind: FamilyKind,
) -> DroppedSize {
    let mut dropped = DroppedSize {
        key_size: 0,
        value_size: 0,
    };
    match kind {
        FamilyKind::SingleValue => {
            if !ALREADY_SORTED_BY_KEY {
                entries.sort_by(|a, b| a.cmp_key(b));
            }
            // Keep only the last entry per key (latest write wins).
            // `dedup_by` keeps `b` and drops `a` when returning true.
            // We swap so `b` retains the later (rightmost) value.
            entries.dedup_by(|a, b| {
                if a.cmp_key(b) == Ordering::Equal {
                    let (ks, vs) = a.entry_size();
                    dropped.key_size += ks;
                    dropped.value_size += vs;
                    std::mem::swap(a, b);
                    true
                } else {
                    false
                }
            });
        }
        FamilyKind::MultiValue => {
            if !ALREADY_SORTED_BY_KEY {
                entries.sort_by(|a, b| a.cmp_key(b));
            }
            // Prune entries before the last tombstone within each key group.
            prune_deleted_groups(entries, &mut dropped);
            // Re-sort by (key, value) for SST storage and dedup identical pairs.
            entries.sort();
            dedupe_key_value_pairs(entries, &mut dropped);
        }
    }
    dropped
}

/// Like `Vec::dedup` but accumulates dropped entry sizes.
fn dedupe_key_value_pairs<E: SortableEntry>(entries: &mut Vec<E>, dropped: &mut DroppedSize) {
    entries.dedup_by(|a, b| {
        if a == b {
            let (ks, vs) = a.entry_size();
            dropped.key_size += ks;
            dropped.value_size += vs;
            true
        } else {
            false
        }
    });
}

/// For MultiValue families: within each key group (sorted by key, preserving
/// insertion order via stable sort), if a Deleted tombstone is present, discard all
/// entries before the last tombstone. The tombstone itself plus any entries after it survive.
fn prune_deleted_groups<E: SortableEntry>(entries: &mut Vec<E>, dropped: &mut DroppedSize) {
    let mut i = entries.len();
    while i > 0 {
        // Find the last Deleted entry in entries[..i]
        let Some(del_pos) = entries[..i].iter().rposition(|e| e.is_deleted()) else {
            break;
        };

        // Scan backwards to find the start of this key group
        let group_start = {
            let mut start = del_pos;
            while start > 0 && entries[start - 1].cmp_key(&entries[del_pos]) == Ordering::Equal {
                start -= 1;
            }
            start
        };

        // Accumulate sizes of entries being removed
        for e in &entries[group_start..del_pos] {
            let (ks, vs) = e.entry_size();
            dropped.key_size += ks;
            dropped.value_size += vs;
        }

        // Remove entries before the tombstone (group_start..del_pos)
        entries.drain(group_start..del_pos);
        i = group_start;
    }
}

/// Classification of a value for sorting purposes.
///
/// Both `CollectorEntryValue` (write path) and `LazyLookupValue` (read/merge path) use
/// this same classification to ensure a consistent total order:
/// `Deleted < ByteContent(ordered by bytes) < Blob(ordered by sequence number)`.
/// The order of bytes and blobs is irrelevant just needs to be stable, but Deleteds need to stay
/// first.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
pub enum ValueCategory<'a> {
    // It is important this this comes first so deleted tokens don't move around
    Deleted,
    ByteContent(&'a [u8]),
    Blob(u32),
}

pub struct CollectorEntry<K: StoreKey> {
    pub key: EntryKey<K>,
    pub value: CollectorEntryValue,
}

impl<K: StoreKey> PartialEq for CollectorEntry<K> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}

impl<K: StoreKey> Eq for CollectorEntry<K> {}

impl<K: StoreKey> PartialOrd for CollectorEntry<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: StoreKey> Ord for CollectorEntry<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key
            .cmp(&other.key)
            .then_with(|| self.value.cmp(&other.value))
    }
}

impl<K: StoreKey> SortableEntry for CollectorEntry<K> {
    fn cmp_key(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }

    fn is_deleted(&self) -> bool {
        matches!(self.value, CollectorEntryValue::Deleted)
    }

    fn entry_size(&self) -> (usize, usize) {
        (self.key.len(), self.value.len())
    }
}

/// The size threshold for inline storage in CollectorEntryValue, this is the largest value that can
/// be stored inline without inflating the size of the enum
pub const TINY_VALUE_THRESHOLD: usize = 22;

pub enum CollectorEntryValue {
    /// Tiny value stored inline (≤22 bytes, no heap allocation)
    Tiny {
        value: [u8; TINY_VALUE_THRESHOLD],
        len: u8,
    },
    /// Small value that fits in shared value blocks (> 16 bytes, ≤ MAX_SMALL_VALUE_SIZE)
    Small {
        value: Box<[u8]>,
    },
    /// Medium value that gets its own value block (> MAX_SMALL_VALUE_SIZE)
    Medium {
        value: Box<[u8]>,
    },
    Large {
        blob: u32,
    },
    Deleted,
}

impl CollectorEntryValue {
    pub fn len(&self) -> usize {
        match self {
            CollectorEntryValue::Tiny { len, .. } => *len as usize,
            CollectorEntryValue::Small { value } => value.len(),
            CollectorEntryValue::Medium { value } => value.len(),
            CollectorEntryValue::Large { blob: _ } => 0,
            CollectorEntryValue::Deleted => 0,
        }
    }

    /// Returns true if this value gets its own dedicated value block.
    pub fn is_medium_value(&self) -> bool {
        matches!(self, CollectorEntryValue::Medium { .. })
    }

    /// Returns the value size if it will be packed into a small value block, or 0 otherwise.
    pub fn small_value_size(&self) -> usize {
        match self {
            CollectorEntryValue::Tiny { len, .. } if (*len as usize) > MAX_INLINE_VALUE_SIZE => {
                *len as usize
            }
            CollectorEntryValue::Small { value } => value.len(),
            _ => 0,
        }
    }
    /// Returns the byte slice for byte-content values, or None for Large/Deleted.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            CollectorEntryValue::Tiny { value, len } => Some(&value[..*len as usize]),
            CollectorEntryValue::Small { value } => Some(value),
            CollectorEntryValue::Medium { value } => Some(value),
            CollectorEntryValue::Large { .. } | CollectorEntryValue::Deleted => None,
        }
    }

    /// Classify this value for sort ordering purposes.
    fn category(&self) -> ValueCategory<'_> {
        match self {
            CollectorEntryValue::Deleted => ValueCategory::Deleted,
            CollectorEntryValue::Large { blob } => ValueCategory::Blob(*blob),
            _ => ValueCategory::ByteContent(self.as_bytes().unwrap()),
        }
    }
}

// Manual impls are needed because the same byte content can live in different
// variants (Tiny vs Small vs Medium) and must compare equal across them.
impl PartialEq for CollectorEntryValue {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for CollectorEntryValue {}

impl PartialOrd for CollectorEntryValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CollectorEntryValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.category().cmp(&other.category())
    }
}

pub struct EntryKey<K: StoreKey> {
    pub hash: u64,
    pub data: K,
}

impl<K: StoreKey> EntryKey<K> {
    pub fn len(&self) -> usize {
        std::mem::size_of::<u64>() + self.data.len()
    }
}

impl<K: StoreKey> PartialEq for EntryKey<K> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.data == other.data
    }
}

impl<K: StoreKey> Eq for EntryKey<K> {}

impl<K: StoreKey> PartialOrd for EntryKey<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: StoreKey> Ord for EntryKey<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hash
            .cmp(&other.hash)
            .then_with(|| self.data.cmp(&other.data))
    }
}

impl<K: StoreKey> Entry for CollectorEntry<K> {
    fn key_hash(&self) -> u64 {
        self.key.hash
    }

    fn key_len(&self) -> usize {
        self.key.data.len()
    }

    fn write_key_to(&self, buf: &mut Vec<u8>) {
        self.key.data.write_to(buf);
    }

    fn value(&self) -> EntryValue<'_> {
        match &self.value {
            CollectorEntryValue::Tiny { value, len } => {
                let slice = &value[..*len as usize];
                if slice.len() <= MAX_INLINE_VALUE_SIZE {
                    EntryValue::Inline { value: slice }
                } else {
                    EntryValue::Small { value: slice }
                }
            }
            CollectorEntryValue::Small { value } => {
                if value.len() <= MAX_INLINE_VALUE_SIZE {
                    EntryValue::Inline { value }
                } else {
                    EntryValue::Small { value }
                }
            }
            CollectorEntryValue::Medium { value } => EntryValue::Medium { value },
            CollectorEntryValue::Large { blob } => EntryValue::Large { blob: *blob },
            CollectorEntryValue::Deleted => EntryValue::Deleted,
        }
    }
}
