// Benchmark command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::Decoder;
use serde_json::json;
use std::time::Instant;

pub fn run(
    file: &str,
    compare: Option<&str>,
    runs: usize,
    path_hot_only: bool,
    json_output: bool,
) -> Result<()> {
    // Check if file exists
    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }

    if !json_output {
        println!("\n⚡ Benchmarking BCS file: {}\n", file);
    }

    let normalized_runs = runs.max(1);

    // Run BCS benchmarks with multiple samples for stability
    let bcs_results = benchmark_bcs(file, normalized_runs, !json_output, path_hot_only)?;

    if !json_output {
        print_benchmark_results("BCS", &bcs_results);
    }

    // If comparison file is provided, benchmark it too
    let mut compare_payload = None;
    if let Some(compare_file) = compare {
        if !utils::file_exists(compare_file) {
            anyhow::bail!("Comparison file not found: {}", compare_file);
        }

        if !json_output {
            println!("\n📊 Comparing with: {}\n", compare_file);
        }

        let compare_results =
            benchmark_comparison(compare_file, normalized_runs, !json_output, path_hot_only)?;
        if !json_output {
            print_benchmark_results("Comparison", &compare_results);
        }

        if !json_output {
            print_comparison_table(&bcs_results, &compare_results);
        }

        compare_payload = Some(json!({
            "file": compare_file,
            "results": benchmark_results_to_json(&compare_results),
            "comparison": comparison_to_json(&bcs_results, &compare_results)
        }));
    }

    if json_output {
        let payload = json!({
            "file": file,
            "runs": normalized_runs,
            "mode": if path_hot_only { "path-hot" } else { "full" },
            "bcs": benchmark_results_to_json(&bcs_results),
            "compare": compare_payload
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("Failed to serialize benchmark JSON")?
        );
    }

    Ok(())
}

#[derive(Debug)]
struct BenchmarkResults {
    file_size: u64,
    load_time_p50: u128,
    load_time_p95: u128,
    full_decode_time_p50: u128,
    full_decode_time_p95: u128,
    random_access_avg: u128,
    random_access_samples: usize,
    path_get_simple_p95: u128,
    path_get_simple_samples: usize,
    path_get_deep_p95: u128,
    path_get_deep_samples: usize,
    path_get_wildcard_p95: u128,
    path_get_wildcard_samples: usize,
    path_get_hot_p95: u128,
    path_get_hot_samples: usize,
    memory_usage: usize,
    runs: usize,
}

/// Benchmark BCS file operations
fn benchmark_bcs(
    file: &str,
    runs: usize,
    emit_logs: bool,
    path_hot_only: bool,
) -> Result<BenchmarkResults> {
    // Get file size
    let file_size = std::fs::metadata(file)
        .context("Failed to get file metadata")?
        .len();

    // Benchmark file loading and full decode with multiple runs
    if emit_logs {
        utils::print_info(&format!("Benchmarking file loading ({} runs)...", runs));
    }
    let mut load_times = Vec::with_capacity(runs);
    let mut decode_times = Vec::with_capacity(runs);

    if !path_hot_only {
        for _ in 0..runs {
            let start = Instant::now();
            let mut decoder = Decoder::from_file(file).context("Failed to load BCS file")?;
            let load_time = start.elapsed().as_nanos();
            load_times.push(load_time);

            let start = Instant::now();
            let _json = decoder.to_json().context("Failed to decode to JSON")?;
            let decode_time = start.elapsed().as_nanos();
            decode_times.push(decode_time);
        }
    }

    let load_time_p50 = percentile_ns(&load_times, 50.0);
    let load_time_p95 = percentile_ns(&load_times, 95.0);
    let full_decode_time_p50 = percentile_ns(&decode_times, 50.0);
    let full_decode_time_p95 = percentile_ns(&decode_times, 95.0);

    let mut decoder = Decoder::from_file(file).context("Failed to load BCS file")?;

    // Benchmark random access (if index table is available)
    let (random_access_avg, samples) = if path_hot_only {
        (0, 0)
    } else {
        if emit_logs {
            utils::print_info("Benchmarking random access...");
        }
        benchmark_random_access(&mut decoder)?
    };

    if emit_logs {
        utils::print_info("Benchmarking path queries...");
    }
    let path_stats = benchmark_path_queries(file, runs)?;

    // Estimate memory usage (rough approximation)
    let memory_usage = file_size as usize;

    Ok(BenchmarkResults {
        file_size,
        load_time_p50,
        load_time_p95,
        full_decode_time_p50,
        full_decode_time_p95,
        random_access_avg,
        random_access_samples: samples,
        path_get_simple_p95: path_stats.simple_p95,
        path_get_simple_samples: path_stats.simple_samples,
        path_get_deep_p95: path_stats.deep_p95,
        path_get_deep_samples: path_stats.deep_samples,
        path_get_wildcard_p95: path_stats.wildcard_p95,
        path_get_wildcard_samples: path_stats.wildcard_samples,
        path_get_hot_p95: path_stats.hot_p95,
        path_get_hot_samples: path_stats.hot_samples,
        memory_usage,
        runs,
    })
}

