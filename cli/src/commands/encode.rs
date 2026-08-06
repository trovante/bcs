// Encode command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::security::KeyWrapper;
use bcs_core::{Decoder, Encoder, Schema};

#[allow(clippy::too_many_arguments)]
pub fn run(
    input: &str,
    output: Option<&str>,
    schema_file: Option<&str>,
    compact: bool,
    compress_data: bool,
    dedup: Option<&str>,
    dedup_min_repeats: usize,
    dedup_min_length: usize,
    index_maps_over: Option<usize>,
    protect_paths: Option<&str>,
    protect_paths_file: Option<&str>,
    sensitive_paths: Option<&str>,
    sensitive_paths_file: Option<&str>,
    protect_password: Option<&str>,
    protect_password_env: Option<&str>,
    protect_scheme: &str,
    kms_provider: Option<&str>,
    kms_key: Option<&str>,
) -> Result<()> {
    if !utils::file_exists(input) {
        anyhow::bail!("Input file not found: {}", input);
    }

    let output_path = match output {
        Some(path) => path.to_string(),
        None => default_output_path(input),
    };

    utils::print_info(&format!("Encoding {} to {}", input, output_path));

    let format = utils::get_extension(input)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine input format from file extension"))?;

    match format {
        "json" | "yaml" | "yml" | "toml" => {}
        _ => anyhow::bail!(
            "Unsupported input format: {}. Supported formats: json, yaml, yml, toml",
            format
        ),
    }

    let mut encoder = Encoder::new();

    if compact {
        utils::print_info("Compact mode enabled: omitting semantic layer and index table");
        encoder.set_compact_mode(true);
    }

    if compress_data {
        utils::print_info("Data compression enabled: compressing data layer with LZ4");
        encoder.set_data_compression(true);
    }

    if let Some(mode_raw) = dedup {
        let mode = bcs_core::DedupMode::parse(mode_raw)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        if mode.is_enabled() {
            utils::print_info(&format!(
                "Structural dedup enabled: {:?} (min_repeats={}, min_length={})",
                mode, dedup_min_repeats, dedup_min_length
            ));
            encoder.set_dedup(mode);
            encoder.set_dedup_thresholds(bcs_core::DedupThresholds {
                min_repeats: dedup_min_repeats.max(1),
                min_length: dedup_min_length,
            });
        }
    }

    if let Some(n) = index_maps_over {
        utils::print_info(&format!(
            "Local nested indexes for maps/structs with >= {} entries",
            n
        ));
        encoder.set_index_maps_over(Some(n));
    }

    let protect_path_list = collect_path_list(
        protect_paths,
        protect_paths_file,
        "--protect-paths",
        "--protect-paths-file",
    )?;
    let schema_only_sensitive = collect_path_list(
        sensitive_paths,
        sensitive_paths_file,
        "--sensitive-paths",
        "--sensitive-paths-file",
    )?;

    let mut stamp_sensitive: Vec<String> = protect_path_list
        .iter()
        .chain(schema_only_sensitive.iter())
        .cloned()
        .collect();
    stamp_sensitive.sort();
    stamp_sensitive.dedup();

    let scheme = protect_scheme.trim().to_ascii_lowercase();
    let kms_wrapper = if !protect_path_list.is_empty() && scheme == "kms" {
        let provider = kms_provider.ok_or_else(|| {
            anyhow::anyhow!(
                "--kms-provider is required with --protect-scheme kms (cmd, {})",
                bcs_secrets::available_kms_providers().join(", ")
            )
        })?;
        Some(crate::kms_wrapper::resolve_key_wrapper(provider)?)
    } else {
        None
    };
    let resolved_password = if protect_path_list.is_empty() || scheme == "kms" {
        None
    } else if let Some(password) = resolve_password(protect_password, protect_password_env)? {
        Some(password)
    } else {
        Some(utils::resolve_password_with_prompt(
            None,
            None,
            "Protect password: ",
            true,
        )?)
    };

    let mut authored_schema: Option<Schema> = None;
    if let Some(schema_path) = schema_file {
        if !utils::file_exists(schema_path) {
            anyhow::bail!("Schema file not found: {}", schema_path);
        }

        utils::print_info(&format!("Loading schema from: {}", schema_path));

        let schema_content = utils::read_file_string(schema_path)?;
        let mut schema: Schema = serde_json::from_str(&schema_content)
            .with_context(|| format!("Failed to parse schema file: {}", schema_path))?;
        if !stamp_sensitive.is_empty() {
            schema.mark_sensitive_paths(stamp_sensitive.iter().cloned());
        }
        authored_schema = Some(schema.clone());
        encoder = encoder.with_schema(schema);
        utils::print_info("Schema loaded successfully");
    }

    let input_content = utils::read_file_string(input)
        .with_context(|| format!("Failed to read input file: {}", input))?;

    utils::print_info(&format!(
        "Encoding from {} format...",
        format.to_uppercase()
    ));

    let mut bcs_data = match format {
        "json" => encoder.encode_from_json(&input_content),
        "yaml" | "yml" => encoder.encode_from_yaml(&input_content),
        "toml" => encoder.encode_from_toml(&input_content),
        _ => unreachable!(),
    }
    .with_context(|| format!("Failed to encode {} file", format))?;

    // Schema-only sensitive tags without encrypt: re-embed schema with stamps.
    if protect_path_list.is_empty() && !stamp_sensitive.is_empty() && !compact {
        bcs_data = stamp_sensitive_on_bcs(&bcs_data, &stamp_sensitive, authored_schema.as_ref())
            .context("Failed to stamp sensitive paths into schema")?;
    }

    if !protect_path_list.is_empty() {
        utils::print_info(&format!(
            "Protecting {} sensitive path(s) with scheme '{}'...",
            protect_path_list.len(),
            scheme
        ));
        bcs_data = protect_bcs_payload(
            &bcs_data,
            &protect_path_list,
            &stamp_sensitive,
            &scheme,
            resolved_password.as_deref(),
            kms_provider,
            kms_key,
            kms_wrapper.as_ref().map(|w| w.as_ref() as &dyn KeyWrapper),
            authored_schema.as_ref(),
        )
        .context("Failed to protect sensitive fields")?;
    }

    utils::write_file(&output_path, &bcs_data)
        .with_context(|| format!("Failed to write output file: {}", output_path))?;

    let input_size = input_content.len() as u64;
    let output_size = bcs_data.len() as u64;
    let compression_ratio = if input_size > 0 {
        (output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    };

    utils::print_success(&format!(
        "Successfully encoded to BCS format\n  Input size:  {}\n  Output size: {} ({:.1}% of original)",
        utils::format_size(input_size),
        utils::format_size(output_size),
        compression_ratio
    ));

    Ok(())
}

fn default_output_path(input: &str) -> String {
    use std::path::Path;

    let input_path = Path::new(input);

    if let Some(stem) = input_path.file_stem().and_then(|s| s.to_str()) {
        let mut output = input_path.to_path_buf();
        output.set_file_name(format!("{}.bcs", stem));
        output.to_string_lossy().to_string()
    } else {
        format!("{}.bcs", input)
    }
}

fn resolve_password(direct: Option<&str>, env_var: Option<&str>) -> Result<Option<String>> {
    if let Some(password) = direct {
        crate::utils::warn_password_on_argv("--protect-password", "--protect-password-env");
        return Ok(Some(password.to_string()));
    }

    if let Some(var_name) = env_var {
        let value = std::env::var(var_name)
            .with_context(|| format!("Failed to read password from env var: {}", var_name))?;
        if value.is_empty() {
            anyhow::bail!("Environment variable '{}' is empty", var_name);
        }

        return Ok(Some(value));
    }

    Ok(None)
}

fn collect_path_list(
    raw: Option<&str>,
    file: Option<&str>,
    flag_name: &str,
    file_flag_name: &str,
) -> Result<Vec<String>> {
    let mut paths: Vec<String> = Vec::new();

    if let Some(raw) = raw {
        paths.extend(parse_comma_paths(raw));
    }

    if let Some(file_path) = file {
        if !utils::file_exists(file_path) {
            anyhow::bail!("{} file not found: {}", file_flag_name, file_path);
        }

        let content = utils::read_file_string(file_path)
            .with_context(|| format!("Failed to read paths file: {}", file_path))?;

        paths.extend(parse_lines_paths(&content));
    }

    paths.sort();
    paths.dedup();

    if raw.is_some() && paths.is_empty() {
        anyhow::bail!("{} was provided but no valid paths were found", flag_name);
    }

    if file.is_some() && paths.is_empty() {
        anyhow::bail!(
            "{} was provided but no valid paths were found",
            file_flag_name
        );
    }

    Ok(paths)
}

fn parse_comma_paths(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_lines_paths(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .map(ToString::to_string)
        .collect()
}

fn stamp_sensitive_on_bcs(
    input: &[u8],
    stamp_paths: &[String],
    authored_schema: Option<&Schema>,
) -> Result<Vec<u8>> {
    let mut decoder =
        Decoder::from_bytes(input).context("Failed to decode BCS payload for schema stamp")?;
    let source_config = bcs_core::EncoderConfig::from_header(decoder.header());
    let value = decoder
        .decode_to_value()
        .context("Failed to decode BCS payload into value tree")?;
    // Prefer schema already embedded (matches current data types after protect/infer).
    let mut schema = decoder
        .schema()
        .context("Failed to load schema for sensitive stamp")?
        .clone();
    if let Some(authored) = authored_schema {
        for (path, doc) in &authored.documentation {
            schema.add_documentation(path.clone(), doc.clone());
        }
        for (path, constraints) in &authored.constraints {
            schema.add_constraints(path.clone(), constraints.clone());
        }
        for (name, tag) in &authored.ai_tags {
            schema.add_ai_tag(name.clone(), tag.clone());
        }
        schema.mark_sensitive_paths(authored.sensitive_path_list());
    }
    schema.mark_sensitive_paths(stamp_paths.iter().cloned());

    let json_value = bcs_core::convert::value_to_json(&value)
        .map_err(anyhow::Error::msg)
        .context("Failed to convert value tree")?;
    let json = serde_json::to_string(&json_value).context("Failed to serialize JSON")?;

    let mut encoder = Encoder::with_config(source_config).with_schema(schema);
    encoder
        .encode_from_json(&json)
        .context("Failed to re-encode BCS with sensitive schema")
}

#[allow(clippy::too_many_arguments)]
fn protect_bcs_payload(
    input: &[u8],
    protect_paths: &[String],
    stamp_paths: &[String],
    scheme: &str,
    password: Option<&str>,
    kms_provider: Option<&str>,
    kms_key: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    authored_schema: Option<&Schema>,
) -> Result<Vec<u8>> {
    let mut decoder = Decoder::from_bytes(input)
        .context("Failed to decode intermediate BCS payload for protection")?;
    let source_config = bcs_core::EncoderConfig::from_header(decoder.header());

    let mut value = decoder
        .decode_to_value()
        .context("Failed to decode BCS payload into value tree")?;

    match scheme {
        "pbkdf2" => {
            let password = password
                .ok_or_else(|| anyhow::anyhow!("Password required for pbkdf2 protect scheme"))?;
            bcs_core::security::protect_paths(&mut value, protect_paths, password)
                .map_err(anyhow::Error::msg)
                .context("Sensitive field protection failed")?;
        }
        "kms" => {
            let provider = kms_provider.ok_or_else(|| {
                anyhow::anyhow!("--kms-provider is required with --protect-scheme kms")
            })?;
            let key = kms_key.ok_or_else(|| {
                anyhow::anyhow!("--kms-key is required with --protect-scheme kms")
            })?;
            let wrapper = wrapper.ok_or_else(|| {
                anyhow::anyhow!("KMS command wrapper is required for kms protect scheme")
            })?;
            bcs_core::security::protect_paths_kms(&mut value, protect_paths, provider, key, wrapper)
                .map_err(anyhow::Error::msg)
                .context("Sensitive field KMS protection failed")?;
        }
        other => anyhow::bail!("Unsupported protect scheme '{}'. Use pbkdf2 or kms", other),
    }

    let json_value = bcs_core::convert::value_to_json(&value)
        .map_err(anyhow::Error::msg)
        .context("Failed to convert protected value tree")?;
    let json = serde_json::to_string(&json_value).context("Failed to serialize protected JSON")?;

    // Infer schema from protected markers (string leaves); skip authored validation mismatch.
    let mut encoder = Encoder::with_config(source_config);
    let protected = encoder
        .encode_from_json(&json)
        .context("Failed to encode protected BCS payload")?;

    stamp_sensitive_on_bcs(&protected, stamp_paths, authored_schema)
}
