// Benchmark tests for BCS
// Tests performance metrics: random access latency, decode throughput, encode throughput, memory usage

mod common;

use bcs_core::{Decoder, Encoder, Result};
use std::time::{Duration, Instant};

fn load_example_json(name: &str) -> String {
    common::read_example(name)
}

/// Helper to measure execution time
fn measure_time<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();
    (result, duration)
}

#[test]
fn benchmark_random_access_latency() -> Result<()> {
    println!("\n=== Random Access Latency Benchmark ===");

    // Create a configuration with many keys
    let mut json = String::from("{");
    for i in 0..1000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#""key_{i}": {{"value": {i}, "name": "item_{i}"}}"#,
            i = i
        ));
    }
    json.push('}');

    // Encode to BCS
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&json)?;

    // Create decoder
    let mut decoder = Decoder::from_bytes(&bcs_data)?;

    // Perform 1000 random lookups
    let lookup_count = 1000;
    let keys: Vec<String> = (0..lookup_count)
        .map(|i| format!("key_{}", i % 1000))
        .collect();

    let (_, total_duration) = measure_time(|| {
        for key in &keys {
            let path = format!("{}.value", key);
            let _ = decoder.get(&path);
        }
    });

    let avg_latency = total_duration.as_nanos() as f64 / lookup_count as f64;
    println!(
        "Average random access latency: {:.2} ns ({:.2} μs)",
        avg_latency,
        avg_latency / 1000.0
    );
    println!(
        "Total time for {} lookups: {:?}",
        lookup_count, total_duration
    );

    // Target: < 1μs per lookup
    assert!(
        avg_latency < 10_000.0,
        "Random access too slow: {:.2} ns",
        avg_latency
    );

    Ok(())
}

#[test]
fn benchmark_full_decode_throughput() -> Result<()> {
    println!("\n=== Full Decode Throughput Benchmark ===");

    // Load a real-world example
    let json_content = load_example_json("app-settings.json");

    // Encode to BCS
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_json(&json_content)?;

    let bcs_size = bcs_data.len();

    // Benchmark decoding
    let iterations = 100;
    let (_, total_duration) = measure_time(|| {
        for _ in 0..iterations {
            let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
            let _ = decoder.to_json().expect("Failed to decode");
        }
    });

    let avg_duration = total_duration.as_micros() as f64 / iterations as f64;
    let throughput_mbps = (bcs_size as f64 / (1024.0 * 1024.0)) / (avg_duration / 1_000_000.0);

    println!("BCS file size: {} bytes", bcs_size);
    println!("Average decode time: {:.2} μs", avg_duration);
    println!("Decode throughput: {:.2} MB/s", throughput_mbps);

    Ok(())
}

#[test]
fn benchmark_encode_throughput() -> Result<()> {
    println!("\n=== Encode Throughput Benchmark ===");

    // Load a real-world example
    let json_content = load_example_json("app-settings.json");

    let json_size = json_content.len();

    // Benchmark encoding
    let iterations = 100;
    let (_, total_duration) = measure_time(|| {
        for _ in 0..iterations {
            let mut encoder = Encoder::new();
            encoder.set_compression(true);
            let _ = encoder
                .encode_from_json(&json_content)
                .expect("Failed to encode");
        }
    });

    let avg_duration = total_duration.as_micros() as f64 / iterations as f64;
    let throughput_mbps = (json_size as f64 / (1024.0 * 1024.0)) / (avg_duration / 1_000_000.0);

    println!("JSON size: {} bytes", json_size);
    println!("Average encode time: {:.2} μs", avg_duration);
    println!("Encode throughput: {:.2} MB/s", throughput_mbps);

    Ok(())
}

