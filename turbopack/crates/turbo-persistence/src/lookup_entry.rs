use std::cmp::Ordering;

use crate::{
    ArcSlice,
    collector_entry::{SortableEntry, ValueCategory},
    constants::{MAX_INLINE_VALUE_SIZE, MAX_SMALL_VALUE_SIZE},
    static_sorted_file_builder::{Entry, EntryValue},
};

/// A value from a SST file lookup.
#[derive(Eq, PartialEq)]
pub enum LookupValue {
    /// The value was deleted.
    Deleted,
    /// The value is stored in the SST file.
    ///
    /// The ArcSlice will be pointing either at a keyblock or a value block in the SST
    Slice { value: ArcSlice<u8> },
    /// The value is stored in a blob file.
    Blob { sequence_number: u32 },
}

/// A value from a SST file lookup.
pub enum LazyLookupValue<'l> {
    /// A LookupValue
    Eager(LookupValue),
    /// A medium sized value that is still compressed.
    Medium {
        uncompressed_size: u32,
        block: &'l [u8],
    },
}

impl LazyLookupValue<'_> {
    /// Returns the size of the value in the SST file.
    pub fn uncompressed_size_in_sst(&self) -> usize {
        match self {
            LazyLookupValue::Eager(LookupValue::Slice { value }) => value.len(),
            LazyLookupValue::Eager(LookupValue::Deleted) => 0,
            LazyLookupValue::Eager(LookupValue::Blob { .. }) => 0,
            LazyLookupValue::Medium {
                uncompressed_size, ..
            } => *uncompressed_size as usize,
        }
    }

    /// Returns true if this value gets its own dedicated value block.
    pub fn is_medium_value(&self) -> bool {
        match self {
            LazyLookupValue::Eager(LookupValue::Slice { value })
                if value.len() > MAX_SMALL_VALUE_SIZE =>
            {
                true
            }
            LazyLookupValue::Medium { .. } => true,
            _ => false,
        }
    }

    /// Returns the value size if it will be packed into a small value block, or 0 otherwise.
    pub fn small_value_size(&self) -> usize {
        match self {
            LazyLookupValue::Eager(LookupValue::Slice { value })
                if value.len() > MAX_INLINE_VALUE_SIZE && value.len() <= MAX_SMALL_VALUE_SIZE =>
            {
                value.len()
            }
            _ => 0,
        }
    }

    /// Classify this value for sort ordering purposes.
    ///
    /// For `Medium` (compressed) values, the compressed block bytes are used for comparison.
    /// This is a proxy that provides a consistent total order, though cross-type comparisons
    /// between `Slice` and `Medium` may not reflect semantic equality.
    fn category(&self) -> ValueCategory<'_> {
        match self {
            LazyLookupValue::Eager(LookupValue::Deleted) => ValueCategory::Deleted,
            LazyLookupValue::Eager(LookupValue::Slice { value }) => {
                ValueCategory::ByteContent(value.as_ref())
            }
            LazyLookupValue::Medium { block, .. } => ValueCategory::ByteContent(block),
            LazyLookupValue::Eager(LookupValue::Blob { sequence_number }) => {
                ValueCategory::Blob(*sequence_number)
            }
        }
    }
}

/// An entry from a SST file lookup.
pub struct LookupEntry<'l> {
    /// The hash of the key.
    pub hash: u64,
    /// The key.
    pub key: ArcSlice<u8>,
    /// The value.
    pub value: LazyLookupValue<'l>,
}

impl PartialEq for LookupEntry<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for LookupEntry<'_> {}

impl PartialOrd for LookupEntry<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LookupEntry<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_key(other)
            .then_with(|| self.value.category().cmp(&other.value.category()))
    }
}

impl SortableEntry for LookupEntry<'_> {
    fn cmp_key(&self, other: &Self) -> Ordering {
        self.hash
            .cmp(&other.hash)
            .then_with(|| (*self.key).cmp(&*other.key))
    }

    fn is_deleted(&self) -> bool {
        matches! {self.value, LazyLookupValue::Eager(LookupValue::Deleted)}
    }

    fn entry_size(&self) -> (usize, usize) {
        (self.key.len(), self.value.uncompressed_size_in_sst())
    }
}

impl Entry for LookupEntry<'_> {
    fn key_hash(&self) -> u64 {
        self.hash
    }

    fn key_len(&self) -> usize {
        self.key.len()
    }

    fn write_key_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.key);
    }

    fn value(&self) -> EntryValue<'_> {
        match &self.value {
            LazyLookupValue::Eager(LookupValue::Deleted) => EntryValue::Deleted,
            LazyLookupValue::Eager(LookupValue::Slice { value }) => {
                if value.len() <= MAX_INLINE_VALUE_SIZE {
                    EntryValue::Inline { value }
                } else if value.len() > MAX_SMALL_VALUE_SIZE {
                    EntryValue::Medium { value }
                } else {
                    EntryValue::Small { value }
                }
            }
            LazyLookupValue::Eager(LookupValue::Blob { sequence_number }) => EntryValue::Large {
                blob: *sequence_number,
            },
            LazyLookupValue::Medium {
                uncompressed_size,
                block,
            } => EntryValue::MediumCompressed {
                uncompressed_size: *uncompressed_size,
                block,
            },
        }
    }
}

// Re-export the generic sort_and_dedup so callers can use `lookup_entry::sort_and_dedup`.
pub use crate::collector_entry::sort_and_dedup;