struct PathQueryBenchStats {
    simple_p95: u128,
    simple_samples: usize,
    deep_p95: u128,
    deep_samples: usize,
    wildcard_p95: u128,
    wildcard_samples: usize,
    hot_p95: u128,
    hot_samples: usize,
}

fn benchmark_path_queries(file: &str, runs: usize) -> Result<PathQueryBenchStats> {
    let mut decoder =
        Decoder::from_file(file).context("Failed to load BCS file for path benchmark")?;

    let simple_paths = ["app", "database", "services"];
    let deep_paths = [
        "app.name",
        "database.host",
        "services[0].name",
        "services[0].routes[0].paths[0]",
    ];
    let wildcard_paths = ["services.$.name", "services.$.routes.$.paths"];
    let hot_candidates = [
        "services[0].routes[0].paths[0]",
        "database.host",
        "app.name",
    ];
    const HOT_LOOP_ITERS: usize = 50;

    let mut simple_times = Vec::new();
    let mut deep_times = Vec::new();
    let mut wildcard_times = Vec::new();
    let mut hot_times = Vec::new();

    // Warmup to reduce first-hit cache and parser effects.
    for path in simple_paths {
        let _ = decoder.get(path);
    }
    for path in deep_paths {
        let _ = decoder.get(path);
    }
    for path in wildcard_paths {
        let _ = decoder.get(path);
    }

    let hot_path = hot_candidates
        .iter()
        .find(|path| decoder.get(path).is_ok())
        .copied();

    for _ in 0..runs {
        for path in simple_paths {
            let start = Instant::now();
            if decoder.get(path).is_ok() {
                simple_times.push(start.elapsed().as_nanos());
            }
        }

        for path in deep_paths {
            let start = Instant::now();
            if decoder.get(path).is_ok() {
                deep_times.push(start.elapsed().as_nanos());
            }
        }

        for path in wildcard_paths {
            let start = Instant::now();
            if decoder.get(path).is_ok() {
                wildcard_times.push(start.elapsed().as_nanos());
            }
        }

        if let Some(path) = hot_path {
            for _ in 0..HOT_LOOP_ITERS {
                let start = Instant::now();
                if decoder.get(path).is_ok() {
                    hot_times.push(start.elapsed().as_nanos());
                }
            }
        }
    }

    let simple_p95 = percentile_ns(&simple_times, 95.0);
    let deep_p95 = percentile_ns(&deep_times, 95.0);
    let wildcard_p95 = percentile_ns(&wildcard_times, 95.0);
    let hot_p95 = percentile_ns(&hot_times, 95.0);

    Ok(PathQueryBenchStats {
        simple_p95,
        simple_samples: simple_times.len(),
        deep_p95,
        deep_samples: deep_times.len(),
        wildcard_p95,
        wildcard_samples: wildcard_times.len(),
        hot_p95,
        hot_samples: hot_times.len(),
    })
}