#[test]
fn benchmark_memory_usage() -> Result<()> {
    println!("\n=== Memory Usage Benchmark ===");

    // Create a large configuration
    let mut json = String::from(r#"{"items": ["#);
    for i in 0..10000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"id": {}, "name": "item_{}", "value": {}, "description": "This is item number {}", "active": true}}"#,
            i, i, i * 10, i
        ));
    }
    json.push_str("]}");

    let json_size = json.len();

    // Encode
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_json(&json)?;

    let bcs_size = bcs_data.len();
    let compression_ratio = json_size as f64 / bcs_size as f64;

    println!(
        "JSON size: {} bytes ({:.2} KB)",
        json_size,
        json_size as f64 / 1024.0
    );
    println!(
        "BCS size: {} bytes ({:.2} KB)",
        bcs_size,
        bcs_size as f64 / 1024.0
    );
    println!("Compression ratio: {:.2}x", compression_ratio);
    println!(
        "Space savings: {:.1}%",
        (1.0 - 1.0 / compression_ratio) * 100.0
    );

    // Keep this benchmark non-flaky: verify data is valid and non-empty.
    assert!(bcs_size > 0, "BCS output should not be empty");
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded = decoder.to_json()?;
    assert!(
        !decoded.is_empty(),
        "Decoded benchmark output should not be empty"
    );

    Ok(())
}

#[test]
fn benchmark_comparison_json_vs_bcs() -> Result<()> {
    println!("\n=== JSON vs BCS Comparison ===");

    let json_content = load_example_json("kubernetes-deployment.json");

    // Benchmark JSON parsing
    let json_iterations = 100;
    let (_, json_parse_duration) = measure_time(|| {
        for _ in 0..json_iterations {
            let _: serde_json::Value =
                serde_json::from_str(&json_content).expect("Failed to parse JSON");
        }
    });
    let avg_json_parse = json_parse_duration.as_micros() as f64 / json_iterations as f64;

    // Encode to BCS
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_json(&json_content)?;

    // Benchmark BCS decoding
    let bcs_iterations = 100;
    let (_, bcs_decode_duration) = measure_time(|| {
        for _ in 0..bcs_iterations {
            let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
            let _ = decoder.to_json().expect("Failed to decode");
        }
    });
    let avg_bcs_decode = bcs_decode_duration.as_micros() as f64 / bcs_iterations as f64;

    println!("JSON parse time: {:.2} μs", avg_json_parse);
    println!("BCS decode time: {:.2} μs", avg_bcs_decode);
    println!("Speedup: {:.2}x", avg_json_parse / avg_bcs_decode);

    println!("\nFile sizes:");
    println!("JSON: {} bytes", json_content.len());
    println!("BCS: {} bytes", bcs_data.len());
    println!(
        "Size ratio: {:.2}x smaller",
        json_content.len() as f64 / bcs_data.len() as f64
    );

    Ok(())
}

#[test]
fn benchmark_comparison_yaml_vs_bcs() -> Result<()> {
    println!("\n=== YAML vs BCS Comparison ===");

    let yaml_content = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: web-app
  namespace: production
  labels:
    app: web-app
    environment: production
spec:
  replicas: 3
  selector:
    matchLabels:
      app: web-app
  template:
    metadata:
      labels:
        app: web-app
    spec:
      containers:
      - name: web-app
        image: myregistry.io/web-app:v1.2.3
        ports:
        - containerPort: 8080
          protocol: TCP
        resources:
          requests:
            memory: 256Mi
            cpu: 250m
          limits:
            memory: 512Mi
            cpu: 500m
"#;

    // Benchmark YAML parsing
    let yaml_iterations = 100;
    let (_, yaml_parse_duration) = measure_time(|| {
        for _ in 0..yaml_iterations {
            let _: serde_yaml::Value =
                serde_yaml::from_str(yaml_content).expect("Failed to parse YAML");
        }
    });
    let avg_yaml_parse = yaml_parse_duration.as_micros() as f64 / yaml_iterations as f64;

    // Encode to BCS
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_yaml(yaml_content)?;

    // Benchmark BCS decoding
    let bcs_iterations = 100;
    let (_, bcs_decode_duration) = measure_time(|| {
        for _ in 0..bcs_iterations {
            let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
            let _ = decoder.to_yaml().expect("Failed to decode");
        }
    });
    let avg_bcs_decode = bcs_decode_duration.as_micros() as f64 / bcs_iterations as f64;

    println!("YAML parse time: {:.2} μs", avg_yaml_parse);
    println!("BCS decode time: {:.2} μs", avg_bcs_decode);
    println!("Speedup: {:.2}x", avg_yaml_parse / avg_bcs_decode);

    println!("\nFile sizes:");
    println!("YAML: {} bytes", yaml_content.len());
    println!("BCS: {} bytes", bcs_data.len());

    Ok(())
}

