// Comprehensive unit tests for index table functionality
// Tests hash function distribution, collision handling, path resolution, and performance

use bcs_core::index::{
    hash_bytes, hash_key, parse_path, IndexTable, IndexTableBuilder, IndexTableLookup, PathCache,
    PathSegment,
};

// ============================================================================
// Hash Function Distribution Tests
// ============================================================================

#[test]
fn test_hash_function_deterministic() {
    // Hash function should be deterministic
    let key = "test_key";
    let hash1 = hash_key(key);
    let hash2 = hash_key(key);
    assert_eq!(hash1, hash2, "Hash function should be deterministic");
}

#[test]
fn test_hash_function_different_keys() {
    // Different keys should produce different hashes
    let keys = vec!["key1", "key2", "key3", "key4", "key5"];
    let mut hashes = Vec::new();

    for key in &keys {
        hashes.push(hash_key(key));
    }

    // Check that all hashes are unique
    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            assert_ne!(
                hashes[i], hashes[j],
                "Different keys should produce different hashes: {} vs {}",
                keys[i], keys[j]
            );
        }
    }
}

#[test]
fn test_hash_function_distribution() {
    // Test that hash function distributes keys well across buckets
    let num_keys = 1000;
    let num_buckets = 128;
    let mut bucket_counts = vec![0; num_buckets];

    for i in 0..num_keys {
        let key = format!("key_{}", i);
        let hash = hash_key(&key);
        let bucket = (hash as usize) % num_buckets;
        bucket_counts[bucket] += 1;
    }

    // Calculate expected count per bucket
    let expected = num_keys as f64 / num_buckets as f64;

    // Calculate standard deviation to measure distribution quality
    let variance: f64 = bucket_counts
        .iter()
        .map(|&count| {
            let diff = count as f64 - expected;
            diff * diff
        })
        .sum::<f64>()
        / num_buckets as f64;

    let std_dev = variance.sqrt();

    // For a good hash function, std dev should be relatively small
    // With perfect uniform distribution, std_dev would be ~2.8 for these parameters
    // Allow up to 4.0 to account for natural variance
    assert!(
        std_dev < 4.0,
        "Hash distribution std dev {} is too high (expected < 4.0)",
        std_dev
    );

    // Also check that no bucket is completely empty or extremely full
    let min_count = *bucket_counts.iter().min().unwrap();
    let max_count = *bucket_counts.iter().max().unwrap();

    assert!(min_count > 0, "Some buckets are empty");
    assert!(
        (max_count as f64) < expected * 2.5,
        "Some buckets are too full: {} (expected ~{})",
        max_count,
        expected
    );
}

#[test]
fn test_hash_function_avalanche_effect() {
    // Small changes in input should cause large changes in hash
    let base = "configuration_key";
    let hash_base = hash_key(base);

    // Change one character
    let modified = "configuration_ley"; // 'k' -> 'l'
    let hash_modified = hash_key(modified);

    // Count differing bits
    let xor = hash_base ^ hash_modified;
    let differing_bits = xor.count_ones();

    // At least 25% of bits should differ (16 out of 64)
    assert!(
        differing_bits >= 16,
        "Avalanche effect: only {} bits differ, expected at least 16",
        differing_bits
    );
}

#[test]
fn test_hash_bytes_consistency() {
    // hash_bytes should produce same result as hash_key for string bytes
    let key = "test_string";
    let hash_from_key = hash_key(key);
    let hash_from_bytes = hash_bytes(key.as_bytes());
    assert_eq!(
        hash_from_key, hash_from_bytes,
        "hash_key and hash_bytes should produce same result"
    );
}

// ============================================================================
// Collision Handling Tests
// ============================================================================

#[test]
fn test_collision_handling_basic() {
    // Create a small table to force collisions
    let mut table = IndexTable::with_capacity(4);

    // Insert many entries to force collisions
    let num_entries = 20;
    for i in 0..num_entries {
        let key = format!("key_{}", i);
        let hash = hash_key(&key);
        table.insert(hash, i * 100).expect("Insert should succeed");
    }

    assert_eq!(
        table.len(),
        num_entries as usize,
        "All entries should be inserted"
    );

    // Verify all entries can be retrieved
    for i in 0..num_entries {
        let key = format!("key_{}", i);
        let hash = hash_key(&key);
        let offset = table.lookup(hash);
        assert_eq!(
            offset,
            Some(i * 100),
            "Should find entry for key_{} after collisions",
            i
        );
    }
}

#[test]
fn test_collision_rate_calculation() {
    let mut table = IndexTable::with_capacity(8);

    // Insert entries
    for i in 0..10 {
        let key = format!("item_{}", i);
        let hash = hash_key(&key);
        table.insert(hash, i * 1000).unwrap();
    }

    let collision_rate = table.collision_rate();

    // Collision rate should be between 0 and 1
    assert!(
        (0.0..=1.0).contains(&collision_rate),
        "Collision rate should be between 0 and 1, got {}",
        collision_rate
    );
}