/// Benchmark random access operations
fn benchmark_random_access(decoder: &mut Decoder) -> Result<(u128, usize)> {
    // Collect real indexed field names before timing lookups.
    let field_names: Vec<String> = match decoder.index_table() {
        Ok(index_table) => {
            if index_table.entry_count() == 0 {
                return Ok((0, 0));
            }
            index_table
                .buckets
                .iter()
                .filter(|b| !b.is_empty())
                .filter_map(|b| b.field_name.clone())
                .collect()
        }
        Err(_) => return Ok((0, 0)),
    };

    if field_names.is_empty() {
        return Ok((0, 0));
    }

    let sample_count = std::cmp::min(1000, field_names.len());
    let mut total_time = 0u128;
    let mut successful_lookups = 0;

    let index_table = decoder
        .index_table()
        .context("Failed to load index table for random access benchmark")?;

    for name in field_names.iter().take(sample_count) {
        let start = Instant::now();
        let hit = index_table.lookup_key_exact(name).is_some();
        let elapsed = start.elapsed().as_nanos();
        total_time += elapsed;
        if hit {
            successful_lookups += 1;
        }
    }

    let avg_time = if successful_lookups > 0 {
        total_time / successful_lookups as u128
    } else {
        0
    };

    Ok((avg_time, successful_lookups))
}

/// Benchmark comparison file (JSON/YAML/TOML)
fn benchmark_comparison(
    file: &str,
    runs: usize,
    emit_logs: bool,
    path_hot_only: bool,
) -> Result<BenchmarkResults> {
    // Get file size
    let file_size = std::fs::metadata(file)
        .context("Failed to get file metadata")?
        .len();

    // Detect format
    let format = utils::get_extension(file)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine file format"))?;

    if emit_logs {
        utils::print_info(&format!(
            "Benchmarking {} loading/parsing ({} runs)...",
            format.to_uppercase(),
            runs
        ));
    }

    let mut load_times = Vec::with_capacity(runs);
    let mut decode_times = Vec::with_capacity(runs);

    if !path_hot_only {
        for _ in 0..runs {
            let start = Instant::now();
            let content = utils::read_file_string(file)?;
            let load_time = start.elapsed().as_nanos();
            load_times.push(load_time);

            let start = Instant::now();
            match format {
                "json" => {
                    let _: serde_json::Value =
                        serde_json::from_str(&content).context("Failed to parse JSON")?;
                }
                "yaml" | "yml" => {
                    let _: serde_yaml::Value =
                        serde_yaml::from_str(&content).context("Failed to parse YAML")?;
                }
                "toml" => {
                    let _: toml::Value =
                        toml::from_str(&content).context("Failed to parse TOML")?;
                }
                _ => anyhow::bail!("Unsupported format: {}", format),
            }
            let parse_time = start.elapsed().as_nanos();
            decode_times.push(load_time + parse_time);
        }
    }

    let load_time_p50 = percentile_ns(&load_times, 50.0);
    let load_time_p95 = percentile_ns(&load_times, 95.0);
    let full_decode_time_p50 = percentile_ns(&decode_times, 50.0);
    let full_decode_time_p95 = percentile_ns(&decode_times, 95.0);

    // Random access is not applicable for text formats
    let random_access_avg = 0;
    let random_access_samples = 0;

    // Memory usage is approximately the file size plus parsed structure
    let memory_usage = (file_size as usize) * 2; // Rough estimate

    Ok(BenchmarkResults {
        file_size,
        load_time_p50,
        load_time_p95,
        full_decode_time_p50,
        full_decode_time_p95,
        random_access_avg,
        random_access_samples,
        path_get_simple_p95: 0,
        path_get_simple_samples: 0,
        path_get_deep_p95: 0,
        path_get_deep_samples: 0,
        path_get_wildcard_p95: 0,
        path_get_wildcard_samples: 0,
        path_get_hot_p95: 0,
        path_get_hot_samples: 0,
        memory_usage,
        runs,
    })
}