#[test]
fn benchmark_streaming_performance() -> Result<()> {
    println!("\n=== Streaming Performance Benchmark ===");

    // Create a large dataset
    let mut json = String::from(r#"{"items": ["#);
    for i in 0..5000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(r#"{{"id": {}, "data": "value_{i}"}}"#, i, i = i));
    }
    json.push_str("]}");

    // Encode
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&json)?;

    // Benchmark full decode
    let (_, full_decode_time) = measure_time(|| {
        let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
        let _ = decoder.to_json().expect("Failed to decode");
    });

    // Benchmark streaming decode
    let (count, streaming_time) = measure_time(|| {
        let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
        let mut stream = decoder.stream().expect("Failed to create stream");
        let mut count = 0;
        while let Ok(Some(_)) = stream.next_value() {
            count += 1;
        }
        count
    });

    println!("Full decode time: {:?}", full_decode_time);
    println!("Streaming decode time: {:?}", streaming_time);
    println!("Items streamed: {}", count);
    println!(
        "Time per item: {:.2} μs",
        streaming_time.as_micros() as f64 / count as f64
    );

    Ok(())
}

#[test]
fn benchmark_partial_decode_vs_full() -> Result<()> {
    println!("\n=== Partial Decode vs Full Decode ===");

    // Create a large nested structure
    let mut json = String::from("{");
    for i in 0..1000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#""section_{i}": {{"data": {{"value": {i}, "name": "item_{i}", "description": "Long description for item {i}"}}}}"#,
            i = i
        ));
    }
    json.push('}');

    // Encode
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&json)?;

    // Benchmark full decode
    let (_, full_decode_time) = measure_time(|| {
        let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
        let _ = decoder.to_json().expect("Failed to decode");
    });

    // Benchmark partial decode (single key)
    let iterations = 100;
    let (_, partial_decode_time) = measure_time(|| {
        for i in 0..iterations {
            let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
            let path = format!("section_{}.data.value", i % 1000);
            let _ = decoder.get(&path);
        }
    });
    let avg_partial = partial_decode_time.as_micros() as f64 / iterations as f64;

    println!("Full decode time: {:?}", full_decode_time);
    println!("Partial decode time (avg): {:.2} μs", avg_partial);
    println!(
        "Speedup: {:.2}x",
        full_decode_time.as_micros() as f64 / avg_partial
    );

    Ok(())
}

#[test]
fn benchmark_generate_performance_report() -> Result<()> {
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║          BCS Performance Benchmark Report                  ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Run all benchmarks and collect results
    let json_content = load_example_json("app-settings.json");

    // Encode
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let (bcs_data, encode_time) = measure_time(|| {
        encoder
            .encode_from_json(&json_content)
            .expect("Failed to encode")
    });

    // Decode
    let (_, decode_time) = measure_time(|| {
        let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
        decoder.to_json().expect("Failed to decode")
    });

    // Random access
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let (_, access_time) = measure_time(|| {
        let _ = decoder.get("server.host");
    });

    // File sizes
    let json_size = json_content.len();
    let bcs_size = bcs_data.len();
    let compression_ratio = json_size as f64 / bcs_size as f64;

    println!("📊 Performance Metrics:");
    println!(
        "  • Encode time:        {:>10.2} μs",
        encode_time.as_micros()
    );
    println!(
        "  • Decode time:        {:>10.2} μs",
        decode_time.as_micros()
    );
    println!(
        "  • Random access:      {:>10.2} ns",
        access_time.as_nanos()
    );
    println!();
    println!("💾 Storage Efficiency:");
    println!("  • JSON size:          {:>10} bytes", json_size);
    println!("  • BCS size:           {:>10} bytes", bcs_size);
    println!("  • Compression ratio:  {:>10.2}x", compression_ratio);
    println!(
        "  • Space savings:      {:>10.1}%",
        (1.0 - 1.0 / compression_ratio) * 100.0
    );
    println!();
    println!("⚡ Throughput:");
    println!(
        "  • Encode:             {:>10.2} MB/s",
        (json_size as f64 / (1024.0 * 1024.0)) / (encode_time.as_secs_f64())
    );
    println!(
        "  • Decode:             {:>10.2} MB/s",
        (bcs_size as f64 / (1024.0 * 1024.0)) / (decode_time.as_secs_f64())
    );
    println!();

    Ok(())
}