#[test]
fn test_collision_chain_integrity() {
    // Test that collision chains maintain integrity
    let mut table = IndexTable::with_capacity(4);
    let mut inserted_keys = Vec::new();

    // Insert 50 entries to create long collision chains
    for i in 0..50 {
        let key = format!("config_{}", i);
        let hash = hash_key(&key);
        table.insert(hash, i as u64).unwrap();
        inserted_keys.push((key, hash, i as u64));
    }

    // Verify every single entry can still be found
    for (key, hash, expected_offset) in inserted_keys {
        let found_offset = table.lookup(hash);
        assert_eq!(
            found_offset,
            Some(expected_offset),
            "Failed to find key '{}' after collision chain",
            key
        );
    }
}

#[test]
fn test_collision_with_similar_keys() {
    // Test collision handling with keys that might hash similarly
    let mut table = IndexTable::with_capacity(8);

    let similar_keys = vec![
        "server_host",
        "server_port",
        "server_name",
        "client_host",
        "client_port",
        "client_name",
        "database_host",
        "database_port",
        "database_name",
    ];

    for (idx, key) in similar_keys.iter().enumerate() {
        let hash = hash_key(key);
        table.insert(hash, (idx * 100) as u64).unwrap();
    }

    // Verify all can be retrieved
    for (idx, key) in similar_keys.iter().enumerate() {
        let hash = hash_key(key);
        assert_eq!(
            table.lookup(hash),
            Some((idx * 100) as u64),
            "Failed to retrieve similar key: {}",
            key
        );
    }
}

#[test]
fn test_table_resize_preserves_entries() {
    // Test that resizing preserves all entries
    let mut table = IndexTable::with_capacity(4);
    let initial_bucket_count = table.bucket_count();

    // Insert enough entries to trigger resize
    let num_entries = 50;
    for i in 0..num_entries {
        let key = format!("entry_{}", i);
        let hash = hash_key(&key);
        table.insert(hash, i * 10).unwrap();
    }

    // Table should have resized
    assert!(
        table.bucket_count() > initial_bucket_count,
        "Table should have resized"
    );

    // All entries should still be accessible
    for i in 0..num_entries {
        let key = format!("entry_{}", i);
        let hash = hash_key(&key);
        assert_eq!(
            table.lookup(hash),
            Some(i * 10),
            "Entry {} should be accessible after resize",
            i
        );
    }
}

// ============================================================================
// Path Resolution Tests
// ============================================================================

#[test]
fn test_parse_simple_path() {
    let segments = parse_path("hostname").unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0], PathSegment::Field("hostname".to_string()));
}

#[test]
fn test_parse_dotted_path() {
    let segments = parse_path("server.config.host").unwrap();
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0], PathSegment::Field("server".to_string()));
    assert_eq!(segments[1], PathSegment::Field("config".to_string()));
    assert_eq!(segments[2], PathSegment::Field("host".to_string()));
}

#[test]
fn test_parse_path_with_array_index() {
    let segments = parse_path("servers[0]").unwrap();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0], PathSegment::Field("servers".to_string()));
    assert_eq!(segments[1], PathSegment::Index(0));
}

#[test]
fn test_parse_complex_nested_path() {
    let segments = parse_path("networking.interfaces[0].ipv4.address").unwrap();
    assert_eq!(segments.len(), 5);
    assert_eq!(segments[0], PathSegment::Field("networking".to_string()));
    assert_eq!(segments[1], PathSegment::Field("interfaces".to_string()));
    assert_eq!(segments[2], PathSegment::Index(0));
    assert_eq!(segments[3], PathSegment::Field("ipv4".to_string()));
    assert_eq!(segments[4], PathSegment::Field("address".to_string()));
}

#[test]
fn test_parse_path_multiple_array_indices() {
    let segments = parse_path("data[0].items[5].values[10]").unwrap();
    assert_eq!(segments.len(), 6);
    assert_eq!(segments[0], PathSegment::Field("data".to_string()));
    assert_eq!(segments[1], PathSegment::Index(0));
    assert_eq!(segments[2], PathSegment::Field("items".to_string()));
    assert_eq!(segments[3], PathSegment::Index(5));
    assert_eq!(segments[4], PathSegment::Field("values".to_string()));
    assert_eq!(segments[5], PathSegment::Index(10));
}

