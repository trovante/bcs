//! `bcs scan` — leak / sensitive-plaintext scanner for sources and `.bcs` files.

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::scan_path;
use std::path::Path;

pub use bcs_core::ScanFailOn as FailOn;

pub fn run(target: &str, json_output: bool, fail_on: FailOn) -> Result<()> {
    let path = Path::new(target);
    let report = scan_path(path, fail_on).with_context(|| format!("scan {}", target))?;

    if json_output {
        println!(
            "{}",
            report
                .to_json_pretty()
                .context("serialize scan JSON")?
        );
    } else if report.findings.is_empty() {
        utils::print_success("Scan clean — no findings.");
    } else {
        for f in &report.findings {
            let line = format!(
                "[{}] {} @ {}: {}",
                f.severity, f.kind, f.location, f.message
            );
            if f.severity == "finding" {
                eprintln!("error: {}", line);
            } else {
                utils::print_warning(&line);
            }
        }
    }

    if !report.ok {
        std::process::exit(1);
    }
    Ok(())
}
