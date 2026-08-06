// Decode command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::schema::{
    find_sensitive_plaintext_under, redact_sensitive_plaintext_under, Schema,
};
use bcs_core::security::{KeyWrapper, ResolverRegistry, SecretResolver};
use bcs_core::Decoder;
use std::time::Instant;

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: &str,
    output: Option<&str>,
    format: &str,
    path: Option<&str>,
    path_flatten: bool,
    stream: bool,
    verbose: bool,
    password: Option<&str>,
    password_env: Option<&str>,
    unwrap_kms: bool,
    kms_provider: Option<&str>,
    resolve_secrets: bool,
    secret_provider: Option<&str>,
    redact_sensitive_plaintext: bool,
    fail_on_sensitive_plaintext: bool,
) -> Result<()> {
    let started = Instant::now();

    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }

    match format {
        "json" | "yaml" => {}
        _ => anyhow::bail!(
            "Unsupported output format: {}. Supported formats: json, yaml",
            format
        ),
    }

    if verbose {
        utils::print_info(&format!("Decoding BCS file: {}", file));
    }

    if password.is_some() {
        utils::warn_password_on_argv("--password", "--password-env");
    }

    let resolved_password = resolve_password(password, password_env)?;
    let kms_wrapper = if unwrap_kms {
        Some(crate::kms_wrapper::resolve_unwrap_wrapper(kms_provider)?)
    } else {
        None
    };
    let resolver = if resolve_secrets {
        Some(build_secret_resolver(secret_provider)?)
    } else {
        None
    };

    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;

    let schema = decoder.schema().ok().cloned();
    let password = resolved_password.as_deref();
    let wrapper = kms_wrapper.as_ref().map(|w| w.as_ref() as &dyn KeyWrapper);
    let secret = resolver.as_ref().map(|r| r as &dyn SecretResolver);

    if stream {
        decode_stream(
            &mut decoder,
            output,
            format,
            password,
            wrapper,
            secret,
            schema.as_ref(),
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
            verbose,
        )?;
    } else if let Some(query_path) = path {
        decode_partial(
            &mut decoder,
            query_path,
            output,
            format,
            password,
            wrapper,
            secret,
            schema.as_ref(),
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
            path_flatten,
            verbose,
        )?;
    } else {
        decode_full(
            &mut decoder,
            output,
            format,
            password,
            wrapper,
            secret,
            schema.as_ref(),
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
            verbose,
        )?;
    }

    if verbose {
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        utils::print_success(&format!(
            "Decoding completed successfully in {:.2}ms",
            elapsed_ms
        ));
    }

    Ok(())
}