#[test]
fn test_parse_path_with_underscores() {
    let segments = parse_path("database_config.connection_pool.max_size").unwrap();
    assert_eq!(segments.len(), 3);
    assert_eq!(
        segments[0],
        PathSegment::Field("database_config".to_string())
    );
    assert_eq!(
        segments[1],
        PathSegment::Field("connection_pool".to_string())
    );
    assert_eq!(segments[2], PathSegment::Field("max_size".to_string()));
}

#[test]
fn test_parse_path_invalid_bracket() {
    assert!(parse_path("key[").is_err(), "Unclosed bracket should error");
    assert!(
        parse_path("key]").is_err(),
        "Unexpected bracket should error"
    );
    assert!(
        parse_path("key[abc]").is_err(),
        "Non-numeric index should error"
    );
}

#[test]
fn test_parse_empty_path() {
    let segments = parse_path("").unwrap();
    assert_eq!(
        segments.len(),
        0,
        "Empty path should produce empty segments"
    );
}

#[test]
fn test_path_cache_hit_and_miss() {
    let mut cache = PathCache::new();

    // Cache miss
    assert_eq!(cache.get("path1"), None);

    // Insert and hit
    cache.insert("path1".to_string(), 1000);
    assert_eq!(cache.get("path1"), Some(1000));

    // Another miss
    assert_eq!(cache.get("path2"), None);
}

#[test]
fn test_path_cache_eviction() {
    let mut cache = PathCache::with_capacity(3);

    // Fill cache
    cache.insert("path1".to_string(), 100);
    cache.insert("path2".to_string(), 200);
    cache.insert("path3".to_string(), 300);
    assert_eq!(cache.len(), 3);

    // Access path1 and path2 multiple times
    for _ in 0..5 {
        cache.get("path1");
        cache.get("path2");
    }

    // Insert new path, should evict path3 (least accessed)
    cache.insert("path4".to_string(), 400);

    assert_eq!(cache.len(), 3);
    assert_eq!(cache.get("path1"), Some(100));
    assert_eq!(cache.get("path2"), Some(200));
    assert_eq!(cache.get("path3"), None); // Evicted
    assert_eq!(cache.get("path4"), Some(400));
}

#[test]
fn test_index_table_lookup_with_path_caching() {
    let mut builder = IndexTableBuilder::new();
    builder.add_entry("server".to_string(), 5000);
    builder.add_entry("database".to_string(), 6000);
    builder.add_entry("cache".to_string(), 7000);

    let table = builder.build().unwrap();
    let mut lookup = IndexTableLookup::new(table);

    // First access - not cached
    assert_eq!(lookup.cache().len(), 0);
    assert_eq!(lookup.lookup_path("server"), Some(5000));
    assert_eq!(lookup.cache().len(), 1);

    // Second access - from cache
    assert_eq!(lookup.lookup_path("server"), Some(5000));

    // Access other paths
    assert_eq!(lookup.lookup_path("database"), Some(6000));
    assert_eq!(lookup.lookup_path("cache"), Some(7000));
    assert_eq!(lookup.cache().len(), 3);
}

#[test]
fn test_index_table_lookup_exact_key_name() {
    let mut builder = IndexTableBuilder::new();
    builder.add_entry("server".to_string(), 5000);
    builder.add_entry("service".to_string(), 6000);

    let table = builder.build().unwrap();

    assert_eq!(table.lookup_key_exact("server"), Some(5000));
    assert_eq!(table.lookup_key_exact("service"), Some(6000));
    assert_eq!(table.lookup_key_exact("missing"), None);
}

// ============================================================================
// Performance Tests with Large Key Sets
// ============================================================================

#[test]
fn test_large_dataset_insertion() {
    let num_keys = 10_000;
    let mut builder = IndexTableBuilder::new();

    // Insert large number of keys
    for i in 0..num_keys {
        let key = format!("config_key_{:06}", i);
        builder.add_entry(key, i * 100);
    }

    let table = builder.build().unwrap();
    assert_eq!(table.len(), num_keys as usize);
}

#[test]
fn test_large_dataset_lookup_performance() {
    let num_keys = 10_000;
    let mut builder = IndexTableBuilder::new();

    // Build large index
    for i in 0..num_keys {
        let key = format!("key_{:06}", i);
        builder.add_entry(key, i * 1000);
    }

    let table = builder.build().unwrap();

    // Test random lookups
    let test_indices = vec![0, 100, 500, 1000, 5000, 9999];
    for idx in test_indices {
        let key = format!("key_{:06}", idx);
        let hash = hash_key(&key);
        let result = table.lookup(hash);
        assert_eq!(
            result,
            Some(idx * 1000),
            "Failed to lookup key at index {}",
            idx
        );
    }
}

