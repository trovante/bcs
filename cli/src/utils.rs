// Common utilities for CLI operations

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Print an error message with proper formatting
pub fn print_error(error: &anyhow::Error) {
    eprintln!("Error: {}", error);

    // Print error chain
    let mut source = error.source();
    while let Some(err) = source {
        eprintln!("  Caused by: {}", err);
        source = err.source();
    }
}

/// Print a success message
pub fn print_success(message: &str) {
    println!("✓ {}", message);
}

/// Print an info message
pub fn print_info(message: &str) {
    println!("ℹ {}", message);
}

/// Print a warning message
pub fn print_warning(message: &str) {
    eprintln!("⚠ {}", message);
}

/// Read file contents
#[allow(dead_code)]
pub fn read_file(path: &str) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("Failed to read file: {}", path))
}

/// Read file contents as string
pub fn read_file_string(path: &str) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))
}

/// Write file contents
pub fn write_file(path: &str, data: &[u8]) -> Result<()> {
    fs::write(path, data).with_context(|| format!("Failed to write file: {}", path))
}

/// Write file contents as string
pub fn write_file_string(path: &str, data: &str) -> Result<()> {
    fs::write(path, data).with_context(|| format!("Failed to write file: {}", path))
}

/// Check if file exists
pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Get file extension
pub fn get_extension(path: &str) -> Option<&str> {
    Path::new(path).extension().and_then(|ext| ext.to_str())
}

/// Format file size in human-readable format
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Format duration in human-readable format
pub fn format_duration(nanos: u128) -> String {
    if nanos < 1_000 {
        format!("{} ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2} μs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

/// Simple progress indicator
#[allow(dead_code)]
pub struct ProgressIndicator {
    message: String,
    started: bool,
}

impl ProgressIndicator {
    #[allow(dead_code)]
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            started: false,
        }
    }

    #[allow(dead_code)]
    pub fn start(&mut self) {
        if !self.started {
            print!("{} ... ", self.message);
            self.started = true;
        }
    }

    #[allow(dead_code)]
    pub fn finish(&self, success: bool) {
        if self.started {
            if success {
                println!("✓");
            } else {
                println!("✗");
            }
        }
    }
}

impl Drop for ProgressIndicator {
    fn drop(&mut self) {
        if self.started {
            println!();
        }
    }
}

/// Format a table row
pub fn format_table_row(columns: &[(&str, usize)]) -> String {
    columns
        .iter()
        .map(|(text, width)| format!("{:<width$}", text, width = width))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Print a table separator
pub fn print_table_separator(column_widths: &[usize]) {
    let separator = column_widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>()
        .join("-+-");
    println!("{}", separator);
}

/// Read a password from an interactive terminal prompt.
pub fn prompt_password(prompt: &str) -> Result<String> {
    use std::io::{self, Write};

    eprint!("{}", prompt);
    io::stderr().flush().ok();

    // Prefer /dev/tty on Unix so prompts work even when stdin is redirected.
    #[cfg(unix)]
    {
        use std::io::BufRead;
        if let Ok(tty) = fs::File::open("/dev/tty") {
            let mut reader = io::BufReader::new(tty);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .context("Failed to read password from terminal")?;
            let password = line.trim_end_matches(['\r', '\n']).to_string();
            if password.is_empty() {
                anyhow::bail!("Password cannot be empty");
            }
            return Ok(password);
        }
    }

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("Failed to read password from stdin")?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }
    Ok(password)
}

/// Emit a stderr warning when a password was supplied on the argv (visible in `ps` / history).
pub fn warn_password_on_argv(flag: &str, env_flag: &str) {
    eprintln!(
        "warning: {} puts the password on the process command line (visible in `ps` and shell history); prefer {} or an interactive prompt",
        flag, env_flag
    );
}

/// Resolve password from flag, env, or interactive prompt.
pub fn resolve_password_with_prompt(
    password: Option<&str>,
    password_env: Option<&str>,
    prompt: &str,
    allow_prompt: bool,
) -> Result<String> {
    if let Some(p) = password {
        if p.is_empty() {
            anyhow::bail!("Password cannot be empty");
        }
        warn_password_on_argv(
            "--password / --protect-password",
            "--password-env / --protect-password-env",
        );
        return Ok(p.to_string());
    }

    if let Some(var_name) = password_env {
        let value = std::env::var(var_name)
            .with_context(|| format!("Failed to read password from env var: {}", var_name))?;
        if value.is_empty() {
            anyhow::bail!("Environment variable '{}' is empty", var_name);
        }
        return Ok(value);
    }

    if allow_prompt {
        return prompt_password(prompt);
    }

    anyhow::bail!("A password is required (use --password, --password-env, or interactive prompt)")
}
