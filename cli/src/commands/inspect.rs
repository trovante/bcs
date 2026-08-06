// Inspect command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::Decoder;
use serde_json::json;

pub fn run(file: &str, verbose: bool, json_output: bool, tree: bool) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }

    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;

    if tree {
        let root = bcs_core::InspectNode::from_decoder(&mut decoder)
            .context("Failed to build inspect AST")?;
        print!(
            "{}",
            root.format_tree()
                .context("Failed to format inspect tree")?
        );
        return Ok(());
    }

    let metadata = decoder.metadata();
    let header = decoder.header().clone();

    let schema_result = decoder.schema().cloned();
    let index_result = decoder.index_table().map(|idx| {
        json!({
            "entry_count": idx.entry_count(),
            "bucket_count": idx.bucket_count(),
            "load_factor": idx.load_factor(),
            "collision_rate": idx.collision_rate()
        })
    });

    if json_output {
        let payload = json!({
            "file": file,
            "metadata": {
                "version_major": metadata.version_major,
                "version_minor": metadata.version_minor,
                "compressed": metadata.compressed,
                "data_compressed": header.flags.data_compressed,
                "ai_metadata": metadata.ai_metadata,
                "semantic_size": metadata.semantic_size,
                "index_size": metadata.index_size,
                "data_size": metadata.data_size,
                "total_size": metadata.total_size
            },
            "header": {
                "magic": header.magic,
                "flags": header.flags.to_u16(),
                "checksum": header.checksum,
                "semantic_offset": header.semantic_offset,
                "index_offset": header.index_offset,
                "data_offset": header.data_offset
            },
            "schema": match schema_result {
                Ok(schema) => {
                    let sensitive = schema.sensitive_path_list();
                    json!({
                    "ok": true,
                    "version": schema.version,
                    "root": schema.root,
                    "type_count": schema.types.len(),
                    "constraint_count": schema.constraints.len(),
                    "documentation_count": schema.documentation.len(),
                    "ai_tag_count": schema.ai_tags.len(),
                    "sensitive_count": sensitive.len(),
                    "sensitive_paths": sensitive,
                    "types": if verbose {
                        serde_json::to_value(&schema.types).unwrap_or(serde_json::Value::Null)
                    } else {
                        json!(null)
                    }
                })
                },
                Err(e) => json!({
                    "ok": false,
                    "error": e.to_string()
                })
            },
            "index_table": match index_result {
                Ok(index) => json!({"ok": true, "stats": index}),
                Err(e) => json!({"ok": false, "error": e.to_string()})
            }
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("Failed to serialize inspect JSON")?
        );
        return Ok(());
    }

    println!("\n📋 Inspecting BCS file: {}\n", file);

    println!("File Information:");
    println!(
        "  Version:      {}.{}",
        metadata.version_major, metadata.version_minor
    );
    println!(
        "  Total Size:   {}",
        utils::format_size(metadata.total_size)
    );
    println!(
        "  Compressed:   {}",
        if metadata.compressed { "Yes" } else { "No" }
    );
    println!(
        "  Data Compressed: {}",
        if header.flags.data_compressed {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "  Reserved 0x0002: {}",
        if metadata.ai_metadata {
            "set (ignored)"
        } else {
            "clear"
        }
    );
    println!();

    println!("Layer Sizes:");
    println!(
        "  Semantic:     {} ({:.1}%)",
        utils::format_size(metadata.semantic_size),
        (metadata.semantic_size as f64 / metadata.total_size as f64) * 100.0
    );
    println!(
        "  Index Table:  {} ({:.1}%)",
        utils::format_size(metadata.index_size),
        (metadata.index_size as f64 / metadata.total_size as f64) * 100.0
    );
    println!(
        "  Data Layer:   {} ({:.1}%)",
        utils::format_size(metadata.data_size),
        (metadata.data_size as f64 / metadata.total_size as f64) * 100.0
    );
    println!();

    if verbose {
        println!("Header Details:");
        println!("  Magic Number: 0x{:08X}", header.magic);
        println!("  Flags:        0x{:04X}", header.flags.to_u16());
        println!("  Checksum:     0x{:016X}", header.checksum);
        println!();

        println!("Offsets:");
        println!(
            "  Semantic:     0x{:08X} ({})",
            header.semantic_offset, header.semantic_offset
        );
        println!(
            "  Index Table:  0x{:08X} ({})",
            header.index_offset, header.index_offset
        );
        println!(
            "  Data Layer:   0x{:08X} ({})",
            header.data_offset, header.data_offset
        );
        println!();
    }

    match schema_result {
        Ok(schema) => {
            println!("Schema Information:");
            println!("  Version:      {}", schema.version);
            println!("  Root Type:    {}", schema.root);
            println!("  Type Count:   {}", schema.types.len());

            if !schema.constraints.is_empty() {
                println!("  Constraints:  {}", schema.constraints.len());
            }

            if !schema.documentation.is_empty() {
                println!("  Documentation: {} entries", schema.documentation.len());
            }

            if !schema.ai_tags.is_empty() {
                println!("  AI Tags:      {}", schema.ai_tags.len());
            }

            let sensitive = schema.sensitive_path_list();
            if !sensitive.is_empty() {
                println!("  Sensitive:    {} path(s)", sensitive.len());
                if verbose {
                    for path in &sensitive {
                        println!("    - {}", path);
                    }
                }
            }

            println!();

            if verbose {
                println!("Type Definitions:");
                for (name, type_def) in &schema.types {
                    println!("  - {}: {:?}", name, type_def);
                }
                println!();
            }
        }
        Err(e) => {
            utils::print_warning(&format!("Failed to load schema: {}", e));
        }
    }

    match decoder.index_table() {
        Ok(index_table) => {
            println!("Index Table:");
            println!("  Entry Count:  {}", index_table.entry_count());
            println!("  Bucket Count: {}", index_table.bucket_count());
            println!("  Load Factor:  {:.2}", index_table.load_factor());

            if verbose {
                println!(
                    "  Collision Rate: {:.1}%",
                    index_table.collision_rate() * 100.0
                );
            }

            println!();
        }
        Err(e) => {
            utils::print_warning(&format!("Failed to load index table: {}", e));
        }
    }

    utils::print_success("Inspection completed");

    Ok(())
}
