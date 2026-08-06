// Index table implementation for O(1) random access

use crate::error::{BCSError, Result};
use crate::limits::{self, MAX_FIELD_NAME_LEN, MAX_INDEX_BUCKETS, MAX_INDEX_ENTRIES};
use std::io::{Read, Write};
use xxhash_rust::xxh64::xxh64;

/// Hash a string key using XXHash64 (seed 0)
pub fn hash_key(key: &str) -> u64 {
    xxh64(key.as_bytes(), 0)
}

/// Hash bytes using XXHash64 (seed 0)
pub fn hash_bytes(data: &[u8]) -> u64 {
    xxh64(data, 0)
}

// ============================================================================
// Hash Bucket Structure
// ============================================================================

/// A single entry in the hash table
#[derive(Debug, Clone)]
pub struct HashBucket {
    /// Hash of the key (8 bytes)
    pub hash: u64,

    /// Offset to the value in the data layer (8 bytes)
    pub offset: u64,

    /// Index of next bucket in collision chain, -1 if end (4 bytes)
    pub next: i32,

    /// Original field name (for reconstruction)
    pub field_name: Option<String>,
}

impl HashBucket {
    /// Size of a bucket in bytes (8 + 8 + 4 = 20 bytes + variable string length)
    pub const BASE_SIZE: usize = 20;

    /// Create an empty bucket
    pub fn empty() -> Self {
        Self {
            hash: 0,
            offset: 0,
            next: -1,
            field_name: None,
        }
    }

    /// Create a new bucket with hash and offset
    pub fn new(hash: u64, offset: u64) -> Self {
        Self {
            hash,
            offset,
            next: -1,
            field_name: None,
        }
    }

    /// Create a new bucket with hash, offset, and field name
    pub fn with_field_name(hash: u64, offset: u64, field_name: String) -> Self {
        Self {
            hash,
            offset,
            next: -1,
            field_name: Some(field_name),
        }
    }

    /// Check if bucket is empty
    pub fn is_empty(&self) -> bool {
        self.hash == 0 && self.offset == 0
    }

    /// Write bucket to a writer
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.hash.to_le_bytes())?;
        writer.write_all(&self.offset.to_le_bytes())?;
        writer.write_all(&self.next.to_le_bytes())?;

        // Write field name (length-prefixed string)
        if let Some(ref name) = self.field_name {
            let name_bytes = name.as_bytes();
            writer.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(name_bytes)?;
        } else {
            writer.write_all(&0u32.to_le_bytes())?; // Empty name
        }

        Ok(())
    }

    /// Read bucket from a reader
    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        let mut hash_buf = [0u8; 8];
        let mut offset_buf = [0u8; 8];
        let mut next_buf = [0u8; 4];
        let mut name_len_buf = [0u8; 4];

        reader.read_exact(&mut hash_buf)?;
        reader.read_exact(&mut offset_buf)?;
        reader.read_exact(&mut next_buf)?;
        reader.read_exact(&mut name_len_buf)?;

        let name_len = u32::from_le_bytes(name_len_buf) as usize;
        limits::ensure_count(name_len, MAX_FIELD_NAME_LEN, "Field name")?;
        let field_name = if name_len > 0 {
            let mut name_buf = limits::alloc_buf(name_len, MAX_FIELD_NAME_LEN, "Field name")?;
            reader.read_exact(&mut name_buf)?;
            Some(
                String::from_utf8(name_buf)
                    .map_err(|e| BCSError::Index(format!("Invalid UTF-8 in field name: {}", e)))?,
            )
        } else {
            None
        };

        Ok(Self {
            hash: u64::from_le_bytes(hash_buf),
            offset: u64::from_le_bytes(offset_buf),
            next: i32::from_le_bytes(next_buf),
            field_name,
        })
    }
}

// ============================================================================
// Index Table Structure
// ============================================================================

/// Index table for O(1) random access to configuration values
pub struct IndexTable {
    /// Hash buckets for storing hash-offset pairs
    pub buckets: Vec<HashBucket>,

    /// Number of entries in the table
    entry_count: u32,

    /// Load factor (entries / buckets)
    load_factor: f32,
}

impl IndexTable {
    /// Default load factor (0.75)
    pub const DEFAULT_LOAD_FACTOR: f32 = 0.75;

    /// Minimum number of buckets
    const MIN_BUCKETS: usize = 16;

