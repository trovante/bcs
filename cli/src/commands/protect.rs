// Protect command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::{Decoder, Encoder, EncoderConfig};
use serde_json::json;

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: &str,
    output: Option<&str>,
    paths_raw: Option<&str>,
    paths_file: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
    scheme: &str,
    kms_provider: Option<&str>,
    kms_key: Option<&str>,
    json_output: bool,
) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("BCS file not found: {}", file);
    }

    let paths = collect_paths(paths_raw, paths_file)?;
    if paths.is_empty() {
        anyhow::bail!("No valid sensitive paths were provided");
    }

    let scheme = scheme.trim().to_ascii_lowercase();
    let output_path = output
        .map(ToString::to_string)
        .unwrap_or_else(|| default_output_path(file));

    if !json_output {
        utils::print_info(&format!(
            "Protecting {} sensitive path(s) in {} -> {} (scheme={})",
            paths.len(),
            file,
            output_path,
            scheme
        ));
    }

    let input_data =
        utils::read_file(file).with_context(|| format!("Failed to read BCS file: {}", file))?;
    let mut decoder = Decoder::from_bytes(&input_data)
        .with_context(|| format!("Failed to decode BCS file: {}", file))?;
    let source_config = EncoderConfig::from_header(decoder.header());
    let prior_schema = decoder.schema().ok().cloned();
    let mut value = decoder
        .decode_to_value()
        .context("Failed to decode BCS payload into value tree")?;

    match scheme.as_str() {
        "pbkdf2" => {
            let resolved_password = resolve_password(password, password_env)?;
            bcs_core::security::protect_paths(&mut value, &paths, &resolved_password)
                .map_err(anyhow::Error::msg)
                .context("Failed to protect sensitive paths")?;
        }
        "kms" => {
            let provider = kms_provider
                .ok_or_else(|| anyhow::anyhow!("--kms-provider is required with --scheme kms"))?;
            let key = kms_key
                .ok_or_else(|| anyhow::anyhow!("--kms-key is required with --scheme kms"))?;
            let wrapper = crate::kms_wrapper::resolve_key_wrapper(provider)?;
            bcs_core::security::protect_paths_kms(
                &mut value,
                &paths,
                provider,
                key,
                wrapper.as_ref(),
            )
            .map_err(anyhow::Error::msg)
            .context("Failed to protect sensitive paths with KMS")?;
        }
        other => anyhow::bail!("Unsupported scheme '{}'. Use pbkdf2 or kms", other),
    }

    let json_value = bcs_core::convert::value_to_json(&value)
        .map_err(anyhow::Error::msg)
        .context("Failed to convert protected value tree")?;
    let json = serde_json::to_string(&json_value).context("Failed to serialize protected JSON")?;

    // Infer from protected markers, then stamp sensitive paths (+ preserve prior metadata).
    let mut encoder = Encoder::with_config(source_config);
    let intermediate = encoder
        .encode_from_json(&json)
        .context("Failed to encode protected BCS payload")?;

    let mut decoder2 = Decoder::from_bytes(&intermediate)
        .context("Failed to decode protected intermediate for schema stamp")?;
    let cfg = EncoderConfig::from_header(decoder2.header());
    let value2 = decoder2
        .decode_to_value()
        .context("Failed to decode protected intermediate")?;
    let mut schema = decoder2
        .schema()
        .context("Failed to load inferred schema")?
        .clone();
    if let Some(prior) = prior_schema {
        for (path, doc) in &prior.documentation {
            schema.add_documentation(path.clone(), doc.clone());
        }
        for (path, constraints) in &prior.constraints {
            schema.add_constraints(path.clone(), constraints.clone());
        }
        for (name, tag) in &prior.ai_tags {
            schema.add_ai_tag(name.clone(), tag.clone());
        }
        schema.mark_sensitive_paths(prior.sensitive_path_list());
    }
    schema.mark_sensitive_paths(paths.iter().cloned());

    let json_value2 = bcs_core::convert::value_to_json(&value2)
        .map_err(anyhow::Error::msg)
        .context("Failed to convert stamped value tree")?;
    let json2 =
        serde_json::to_string(&json_value2).context("Failed to serialize stamped JSON")?;
    let mut encoder2 = Encoder::with_config(cfg).with_schema(schema);
    let output_data = encoder2
        .encode_from_json(&json2)
        .context("Failed to encode BCS with sensitive schema")?;

    utils::write_file(&output_path, &output_data)
        .with_context(|| format!("Failed to write output file: {}", output_path))?;

    if json_output {
        let payload = json!({
            "ok": true,
            "file": file,
            "output": output_path,
            "path_count": paths.len(),
            "scheme": scheme,
            "input_size": input_data.len(),
            "output_size": output_data.len(),
            "compression_ratio_percent": if input_data.is_empty() {
                0.0
            } else {
                (output_data.len() as f64 / input_data.len() as f64) * 100.0
            }
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("Failed to serialize protect JSON")?
        );
    } else {
        utils::print_success(&format!(
            "Sensitive protection applied successfully\n  Input:  {}\n  Output: {}",
            utils::format_size(input_data.len() as u64),
            utils::format_size(output_data.len() as u64)
        ));
    }

    Ok(())
}

fn default_output_path(input: &str) -> String {
    use std::path::Path;

    let input_path = Path::new(input);

    if let Some(stem) = input_path.file_stem().and_then(|s| s.to_str()) {
        let mut output = input_path.to_path_buf();
        output.set_file_name(format!("{}.protected.bcs", stem));
        output.to_string_lossy().to_string()
    } else {
        format!("{}.protected.bcs", input)
    }
}

fn resolve_password(password: Option<&str>, password_env: Option<&str>) -> Result<String> {
    utils::resolve_password_with_prompt(password, password_env, "Protect password: ", true)
}

fn collect_paths(raw: Option<&str>, paths_file: Option<&str>) -> Result<Vec<String>> {
    let mut paths: Vec<String> = Vec::new();

    if let Some(raw_paths) = raw {
        paths.extend(
            raw_paths
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string),
        );
    }

    if let Some(file_path) = paths_file {
        if !utils::file_exists(file_path) {
            anyhow::bail!("Paths file not found: {}", file_path);
        }

        let content = utils::read_file_string(file_path)
            .with_context(|| format!("Failed to read paths file: {}", file_path))?;

        paths.extend(
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter(|line| !line.starts_with('#'))
                .map(ToString::to_string),
        );
    }

    paths.sort();
    paths.dedup();

    if paths.is_empty() {
        anyhow::bail!("No sensitive paths were provided from --paths/--paths-file");
    }

    Ok(paths)
}
