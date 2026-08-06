// Reindex command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::{Decoder, Encoder, EncoderConfig};
use serde_json::json;

pub fn run(
    file: &str,
    output: Option<&str>,
    add_schema: bool,
    compress_data: bool,
    dry_run: bool,
    json_output: bool,
) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("Input BCS file not found: {}", file);
    }

    let output_path = if dry_run {
        "<dry-run>".to_string()
    } else {
        output
            .map(ToString::to_string)
            .unwrap_or_else(|| default_output_path(file))
    };

    if !json_output {
        utils::print_info(&format!("Reindexing {} -> {}", file, output_path));
    }

    let input_data =
        utils::read_file(file).with_context(|| format!("Failed to read BCS file: {}", file))?;
    let mut decoder = Decoder::from_bytes(&input_data)
        .with_context(|| format!("Failed to decode BCS file: {}", file))?;

    let mut config = EncoderConfig::from_header(decoder.header());
    // Reindex always rebuilds an index table; optionally add schema / compression.
    config.include_index_table = true;
    if add_schema {
        config.include_semantic_layer = true;
        config.compression = true;
    }
    if compress_data {
        config.data_compression = true;
    }

    let value = decoder
        .decode_to_value()
        .context("Failed to decode BCS payload")?;
    let json_value = bcs_core::convert::value_to_json(&value)
        .map_err(anyhow::Error::msg)
        .context("Failed to convert BCS value to JSON")?;
    let json = serde_json::to_string(&json_value)
        .context("Failed to serialize normalized JSON payload")?;

    let mut encoder = Encoder::with_config(config);

    if compress_data && !json_output {
        utils::print_info("Data compression enabled for reindexed output");
    }

    if add_schema && !json_output {
        utils::print_info("Semantic layer embedding enabled for reindexed output");
    }

    let output_data = encoder
        .encode_from_json(&json)
        .context("Failed to encode reindexed BCS payload")?;

    let input_size = input_data.len() as u64;
    let output_size = output_data.len() as u64;
    let ratio = if input_size > 0 {
        (output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    };

    let input_decoder =
        Decoder::from_bytes(&input_data).context("Failed to decode input BCS metadata")?;
    let input_meta = input_decoder.metadata();

    let output_decoder = Decoder::from_bytes(&output_data)
        .context("Failed to decode projected output BCS metadata")?;
    let output_meta = output_decoder.metadata();

    if !json_output {
        print_section_summary("Input", &input_meta);
        print_section_summary("Projected Output", &output_meta);
    }

    if dry_run {
        if json_output {
            let payload = json!({
                "ok": true,
                "file": file,
                "output": serde_json::Value::Null,
                "dry_run": true,
                "input_size": input_size,
                "projected_output_size": output_size,
                "ratio_percent": ratio,
                "input_sections": {
                    "semantic_size": input_meta.semantic_size,
                    "index_size": input_meta.index_size,
                    "data_size": input_meta.data_size,
                    "total_size": input_meta.total_size
                },
                "projected_output_sections": {
                    "semantic_size": output_meta.semantic_size,
                    "index_size": output_meta.index_size,
                    "data_size": output_meta.data_size,
                    "total_size": output_meta.total_size
                },
                "options": {
                    "add_schema": add_schema,
                    "compress_data": compress_data
                }
            });

            println!(
                "{}",
                serde_json::to_string_pretty(&payload)
                    .context("Failed to serialize reindex JSON")?
            );
        } else {
            utils::print_info("Dry-run enabled: output file was not written");
        }
        return Ok(());
    }

    utils::write_file(&output_path, &output_data)
        .with_context(|| format!("Failed to write output file: {}", output_path))?;

    if json_output {
        let payload = json!({
            "ok": true,
            "file": file,
            "output": output_path,
            "dry_run": false,
            "input_size": input_size,
            "output_size": output_size,
            "ratio_percent": ratio,
            "input_sections": {
                "semantic_size": input_meta.semantic_size,
                "index_size": input_meta.index_size,
                "data_size": input_meta.data_size,
                "total_size": input_meta.total_size
            },
            "output_sections": {
                "semantic_size": output_meta.semantic_size,
                "index_size": output_meta.index_size,
                "data_size": output_meta.data_size,
                "total_size": output_meta.total_size
            },
            "options": {
                "add_schema": add_schema,
                "compress_data": compress_data
            }
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("Failed to serialize reindex JSON")?
        );
    } else {
        utils::print_success(&format!(
            "Reindex completed\n  Input size:  {}\n  Output size: {} ({:.1}% of input)",
            utils::format_size(input_size),
            utils::format_size(output_size),
            ratio
        ));
    }

    Ok(())
}

fn default_output_path(input: &str) -> String {
    use std::path::Path;

    let input_path = Path::new(input);

    if let Some(stem) = input_path.file_stem().and_then(|s| s.to_str()) {
        let mut output = input_path.to_path_buf();
        output.set_file_name(format!("{}.reindexed.bcs", stem));
        output.to_string_lossy().to_string()
    } else {
        format!("{}.reindexed.bcs", input)
    }
}

fn print_section_summary(label: &str, meta: &bcs_core::decoder::FileMetadata) {
    println!("{} sections:", label);
    println!("  Semantic: {}", utils::format_size(meta.semantic_size));
    println!("  Index:    {}", utils::format_size(meta.index_size));
    println!("  Data:     {}", utils::format_size(meta.data_size));
    println!("  Total:    {}", utils::format_size(meta.total_size));
}