    /// Create a new empty index table
    pub fn new() -> Self {
        Self {
            buckets: vec![HashBucket::empty(); Self::MIN_BUCKETS],
            entry_count: 0,
            load_factor: Self::DEFAULT_LOAD_FACTOR,
        }
    }

    /// Create a new index table with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let bucket_count = Self::calculate_bucket_count(capacity, Self::DEFAULT_LOAD_FACTOR);
        Self {
            buckets: vec![HashBucket::empty(); bucket_count],
            entry_count: 0,
            load_factor: Self::DEFAULT_LOAD_FACTOR,
        }
    }

    /// Create a new index table with specified capacity and load factor
    pub fn with_capacity_and_load_factor(capacity: usize, load_factor: f32) -> Self {
        let bucket_count = Self::calculate_bucket_count(capacity, load_factor);
        Self {
            buckets: vec![HashBucket::empty(); bucket_count],
            entry_count: 0,
            load_factor,
        }
    }

    /// Calculate the number of buckets needed for a given capacity
    fn calculate_bucket_count(capacity: usize, load_factor: f32) -> usize {
        let count = ((capacity as f32) / load_factor).ceil() as usize;
        count.max(Self::MIN_BUCKETS).next_power_of_two()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entry_count as usize
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Get the number of entries (alias for len)
    pub fn entry_count(&self) -> usize {
        self.entry_count as usize
    }

    /// Get the number of buckets
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Get the load factor
    pub fn load_factor(&self) -> f32 {
        self.load_factor
    }

    /// Calculate the collision rate (percentage of entries that had collisions)
    pub fn collision_rate(&self) -> f32 {
        if self.entry_count == 0 {
            return 0.0;
        }

        let mut collisions = 0;
        for bucket in &self.buckets {
            if !bucket.is_empty() && bucket.next != -1 {
                collisions += 1;
            }
        }

        collisions as f32 / self.entry_count as f32
    }

    /// Calculate the bucket index for a hash using modulo
    fn bucket_index(&self, hash: u64) -> usize {
        (hash as usize) % self.buckets.len()
    }

    /// Insert a hash-offset pair with field name into the table
    /// Uses linear probing for collision resolution
    pub fn insert_with_name(&mut self, hash: u64, offset: u64, field_name: String) -> Result<()> {
        // Check if we need to resize
        if (self.entry_count + 1) as f32 > (self.buckets.len() as f32 * self.load_factor) {
            self.resize()?;
        }

        let index = self.bucket_index(hash);

        // Check if the primary slot is empty
        if self.buckets[index].is_empty() {
            self.buckets[index] = HashBucket::with_field_name(hash, offset, field_name);
            self.entry_count += 1;
            return Ok(());
        }

        // Primary slot is occupied - need to handle collision
        // Check if this is the same hash (collision chain)
        if self.buckets[index].hash == hash {
            // Follow the collision chain to the end
            let mut current_idx = index;
            while self.buckets[current_idx].next != -1 {
                current_idx = self.buckets[current_idx].next as usize;
            }

            // Find next empty slot using linear probing
            let mut next_idx = (current_idx + 1) % self.buckets.len();
            let mut probe_count = 0;
            while !self.buckets[next_idx].is_empty() && probe_count < self.buckets.len() {
                next_idx = (next_idx + 1) % self.buckets.len();
                probe_count += 1;
            }

            if probe_count >= self.buckets.len() {
                return Err(BCSError::Index("Hash table full".to_string()));
            }

            // Link the chain
            self.buckets[current_idx].next = next_idx as i32;
            self.buckets[next_idx] = HashBucket::with_field_name(hash, offset, field_name);
            self.entry_count += 1;
            return Ok(());
        }

        // Different hash - use linear probing to find empty slot
        let mut probe_idx = (index + 1) % self.buckets.len();
        let mut probe_count = 0;

        while !self.buckets[probe_idx].is_empty() && probe_count < self.buckets.len() {
            probe_idx = (probe_idx + 1) % self.buckets.len();
            probe_count += 1;
        }

        if probe_count >= self.buckets.len() {
            return Err(BCSError::Index("Hash table full".to_string()));
        }

        self.buckets[probe_idx] = HashBucket::with_field_name(hash, offset, field_name);
        self.entry_count += 1;
        Ok(())
    }

    /// Insert a hash-offset pair into the table
    /// Uses linear probing for collision resolution
    pub fn insert(&mut self, hash: u64, offset: u64) -> Result<()> {
        // Check if we need to resize
        if (self.entry_count + 1) as f32 > (self.buckets.len() as f32 * self.load_factor) {
            self.resize()?;
        }

        let index = self.bucket_index(hash);

        // Check if the primary slot is empty
        if self.buckets[index].is_empty() {
            self.buckets[index] = HashBucket::new(hash, offset);
            self.entry_count += 1;
            return Ok(());
        }

        // Primary slot is occupied - need to handle collision
        // Check if this is the same hash (collision chain)
        if self.buckets[index].hash == hash {
            // Follow the collision chain to the end
            let mut current_idx = index;
            while self.buckets[current_idx].next != -1 {
                current_idx = self.buckets[current_idx].next as usize;
            }

            // Find next empty slot using linear probing
            let mut next_idx = (current_idx + 1) % self.buckets.len();
            let mut probe_count = 0;
            while !self.buckets[next_idx].is_empty() && probe_count < self.buckets.len() {
                next_idx = (next_idx + 1) % self.buckets.len();
                probe_count += 1;
            }

            if probe_count >= self.buckets.len() {
                return Err(BCSError::Index("Hash table full".to_string()));
            }

            // Link the chain
            self.buckets[current_idx].next = next_idx as i32;
            self.buckets[next_idx] = HashBucket::new(hash, offset);
            self.entry_count += 1;
            return Ok(());
        }

        // Different hash - use linear probing to find empty slot
        let mut probe_idx = (index + 1) % self.buckets.len();
        let mut probe_count = 0;

        while !self.buckets[probe_idx].is_empty() && probe_count < self.buckets.len() {
            probe_idx = (probe_idx + 1) % self.buckets.len();
            probe_count += 1;
        }

        if probe_count >= self.buckets.len() {
            return Err(BCSError::Index("Hash table full".to_string()));
        }

        self.buckets[probe_idx] = HashBucket::new(hash, offset);
        self.entry_count += 1;
        Ok(())
    }

    /// Lookup an offset by hash
    /// Returns None if not found
    pub fn lookup(&self, hash: u64) -> Option<u64> {
        let index = self.bucket_index(hash);

        if self.buckets[index].is_empty() {
            return None;
        }

        // Check the primary bucket
        if self.buckets[index].hash == hash {
            // Follow the collision chain if it exists
            let mut current_idx = index;
            loop {
                if self.buckets[current_idx].hash == hash {
                    return Some(self.buckets[current_idx].offset);
                }
                if self.buckets[current_idx].next == -1 {
                    break;
                }
                current_idx = self.buckets[current_idx].next as usize;
            }
        }

        // Linear probe to find the hash
        let mut probe_idx = (index + 1) % self.buckets.len();
        let mut probe_count = 0;

        while !self.buckets[probe_idx].is_empty() && probe_count < self.buckets.len() {
            if self.buckets[probe_idx].hash == hash {
                return Some(self.buckets[probe_idx].offset);
            }
            probe_idx = (probe_idx + 1) % self.buckets.len();
            probe_count += 1;
        }

        None
    }

    /// Lookup an offset by exact key name.
    ///
    /// This uses the key hash as primary filter and validates the stored
    /// field name to avoid returning wrong values in hash-collision cases.
    pub fn lookup_key_exact(&self, key: &str) -> Option<u64> {
        let hash = hash_key(key);
        let index = self.bucket_index(hash);

        if self.buckets[index].is_empty() {
            return None;
        }

        // Check primary bucket and collision chain first
        let mut current_idx = index;
        loop {
            let bucket = &self.buckets[current_idx];
            if !bucket.is_empty() && bucket.hash == hash {
                if let Some(field_name) = &bucket.field_name {
                    if field_name == key {
                        return Some(bucket.offset);
                    }
                }
            }

            if bucket.next == -1 {
                break;
            }
            current_idx = bucket.next as usize;
        }

        // Fallback linear probing (for entries placed outside chain)
        let mut probe_idx = (index + 1) % self.buckets.len();
        let mut probe_count = 0;

        while !self.buckets[probe_idx].is_empty() && probe_count < self.buckets.len() {
            let bucket = &self.buckets[probe_idx];
            if bucket.hash == hash {
                if let Some(field_name) = &bucket.field_name {
                    if field_name == key {
                        return Some(bucket.offset);
                    }
                }
            }
            probe_idx = (probe_idx + 1) % self.buckets.len();
            probe_count += 1;
        }

        None
    }

    /// Resize the hash table to accommodate more entries
    fn resize(&mut self) -> Result<()> {
        let new_size = self.buckets.len() * 2;
        let old_buckets = std::mem::replace(&mut self.buckets, vec![HashBucket::empty(); new_size]);
        let previous_load_factor = self.load_factor;
        // Prevent recursive resize while reinserting into the enlarged table.
        self.load_factor = 1.0;
        self.entry_count = 0;

        // Reinsert all entries, preserving field names when present.
        for bucket in old_buckets {
            if !bucket.is_empty() {
                if let Some(field_name) = bucket.field_name {
                    self.insert_with_name(bucket.hash, bucket.offset, field_name)?;
                } else {
                    self.insert(bucket.hash, bucket.offset)?;
                }
            }
        }

        self.load_factor = previous_load_factor;
        Ok(())
    }

    /// Write the index table to a writer
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        // Write header
        writer.write_all(&self.entry_count.to_le_bytes())?;
        writer.write_all(&(self.buckets.len() as u32).to_le_bytes())?;
        writer.write_all(&self.load_factor.to_le_bytes())?;

        // Write only occupied buckets to keep on-disk representation compact.
        for (index, bucket) in self.buckets.iter().enumerate() {
            if bucket.is_empty() {
                continue;
            }

            // Persist bucket index so we can rebuild sparse table layout on read.
            writer.write_all(&(index as u32).to_le_bytes())?;
            bucket.write(writer)?;
        }

        Ok(())
    }

    /// Read the index table from a reader
    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        // Read header
        let mut entry_count_buf = [0u8; 4];
        let mut bucket_count_buf = [0u8; 4];
        let mut load_factor_buf = [0u8; 4];

        reader.read_exact(&mut entry_count_buf)?;
        reader.read_exact(&mut bucket_count_buf)?;
        reader.read_exact(&mut load_factor_buf)?;

        let entry_count = u32::from_le_bytes(entry_count_buf);
        let bucket_count = u32::from_le_bytes(bucket_count_buf);
        let load_factor = f32::from_le_bytes(load_factor_buf);

        limits::ensure_count(entry_count as usize, MAX_INDEX_ENTRIES, "Index entry")?;
        limits::ensure_count(bucket_count as usize, MAX_INDEX_BUCKETS, "Index bucket")?;

        // Reconstruct sparse bucket table by reading only occupied buckets.
        let mut buckets = vec![HashBucket::empty(); bucket_count as usize];
        for _ in 0..entry_count {
            let mut index_buf = [0u8; 4];
            reader.read_exact(&mut index_buf)?;
            let index = u32::from_le_bytes(index_buf) as usize;

            if index >= buckets.len() {
                return Err(BCSError::Index(format!(
                    "Invalid bucket index {} (bucket_count {})",
                    index,
                    buckets.len()
                )));
            }

            buckets[index] = HashBucket::read(reader)?;
        }

        Ok(Self {
            buckets,
            entry_count,
            load_factor,
        })
    }

    /// Calculate the size of the index table in bytes
    pub fn size_bytes(&self) -> usize {
        // Header: 4 + 4 + 4 = 12 bytes
        // Sparse buckets: one u32 index + bucket payload per occupied bucket
        let mut total_size = 12; // Header
        for bucket in &self.buckets {
            if bucket.is_empty() {
                continue;
            }

            total_size += 4; // bucket index
            total_size += HashBucket::BASE_SIZE;
            if let Some(ref name) = bucket.field_name {
                total_size += 4 + name.len(); // Length prefix + string bytes
            } else {
                total_size += 4; // Empty name length
            }
        }
        total_size
    }
}