#[test]
fn test_large_dataset_collision_rate() {
    let num_keys = 5_000;
    let mut builder = IndexTableBuilder::new();

    for i in 0..num_keys {
        let key = format!("item_{}", i);
        builder.add_entry(key, i as u64);
    }

    let table = builder.build().unwrap();
    let collision_rate = table.collision_rate();

    // With a good hash function and proper sizing, collision rate should be reasonable
    assert!(
        collision_rate < 0.5,
        "Collision rate too high: {}",
        collision_rate
    );
}

#[test]
fn test_large_dataset_all_keys_retrievable() {
    let num_keys = 1_000;
    let mut keys_and_hashes = Vec::new();
    let mut builder = IndexTableBuilder::new();

    // Insert keys
    for i in 0..num_keys {
        let key = format!("entry_{:04}", i);
        let hash = hash_key(&key);
        builder.add_entry(key.clone(), i * 50);
        keys_and_hashes.push((key, hash, i * 50));
    }

    let table = builder.build().unwrap();

    // Verify every single key is retrievable
    for (key, hash, expected_offset) in keys_and_hashes {
        let result = table.lookup(hash);
        assert_eq!(
            result,
            Some(expected_offset),
            "Failed to retrieve key: {}",
            key
        );
    }
}

#[test]
fn test_builder_with_duplicate_keys() {
    // Test that builder handles duplicate keys (last one wins)
    let mut builder = IndexTableBuilder::new();

    builder.add_entry("key1".to_string(), 100);
    builder.add_entry("key2".to_string(), 200);
    builder.add_entry("key1".to_string(), 300); // Duplicate

    let table = builder.build().unwrap();

    // The table will have both entries with same hash
    // This is expected behavior - the index table stores all hash-offset pairs
    assert_eq!(table.len(), 3);
}

#[test]
fn test_load_factor_impact() {
    let num_keys = 100;

    // Test with different load factors
    let load_factors = vec![0.5, 0.75, 0.9];

    for load_factor in load_factors {
        let mut builder = IndexTableBuilder::new().with_load_factor(load_factor);

        for i in 0..num_keys {
            let key = format!("key_{}", i);
            builder.add_entry(key, i * 10);
        }

        let table = builder.build().unwrap();

        assert_eq!(table.len(), num_keys as usize);
        assert_eq!(table.load_factor(), load_factor);

        // Verify all keys are retrievable
        for i in 0..num_keys {
            let key = format!("key_{}", i);
            let hash = hash_key(&key);
            assert_eq!(table.lookup(hash), Some(i * 10));
        }
    }
}

#[test]
fn test_sequential_vs_random_keys() {
    // Test that both sequential and random-looking keys work well
    let mut builder = IndexTableBuilder::new();

    // Sequential keys
    for i in 0..100 {
        let key = format!("{:04}", i);
        builder.add_entry(key, i * 100);
    }

    // Random-looking keys
    for i in 0..100 {
        let key = format!("rand_{:x}", i * 7919); // Prime multiplier
        builder.add_entry(key, (i + 100) * 100);
    }

    let table = builder.build().unwrap();
    assert_eq!(table.len(), 200);

    // Verify sequential keys
    for i in 0..100 {
        let key = format!("{:04}", i);
        let hash = hash_key(&key);
        assert_eq!(table.lookup(hash), Some(i * 100));
    }

    // Verify random keys
    for i in 0..100 {
        let key = format!("rand_{:x}", i * 7919);
        let hash = hash_key(&key);
        assert_eq!(table.lookup(hash), Some((i + 100) * 100));
    }
}

#[test]
fn test_bucket_utilization() {
    let num_keys = 1000;
    let mut builder = IndexTableBuilder::new();

    for i in 0..num_keys {
        let key = format!("config_{}", i);
        builder.add_entry(key, i as u64);
    }

    let table = builder.build().unwrap();

    // Check that bucket count is reasonable
    let bucket_count = table.bucket_count();
    let entry_count = table.entry_count();
    let actual_load = entry_count as f32 / bucket_count as f32;

    // Actual load should be close to target load factor
    assert!(
        actual_load <= table.load_factor(),
        "Actual load {} exceeds target load factor {}",
        actual_load,
        table.load_factor()
    );
}

#[test]
fn test_empty_table_operations() {
    let table = IndexTable::new();

    assert_eq!(table.len(), 0);
    assert!(table.is_empty());
    assert_eq!(table.collision_rate(), 0.0);

    // Lookup on empty table
    let hash = hash_key("nonexistent");
    assert_eq!(table.lookup(hash), None);
}

#[test]
fn test_single_entry_table() {
    let mut table = IndexTable::new();
    let hash = hash_key("only_key");

    table.insert(hash, 42).unwrap();

    assert_eq!(table.len(), 1);
    assert!(!table.is_empty());
    assert_eq!(table.lookup(hash), Some(42));
    assert_eq!(table.collision_rate(), 0.0);
}