/// Print benchmark results
fn print_benchmark_results(label: &str, results: &BenchmarkResults) {
    println!("{} Results:", label);
    println!("{}", "=".repeat(60));
    println!(
        "  File Size:        {}",
        utils::format_size(results.file_size)
    );
    println!("  Runs:             {}", results.runs);
    println!(
        "  Load Time (p50):  {}",
        utils::format_duration(results.load_time_p50)
    );
    println!(
        "  Load Time (p95):  {}",
        utils::format_duration(results.load_time_p95)
    );
    println!(
        "  Decode Time (p50): {}",
        utils::format_duration(results.full_decode_time_p50)
    );
    println!(
        "  Decode Time (p95): {}",
        utils::format_duration(results.full_decode_time_p95)
    );

    if results.random_access_samples > 0 {
        println!(
            "  Random Access:    {} ({} samples)",
            utils::format_duration(results.random_access_avg),
            results.random_access_samples
        );
    } else {
        println!("  Random Access:    N/A");
    }

    if results.path_get_simple_samples > 0 {
        println!(
            "  Path Get Simple (p95): {} ({} samples)",
            utils::format_duration(results.path_get_simple_p95),
            results.path_get_simple_samples
        );
    } else {
        println!("  Path Get Simple (p95): N/A");
    }

    if results.path_get_deep_samples > 0 {
        println!(
            "  Path Get Deep (p95):   {} ({} samples)",
            utils::format_duration(results.path_get_deep_p95),
            results.path_get_deep_samples
        );
    } else {
        println!("  Path Get Deep (p95):   N/A");
    }

    if results.path_get_wildcard_samples > 0 {
        println!(
            "  Path Get Wildcard (p95): {} ({} samples)",
            utils::format_duration(results.path_get_wildcard_p95),
            results.path_get_wildcard_samples
        );
    } else {
        println!("  Path Get Wildcard (p95): N/A");
    }

    if results.path_get_hot_samples > 0 {
        println!(
            "  Path Get Hot-loop (p95): {} ({} samples)",
            utils::format_duration(results.path_get_hot_p95),
            results.path_get_hot_samples
        );
    } else {
        println!("  Path Get Hot-loop (p95): N/A");
    }

    println!(
        "  Memory Usage:     ~{}",
        utils::format_size(results.memory_usage as u64)
    );
    println!();
}