impl Default for IndexTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Index Table Builder
// ============================================================================

/// Builder for constructing an index table from configuration data
pub struct IndexTableBuilder {
    /// Entries to be inserted (key, offset)
    entries: Vec<(String, u64)>,

    /// Load factor for the table
    load_factor: f32,
}

impl IndexTableBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            load_factor: IndexTable::DEFAULT_LOAD_FACTOR,
        }
    }

    /// Set the load factor
    pub fn with_load_factor(mut self, load_factor: f32) -> Self {
        self.load_factor = load_factor;
        self
    }

    /// Add a top-level key with its offset
    pub fn add_entry(&mut self, key: String, offset: u64) {
        self.entries.push((key, offset));
    }

    /// Add multiple entries at once
    pub fn add_entries(&mut self, entries: Vec<(String, u64)>) {
        self.entries.extend(entries);
    }

    /// Build the index table
    pub fn build(self) -> Result<IndexTable> {
        let capacity = self.entries.len();
        let mut table = IndexTable::with_capacity_and_load_factor(capacity, self.load_factor);

        // Compute hashes and insert all entries with field names
        for (key, offset) in self.entries {
            let hash = hash_key(&key);
            table.insert_with_name(hash, offset, key)?;
        }

        Ok(table)
    }

    /// Build from a map of keys to offsets
    pub fn from_map(map: std::collections::HashMap<String, u64>) -> Result<IndexTable> {
        let mut builder = Self::new();
        for (key, offset) in map {
            builder.add_entry(key, offset);
        }
        builder.build()
    }

    /// Build from a vector of key-offset pairs
    pub fn from_vec(entries: Vec<(String, u64)>) -> Result<IndexTable> {
        let mut builder = Self::new();
        builder.add_entries(entries);
        builder.build()
    }
}

