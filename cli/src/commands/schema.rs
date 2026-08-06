// Schema command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::Decoder;

pub fn run(file: &str, export: Option<&str>, agent_safe: bool) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }

    utils::print_info(&format!("Extracting schema from: {}", file));

    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;

    let schema = decoder
        .schema()
        .context("Failed to extract schema from BCS file")?;

    if agent_safe {
        let agent_json = schema
            .to_agent_safe_json()
            .context("Failed to build agent-safe schema")?;
        if let Some(export_path) = export {
            utils::write_file_string(export_path, &agent_json)
                .with_context(|| format!("Failed to write schema file: {}", export_path))?;
            utils::print_success(&format!("Agent-safe schema exported to: {}", export_path));
        } else {
            println!("{}", agent_json);
        }
        return Ok(());
    }

    if let Some(export_path) = export {
        utils::print_info(&format!("Exporting schema to: {}", export_path));

        let schema_json =
            serde_json::to_string_pretty(schema).context("Failed to serialize schema to JSON")?;

        utils::write_file_string(export_path, &schema_json)
            .with_context(|| format!("Failed to write schema file: {}", export_path))?;

        utils::print_success(&format!("Schema exported to: {}", export_path));
    } else {
        print_schema(schema)?;
    }

    Ok(())
}

/// Print schema in human-readable format
fn print_schema(schema: &bcs_core::Schema) -> Result<()> {
    println!("\nSchema Definition\n");

    println!("Version: {}", schema.version);
    println!("Root Type: {}\n", schema.root);

    let sensitive = schema.sensitive_path_list();
    if !sensitive.is_empty() {
        println!("Sensitive Paths ({}):", sensitive.len());
        println!("{}", "=".repeat(60));
        for path in &sensitive {
            println!("  - {}", path);
        }
        println!();
    }

    if !schema.types.is_empty() {
        println!("Type Definitions ({}):", schema.types.len());
        println!("{}", "=".repeat(60));

        for (name, type_def) in &schema.types {
            println!("\n{}", name);
            print_type_definition(type_def, 1);
        }

        println!();
    }

    if !schema.constraints.is_empty() {
        println!("\nConstraints ({}):", schema.constraints.len());
        println!("{}", "=".repeat(60));

        for (path, constraints) in &schema.constraints {
            println!("\n  Path: {}", path);
            for constraint in constraints {
                println!("    - {:?}", constraint);
            }
        }

        println!();
    }

    if !schema.documentation.is_empty() {
        println!("\nDocumentation ({} entries):", schema.documentation.len());
        println!("{}", "=".repeat(60));

        for (path, doc) in &schema.documentation {
            println!("\n  {}", path);
            println!("    {}", doc);
        }

        println!();
    }

    if !schema.ai_tags.is_empty() {
        println!("\nAI Semantic Tags ({}):", schema.ai_tags.len());
        println!("{}", "=".repeat(60));

        for (tag_name, tag) in &schema.ai_tags {
            println!("\n  {}", tag_name);
            println!("    Category:    {}", tag.category);
            if let Some(ref subcat) = tag.subcategory {
                println!("    Subcategory: {}", subcat);
            }
            println!("    Description: {}", tag.description);
            if let Some(ref sensitivity) = tag.sensitivity {
                println!("    Sensitivity: {}", sensitivity);
            }
            if let Some(ref embedding) = tag.embedding {
                println!("    Embedding:   {} dimensions", embedding.len());
            }
        }

        println!();
    }

    Ok(())
}

/// Print a type definition with indentation
fn print_type_definition(type_def: &bcs_core::schema::TypeDefinition, indent: usize) {
    use bcs_core::schema::TypeDefinition;

    let prefix = "  ".repeat(indent);

    match type_def {
        TypeDefinition::Int8 => println!("{}Type: int8", prefix),
        TypeDefinition::Int16 => println!("{}Type: int16", prefix),
        TypeDefinition::Int32 => println!("{}Type: int32", prefix),
        TypeDefinition::Int64 => println!("{}Type: int64", prefix),
        TypeDefinition::UInt8 => println!("{}Type: uint8", prefix),
        TypeDefinition::UInt16 => println!("{}Type: uint16", prefix),
        TypeDefinition::UInt32 => println!("{}Type: uint32", prefix),
        TypeDefinition::UInt64 => println!("{}Type: uint64", prefix),
        TypeDefinition::Float32 => println!("{}Type: float32", prefix),
        TypeDefinition::Float64 => println!("{}Type: float64", prefix),
        TypeDefinition::Bool => println!("{}Type: bool", prefix),
        TypeDefinition::String => println!("{}Type: string", prefix),
        TypeDefinition::Bytes => println!("{}Type: bytes", prefix),
        TypeDefinition::Null => println!("{}Type: null", prefix),

        TypeDefinition::List(inner) => {
            println!("{}Type: list", prefix);
            println!("{}Element Type:", prefix);
            print_type_definition(inner, indent + 1);
        }

        TypeDefinition::Map(key_type, value_type) => {
            println!("{}Type: map", prefix);
            println!("{}Key Type:", prefix);
            print_type_definition(key_type, indent + 1);
            println!("{}Value Type:", prefix);
            print_type_definition(value_type, indent + 1);
        }

        TypeDefinition::Struct(fields) => {
            println!("{}Type: struct", prefix);
            println!("{}Fields ({}):", prefix, fields.len());
            for (field_name, field_def) in fields {
                println!("{}  - {}", prefix, field_name);
                println!("{}    Required: {}", prefix, field_def.required);
                if let Some(ref doc) = field_def.documentation {
                    println!("{}    Doc: {}", prefix, doc);
                }
                if let Some(ref ai_tag) = field_def.ai_tag {
                    println!("{}    AI Tag: {}", prefix, ai_tag);
                }
                print_type_definition(&field_def.field_type, indent + 2);
                if !field_def.constraints.is_empty() {
                    println!("{}    Constraints:", prefix);
                    for constraint in &field_def.constraints {
                        println!("{}      - {:?}", prefix, constraint);
                    }
                }
            }
        }

        TypeDefinition::Union(variants) => {
            println!("{}Type: union", prefix);
            println!("{}Variants ({}):", prefix, variants.len());
            for variant in variants {
                println!("{}  - {} (tag: {})", prefix, variant.name, variant.tag);
                if let Some(ref doc) = variant.documentation {
                    println!("{}    Doc: {}", prefix, doc);
                }
                print_type_definition(&variant.value_type, indent + 2);
            }
        }

        TypeDefinition::Optional(inner) => {
            println!("{}Type: optional", prefix);
            println!("{}Inner Type:", prefix);
            print_type_definition(inner, indent + 1);
        }

        TypeDefinition::Custom(name) => {
            println!("{}Type: custom ({})", prefix, name);
        }
    }
}