fn resolve_password(direct: Option<&str>, env_var: Option<&str>) -> Result<Option<String>> {
    if let Some(password) = direct {
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

pub fn build_secret_resolver(provider: Option<&str>) -> Result<ResolverRegistry> {
    let provider = provider
        .map(str::to_string)
        .or_else(|| std::env::var("BCS_SECRET_PROVIDER").ok())
        .unwrap_or_else(|| "env".to_string());
    bcs_secrets::registry_for_provider(&provider)
        .map_err(anyhow::Error::msg)
        .with_context(|| {
            format!(
                "Failed to initialize secret provider '{}' (available: {})",
                provider,
                bcs_secrets::available_providers().join(", ")
            )
        })
}

fn apply_protection(
    value: &mut bcs_core::types::Value,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
) -> Result<()> {
    if password.is_some() || wrapper.is_some() {
        bcs_core::security::reveal_all_ex(value, password, wrapper)
            .map_err(anyhow::Error::msg)
            .context("Failed to reveal protected fields")?;
    } else {
        bcs_core::security::mask_all(value)
            .map_err(anyhow::Error::msg)
            .context("Failed to mask protected fields")?;
    }
    Ok(())
}

fn apply_secret_refs(
    value: &mut bcs_core::types::Value,
    resolver: Option<&dyn SecretResolver>,
) -> Result<()> {
    if let Some(resolver) = resolver {
        bcs_core::security::resolve_secret_refs(value, resolver)
            .map_err(anyhow::Error::msg)
            .context("Failed to resolve secret references")?;
    } else {
        bcs_core::security::mask_secret_refs(value)
            .map_err(anyhow::Error::msg)
            .context("Failed to mask secret references")?;
    }
    Ok(())
}

fn apply_sensitive_plaintext_policy(
    schema: Option<&Schema>,
    value: &mut bcs_core::types::Value,
    root_path: Option<&str>,
    redact: bool,
    fail: bool,
) -> Result<()> {
    if !redact && !fail {
        return Ok(());
    }
    let Some(schema) = schema else {
        if fail || redact {
            utils::print_info(
                "note: --redact-sensitive-plaintext / --fail-on-sensitive-plaintext require an embedded schema; skipping",
            );
        }
        return Ok(());
    };

    if fail {
        let findings = find_sensitive_plaintext_under(schema, value, root_path)
            .map_err(anyhow::Error::msg)
            .context("Failed to check sensitive plaintext")?;
        if !findings.is_empty() {
            for f in &findings {
                eprintln!("error: {}", f.message);
            }
            anyhow::bail!(
                "{} sensitive path(s) hold plaintext; refuse to decode (--fail-on-sensitive-plaintext)",
                findings.len()
            );
        }
    }

    if redact {
        let findings = redact_sensitive_plaintext_under(schema, value, root_path)
            .map_err(anyhow::Error::msg)
            .context("Failed to redact sensitive plaintext")?;
        if !findings.is_empty() {
            eprintln!(
                "warning: redacted {} sensitive plaintext path(s) to [SENSITIVE]",
                findings.len()
            );
        }
    }

    Ok(())
}

fn write_decoded(output: Option<&str>, decoded: &str, verbose: bool) -> Result<()> {
    if let Some(output_path) = output {
        utils::write_file_string(output_path, decoded)
            .with_context(|| format!("Failed to write output file: {}", output_path))?;
        if verbose {
            utils::print_info(&format!("Output written to: {}", output_path));
        }
    } else {
        println!("{}", decoded);
    }
    Ok(())
}

fn serialize_value(value: &bcs_core::types::Value, format: &str) -> Result<String> {
    let json_value = bcs_core::convert::value_to_json(value)
        .map_err(anyhow::Error::msg)
        .context("Failed to convert decoded value")?;
    match format {
        "json" => serde_json::to_string_pretty(&json_value).context("Failed to serialize to JSON"),
        "yaml" => serde_yaml::to_string(&json_value).context("Failed to serialize to YAML"),
        _ => unreachable!(),
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_full(
    decoder: &mut Decoder,
    output: Option<&str>,
    format: &str,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    resolver: Option<&dyn SecretResolver>,
    schema: Option<&Schema>,
    redact: bool,
    fail: bool,
    verbose: bool,
) -> Result<()> {
    if verbose {
        utils::print_info("Decoding full file...");
    }

    let mut value = decoder
        .decode_to_value()
        .context("Failed to decode full value tree")?;

    apply_protection(&mut value, password, wrapper)?;
    apply_secret_refs(&mut value, resolver)?;
    apply_sensitive_plaintext_policy(schema, &mut value, None, redact, fail)?;

    let decoded = serialize_value(&value, format)?;
    write_decoded(output, &decoded, verbose)
}

#[allow(clippy::too_many_arguments)]
fn decode_partial(
    decoder: &mut Decoder,
    path: &str,
    output: Option<&str>,
    format: &str,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    resolver: Option<&dyn SecretResolver>,
    schema: Option<&Schema>,
    redact: bool,
    fail: bool,
    path_flatten: bool,
    verbose: bool,
) -> Result<()> {
    if verbose {
        utils::print_info(&format!("Decoding path: {}", path));
    }

    let (mut value, access) = decoder
        .get_path_with_access(path)
        .with_context(|| format!("Failed to get value at path: {}", path))?;

    if verbose {
        let access_label = match access {
            bcs_core::PathAccessKind::Indexed => "indexed",
            bcs_core::PathAccessKind::Walk => "walk",
            bcs_core::PathAccessKind::Full => "full",
        };
        utils::print_info(&format!("access={}", access_label));
    }

    // Align with full decode: recursively mask/reveal protect markers in the subtree.
    apply_protection(&mut value, password, wrapper)?;
    apply_secret_refs(&mut value, resolver)?;
    apply_sensitive_plaintext_policy(schema, &mut value, Some(path), redact, fail)?;

    if path_flatten {
        value = flatten_value_lists(value);
    }

    let decoded = serialize_value(&value, format)?;
    write_decoded(output, &decoded, verbose)
}

fn flatten_value_lists(value: bcs_core::types::Value) -> bcs_core::types::Value {
    use bcs_core::types::Value;

    let mut out = Vec::new();
    let _ = flatten_recursive(value, &mut out, 0);
    Value::List(out)
}

fn flatten_recursive(
    value: bcs_core::types::Value,
    out: &mut Vec<bcs_core::types::Value>,
    depth: usize,
) -> Result<()> {
    use bcs_core::limits;
    use bcs_core::types::Value;

    limits::ensure_depth(depth).map_err(anyhow::Error::msg)?;
    match value {
        Value::List(items) => {
            for item in items {
                flatten_recursive(item, out, depth + 1)?;
            }
        }
        other => out.push(other),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decode_stream(
    decoder: &mut Decoder,
    output: Option<&str>,
    format: &str,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    resolver: Option<&dyn SecretResolver>,
    schema: Option<&Schema>,
    redact: bool,
    fail: bool,
    verbose: bool,
) -> Result<()> {
    if verbose {
        utils::print_info("Streaming decode...");
    }

    // Stream path currently materializes the full tree (same as historical behavior).
    let mut value = decoder
        .decode_to_value()
        .context("Failed to decode value for stream mode")?;

    apply_protection(&mut value, password, wrapper)?;
    apply_secret_refs(&mut value, resolver)?;
    apply_sensitive_plaintext_policy(schema, &mut value, None, redact, fail)?;

    let decoded = serialize_value(&value, format)?;
    write_decoded(output, &decoded, verbose)
}