impl Default for IndexTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Path Resolution
// ============================================================================

/// Represents a segment in a configuration path
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// A named field (e.g., "networking")
    Field(String),

    /// An array index (e.g., `[0]`)
    Index(usize),

    /// A wildcard array index (e.g., [$] or .$.)
    WildcardIndex,
}

/// Parse a path string into segments
/// Examples:
/// - `"networking.interfaces[0].ip"` -> `[Field("networking"), Field("interfaces"), Index(0), Field("ip")]`
/// - `"a.b.c"` -> `[Field("a"), Field("b"), Field("c")]`
pub fn parse_path(path: &str) -> Result<Vec<PathSegment>> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_bracket = false;

    for ch in path.chars() {
        match ch {
            '.' if !in_bracket => {
                if !current.is_empty() {
                    if current == "$" {
                        segments.push(PathSegment::WildcardIndex);
                    } else {
                        segments.push(PathSegment::Field(current.clone()));
                    }
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    if current == "$" {
                        segments.push(PathSegment::WildcardIndex);
                    } else {
                        segments.push(PathSegment::Field(current.clone()));
                    }
                    current.clear();
                }
                in_bracket = true;
            }
            ']' => {
                if in_bracket {
                    if current == "$" {
                        segments.push(PathSegment::WildcardIndex);
                    } else {
                        let index = current.parse::<usize>().map_err(|_| {
                            BCSError::Index(format!("Invalid array index: {}", current))
                        })?;
                        segments.push(PathSegment::Index(index));
                    }
                    current.clear();
                    in_bracket = false;
                } else {
                    return Err(BCSError::Index("Unexpected ']' in path".to_string()));
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if in_bracket {
        return Err(BCSError::Index("Unclosed '[' in path".to_string()));
    }

    if !current.is_empty() {
        if current == "$" {
            segments.push(PathSegment::WildcardIndex);
        } else {
            segments.push(PathSegment::Field(current));
        }
    }

    Ok(segments)
}

// ============================================================================
// Path Cache
// ============================================================================

/// Cache for frequently accessed paths
pub struct PathCache {
    /// Map from path string to offset
    cache: std::collections::HashMap<String, u64>,

    /// Maximum cache size
    max_size: usize,

    /// Access count for LRU eviction
    access_count: std::collections::HashMap<String, usize>,
}

impl PathCache {
    /// Default maximum cache size
    const DEFAULT_MAX_SIZE: usize = 1000;

    /// Create a new path cache
    pub fn new() -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            max_size: Self::DEFAULT_MAX_SIZE,
            access_count: std::collections::HashMap::new(),
        }
    }

    /// Create a new path cache with specified max size
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            cache: std::collections::HashMap::with_capacity(max_size),
            max_size,
            access_count: std::collections::HashMap::with_capacity(max_size),
        }
    }

    /// Get an offset from the cache
    pub fn get(&mut self, path: &str) -> Option<u64> {
        if let Some(&offset) = self.cache.get(path) {
            // Update access count
            *self.access_count.entry(path.to_string()).or_insert(0) += 1;
            Some(offset)
        } else {
            None
        }
    }

    /// Insert a path-offset pair into the cache
    pub fn insert(&mut self, path: String, offset: u64) {
        // Check if we need to evict
        if self.cache.len() >= self.max_size && !self.cache.contains_key(&path) {
            self.evict_lru();
        }

        self.cache.insert(path.clone(), offset);
        self.access_count.insert(path, 1);
    }

    /// Evict the least recently used entry
    fn evict_lru(&mut self) {
        if let Some((lru_path, _)) = self.access_count.iter().min_by_key(|(_, &count)| count) {
            let lru_path = lru_path.clone();
            self.cache.remove(&lru_path);
            self.access_count.remove(&lru_path);
        }
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_count.clear();
    }

    /// Get the number of cached entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for PathCache {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Index Table Lookup with Path Resolution