/// Print comparison table
fn print_comparison_table(bcs: &BenchmarkResults, other: &BenchmarkResults) {
    println!("\n📈 Performance Comparison");
    println!("{}", "=".repeat(80));

    // Calculate speedups
    let load_speedup = if bcs.load_time_p50 > 0 {
        other.load_time_p50 as f64 / bcs.load_time_p50 as f64
    } else {
        0.0
    };

    let decode_speedup = if bcs.full_decode_time_p50 > 0 {
        other.full_decode_time_p50 as f64 / bcs.full_decode_time_p50 as f64
    } else {
        0.0
    };

    let size_ratio = if bcs.file_size > 0 {
        (bcs.file_size as f64 / other.file_size as f64) * 100.0
    } else {
        0.0
    };

    let memory_ratio = if bcs.memory_usage > 0 {
        (bcs.memory_usage as f64 / other.memory_usage as f64) * 100.0
    } else {
        0.0
    };

    // Print table
    let col_widths = [30, 20, 20, 15];

    println!(
        "{}",
        utils::format_table_row(&[
            ("Metric", col_widths[0]),
            ("BCS", col_widths[1]),
            ("Comparison", col_widths[2]),
            ("Speedup", col_widths[3]),
        ])
    );

    utils::print_table_separator(&col_widths);

    println!(
        "{}",
        utils::format_table_row(&[
            ("File Size", col_widths[0]),
            (&utils::format_size(bcs.file_size), col_widths[1]),
            (&utils::format_size(other.file_size), col_widths[2]),
            (&format!("{:.1}%", size_ratio), col_widths[3]),
        ])
    );

    println!(
        "{}",
        utils::format_table_row(&[
            ("Load Time (p50)", col_widths[0]),
            (&utils::format_duration(bcs.load_time_p50), col_widths[1]),
            (&utils::format_duration(other.load_time_p50), col_widths[2]),
            (&format!("{:.2}x", load_speedup), col_widths[3]),
        ])
    );

    println!(
        "{}",
        utils::format_table_row(&[
            ("Decode Time (p50)", col_widths[0]),
            (
                &utils::format_duration(bcs.full_decode_time_p50),
                col_widths[1]
            ),
            (
                &utils::format_duration(other.full_decode_time_p50),
                col_widths[2]
            ),
            (&format!("{:.2}x", decode_speedup), col_widths[3]),
        ])
    );

    if bcs.random_access_samples > 0 {
        println!(
            "{}",
            utils::format_table_row(&[
                ("Random Access (avg)", col_widths[0]),
                (
                    &utils::format_duration(bcs.random_access_avg),
                    col_widths[1]
                ),
                ("N/A", col_widths[2]),
                ("-", col_widths[3]),
            ])
        );
    }

    println!(
        "{}",
        utils::format_table_row(&[
            ("Memory Usage", col_widths[0]),
            (&utils::format_size(bcs.memory_usage as u64), col_widths[1]),
            (
                &utils::format_size(other.memory_usage as u64),
                col_widths[2]
            ),
            (&format!("{:.1}%", memory_ratio), col_widths[3]),
        ])
    );

    println!();

    // Print summary
    if decode_speedup > 1.0 {
        utils::print_success(&format!(
            "BCS is {:.2}x faster for full decoding!",
            decode_speedup
        ));
    } else if decode_speedup < 1.0 && decode_speedup > 0.0 {
        utils::print_warning(&format!(
            "Comparison format is {:.2}x faster for full decoding",
            1.0 / decode_speedup
        ));
    }

    if size_ratio < 100.0 {
        utils::print_success(&format!("BCS file is {:.1}% smaller!", 100.0 - size_ratio));
    }
}

fn percentile_ns(values: &[u128], percentile: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();

    let rank = ((percentile / 100.0) * ((sorted.len() - 1) as f64)).round() as usize;
    sorted[rank]
}

fn benchmark_results_to_json(results: &BenchmarkResults) -> serde_json::Value {
    json!({
        "file_size": results.file_size,
        "load_time_p50_ns": results.load_time_p50,
        "load_time_p95_ns": results.load_time_p95,
        "decode_time_p50_ns": results.full_decode_time_p50,
        "decode_time_p95_ns": results.full_decode_time_p95,
        "random_access_avg_ns": results.random_access_avg,
        "random_access_samples": results.random_access_samples,
        "path_get_simple_p95_ns": results.path_get_simple_p95,
        "path_get_simple_samples": results.path_get_simple_samples,
        "path_get_deep_p95_ns": results.path_get_deep_p95,
        "path_get_deep_samples": results.path_get_deep_samples,
        "path_get_wildcard_p95_ns": results.path_get_wildcard_p95,
        "path_get_wildcard_samples": results.path_get_wildcard_samples,
        "path_get_hot_p95_ns": results.path_get_hot_p95,
        "path_get_hot_samples": results.path_get_hot_samples,
        "memory_usage_bytes": results.memory_usage,
        "runs": results.runs
    })
}

fn comparison_to_json(bcs: &BenchmarkResults, other: &BenchmarkResults) -> serde_json::Value {
    let load_speedup = if bcs.load_time_p50 > 0 {
        other.load_time_p50 as f64 / bcs.load_time_p50 as f64
    } else {
        0.0
    };

    let decode_speedup = if bcs.full_decode_time_p50 > 0 {
        other.full_decode_time_p50 as f64 / bcs.full_decode_time_p50 as f64
    } else {
        0.0
    };

    let size_ratio_percent = if other.file_size > 0 {
        (bcs.file_size as f64 / other.file_size as f64) * 100.0
    } else {
        0.0
    };

    json!({
        "load_speedup_x": load_speedup,
        "decode_speedup_x": decode_speedup,
        "size_ratio_percent": size_ratio_percent
    })
}