// ============================================================================

/// Extended index table with path resolution and caching
pub struct IndexTableLookup {
    /// The underlying index table
    table: IndexTable,

    /// Path cache for frequently accessed paths
    cache: PathCache,
}

impl IndexTableLookup {
    /// Create a new lookup from an index table
    pub fn new(table: IndexTable) -> Self {
        Self {
            table,
            cache: PathCache::new(),
        }
    }

    /// Create a new lookup with a custom cache size
    pub fn with_cache_size(table: IndexTable, cache_size: usize) -> Self {
        Self {
            table,
            cache: PathCache::with_capacity(cache_size),
        }
    }

    /// Lookup by key (simple, non-nested)
    pub fn lookup_key(&self, key: &str) -> Option<u64> {
        let hash = hash_key(key);
        self.table.lookup(hash)
    }

    /// Lookup by path with caching
    /// For simple keys, this is O(1)
    /// For nested paths, this requires following offsets (still fast with cache)
    pub fn lookup_path(&mut self, path: &str) -> Option<u64> {
        // Check cache first
        if let Some(offset) = self.cache.get(path) {
            return Some(offset);
        }

        // Parse the path
        let segments = match parse_path(path) {
            Ok(s) => s,
            Err(_) => return None,
        };

        if segments.is_empty() {
            return None;
        }

        // For now, we only support top-level keys in the index table
        // Nested resolution would require reading the data layer
        // This is a simplified implementation that handles the first segment
        if let PathSegment::Field(key) = &segments[0] {
            let offset = self.lookup_key(key)?;

            // If this is a simple path (no nesting), cache and return
            if segments.len() == 1 {
                self.cache.insert(path.to_string(), offset);
                return Some(offset);
            }

            // For nested paths, we would need to:
            // 1. Read the value at offset
            // 2. Navigate through the structure based on remaining segments
            // 3. Return the final offset
            // This requires access to the data layer, which is not available here
            // For now, return the offset of the top-level key
            Some(offset)
        } else {
            None
        }
    }

    /// Get the underlying index table
    pub fn table(&self) -> &IndexTable {
        &self.table
    }

    /// Get the cache
    pub fn cache(&self) -> &PathCache {
        &self.cache
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xxhash64_basic() {
        let hash1 = hash_key("hello");
        let hash2 = hash_key("hello");
        let hash3 = hash_key("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_hash_bucket_empty() {
        let bucket = HashBucket::empty();
        assert!(bucket.is_empty());
        assert_eq!(bucket.next, -1);
    }

    #[test]
    fn test_hash_bucket_new() {
        let bucket = HashBucket::new(12345, 67890);
        assert!(!bucket.is_empty());
        assert_eq!(bucket.hash, 12345);
        assert_eq!(bucket.offset, 67890);
        assert_eq!(bucket.next, -1);
    }

    #[test]
    fn test_index_table_insert_and_lookup() {
        let mut table = IndexTable::new();

        let hash1 = hash_key("key1");
        let hash2 = hash_key("key2");

        table.insert(hash1, 100).unwrap();
        table.insert(hash2, 200).unwrap();

        assert_eq!(table.len(), 2);
        assert_eq!(table.lookup(hash1), Some(100));
        assert_eq!(table.lookup(hash2), Some(200));
    }

    #[test]
    fn test_index_table_collision() {
        let mut table = IndexTable::with_capacity(4);

        // Insert multiple entries that may collide
        for i in 0..10 {
            let key = format!("key{}", i);
            let hash = hash_key(&key);
            table.insert(hash, i * 100).unwrap();
        }

        assert_eq!(table.len(), 10);

        // Verify all entries can be found
        for i in 0..10 {
            let key = format!("key{}", i);
            let hash = hash_key(&key);
            assert_eq!(table.lookup(hash), Some(i * 100));
        }
    }

    #[test]
    fn test_index_table_not_found() {
        let table = IndexTable::new();
        let hash = hash_key("nonexistent");
        assert_eq!(table.lookup(hash), None);
    }

    #[test]
    fn test_index_table_resize() {
        let mut table = IndexTable::with_capacity(4);

        // Insert enough entries to trigger resize
        for i in 0..20 {
            let key = format!("key{}", i);
            let hash = hash_key(&key);
            table.insert(hash, i * 100).unwrap();
        }

        assert_eq!(table.len(), 20);
        assert!(table.bucket_count() > 4);

        // Verify all entries still accessible after resize
        for i in 0..20 {
            let key = format!("key{}", i);
            let hash = hash_key(&key);
            assert_eq!(table.lookup(hash), Some(i * 100));
        }
    }

    #[test]
    fn test_index_table_resize_preserves_field_names() {
        let mut table = IndexTable::with_capacity(4);

        for i in 0..20 {
            let key = format!("named_key_{}", i);
            let hash = hash_key(&key);
            table.insert_with_name(hash, i * 100, key.clone()).unwrap();
            assert_eq!(
                table.lookup_key_exact(&key),
                Some(i * 100),
                "exact name lookup must work before and after any resize"
            );
        }

        assert!(table.bucket_count() > 4);
        for i in 0..20 {
            let key = format!("named_key_{}", i);
            assert_eq!(table.lookup_key_exact(&key), Some(i * 100));
        }
    }

    #[test]
    fn test_builder_basic() {
        let mut builder = IndexTableBuilder::new();
        builder.add_entry("key1".to_string(), 100);
        builder.add_entry("key2".to_string(), 200);
        builder.add_entry("key3".to_string(), 300);

        let table = builder.build().unwrap();

        assert_eq!(table.len(), 3);
        assert_eq!(table.lookup(hash_key("key1")), Some(100));
        assert_eq!(table.lookup(hash_key("key2")), Some(200));
        assert_eq!(table.lookup(hash_key("key3")), Some(300));
    }

    #[test]
    fn test_builder_from_vec() {
        let entries = vec![
            ("host".to_string(), 1000),
            ("port".to_string(), 2000),
            ("database".to_string(), 3000),
        ];

        let table = IndexTableBuilder::from_vec(entries).unwrap();

        assert_eq!(table.len(), 3);
        assert_eq!(table.lookup(hash_key("host")), Some(1000));
        assert_eq!(table.lookup(hash_key("port")), Some(2000));
        assert_eq!(table.lookup(hash_key("database")), Some(3000));
    }

    #[test]
    fn test_builder_from_map() {
        let mut map = std::collections::HashMap::new();
        map.insert("server".to_string(), 5000);
        map.insert("client".to_string(), 6000);

        let table = IndexTableBuilder::from_map(map).unwrap();

        assert_eq!(table.len(), 2);
        assert_eq!(table.lookup(hash_key("server")), Some(5000));
        assert_eq!(table.lookup(hash_key("client")), Some(6000));
    }

    #[test]
    fn test_builder_large_dataset() {
        let mut builder = IndexTableBuilder::new();

        // Build a large index table
        for i in 0..1000 {
            let key = format!("config_key_{}", i);
            builder.add_entry(key, i * 1000);
        }

        let table = builder.build().unwrap();

        assert_eq!(table.len(), 1000);

        // Verify random access
        for i in (0..1000).step_by(100) {
            let key = format!("config_key_{}", i);
            assert_eq!(table.lookup(hash_key(&key)), Some(i * 1000));
        }
    }

    #[test]
    fn test_parse_path_simple() {
        let segments = parse_path("a.b.c").unwrap();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], PathSegment::Field("a".to_string()));
        assert_eq!(segments[1], PathSegment::Field("b".to_string()));
        assert_eq!(segments[2], PathSegment::Field("c".to_string()));
    }

    #[test]
    fn test_parse_path_with_array() {
        let segments = parse_path("networking.interfaces[0].ip").unwrap();
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0], PathSegment::Field("networking".to_string()));
        assert_eq!(segments[1], PathSegment::Field("interfaces".to_string()));
        assert_eq!(segments[2], PathSegment::Index(0));
        assert_eq!(segments[3], PathSegment::Field("ip".to_string()));
    }

    #[test]
    fn test_parse_path_multiple_arrays() {
        let segments = parse_path("data[0].items[5].value").unwrap();
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], PathSegment::Field("data".to_string()));
        assert_eq!(segments[1], PathSegment::Index(0));
        assert_eq!(segments[2], PathSegment::Field("items".to_string()));
        assert_eq!(segments[3], PathSegment::Index(5));
        assert_eq!(segments[4], PathSegment::Field("value".to_string()));
    }

    #[test]
    fn test_parse_path_wildcard_dot_syntax() {
        let segments = parse_path("services.$.routes.$.paths").unwrap();
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], PathSegment::Field("services".to_string()));
        assert_eq!(segments[1], PathSegment::WildcardIndex);
        assert_eq!(segments[2], PathSegment::Field("routes".to_string()));
        assert_eq!(segments[3], PathSegment::WildcardIndex);
        assert_eq!(segments[4], PathSegment::Field("paths".to_string()));
    }

    #[test]
    fn test_parse_path_wildcard_bracket_syntax() {
        let segments = parse_path("services[$].routes[$].paths").unwrap();
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], PathSegment::Field("services".to_string()));
        assert_eq!(segments[1], PathSegment::WildcardIndex);
        assert_eq!(segments[2], PathSegment::Field("routes".to_string()));
        assert_eq!(segments[3], PathSegment::WildcardIndex);
        assert_eq!(segments[4], PathSegment::Field("paths".to_string()));
    }

    #[test]
    fn test_parse_path_single_field() {
        let segments = parse_path("hostname").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], PathSegment::Field("hostname".to_string()));
    }

    #[test]
    fn test_parse_path_invalid() {
        assert!(parse_path("a[b]").is_err()); // Invalid index
        assert!(parse_path("a[").is_err()); // Unclosed bracket
        assert!(parse_path("a]").is_err()); // Unexpected bracket
    }

    #[test]
    fn test_path_cache_basic() {
        let mut cache = PathCache::new();

        cache.insert("key1".to_string(), 100);
        cache.insert("key2".to_string(), 200);

        assert_eq!(cache.get("key1"), Some(100));
        assert_eq!(cache.get("key2"), Some(200));
        assert_eq!(cache.get("key3"), None);
    }

    #[test]
    fn test_path_cache_lru_eviction() {
        let mut cache = PathCache::with_capacity(3);

        cache.insert("key1".to_string(), 100);
        cache.insert("key2".to_string(), 200);
        cache.insert("key3".to_string(), 300);

        // Access key1 and key2 to increase their counts
        cache.get("key1");
        cache.get("key2");

        // Insert key4, should evict key3 (least accessed)
        cache.insert("key4".to_string(), 400);

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("key1"), Some(100));
        assert_eq!(cache.get("key2"), Some(200));
        assert_eq!(cache.get("key3"), None); // Evicted
        assert_eq!(cache.get("key4"), Some(400));
    }

    #[test]
    fn test_index_table_lookup_simple() {
        let mut builder = IndexTableBuilder::new();
        builder.add_entry("host".to_string(), 1000);
        builder.add_entry("port".to_string(), 2000);

        let table = builder.build().unwrap();
        let lookup = IndexTableLookup::new(table);

        assert_eq!(lookup.lookup_key("host"), Some(1000));
        assert_eq!(lookup.lookup_key("port"), Some(2000));
        assert_eq!(lookup.lookup_key("missing"), None);
    }

    #[test]
    fn test_index_table_lookup_with_cache() {
        let mut builder = IndexTableBuilder::new();
        builder.add_entry("database".to_string(), 5000);

        let table = builder.build().unwrap();
        let mut lookup = IndexTableLookup::new(table);

        // First lookup - not cached
        assert_eq!(lookup.lookup_path("database"), Some(5000));
        assert_eq!(lookup.cache().len(), 1);

        // Second lookup - from cache
        assert_eq!(lookup.lookup_path("database"), Some(5000));

        // Clear cache
        lookup.clear_cache();
        assert_eq!(lookup.cache().len(), 0);
    }

    #[test]
    fn test_index_table_lookup_nested_path() {
        let mut builder = IndexTableBuilder::new();
        builder.add_entry("networking".to_string(), 10000);

        let table = builder.build().unwrap();
        let mut lookup = IndexTableLookup::new(table);

        // For nested paths, we return the top-level offset
        // Full nested resolution requires data layer access
        assert_eq!(
            lookup.lookup_path("networking.interfaces[0].ip"),
            Some(10000)
        );
    }
}
