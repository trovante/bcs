//! # Binary Config Schema (BCS) — Core Library
//!
//! A Rust-first binary configuration format that encodes JSON, YAML, or TOML
//! into a compact, inspectable container with optional indexing, compression,
//! integrity checks, and field-level secret protection.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use bcs_core::{Encoder, Decoder};
//!
//! // Encode JSON to BCS
//! let mut encoder = Encoder::new();
//! let bcs_data = encoder.encode_from_json(r#"{"host":"localhost","port":8080}"#).unwrap();
//!
//! // Decode BCS back to JSON
//! let mut decoder = Decoder::from_bytes(&bcs_data).unwrap();
//! let json = decoder.to_json().unwrap();
//! ```
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`encoder`] | Encode JSON/YAML/TOML into BCS binary format |
//! | [`decoder`] | Decode BCS files with full, partial, or streaming access |
//! | [`index`] | O(1) path lookup via XXHash64-based index tables |
//! | [`schema`] | Type definitions, constraints, and validation engine |
//! | [`security`] | Field-level encryption (PBKDF2/KMS) and secret references |
//! | [`secret_resolver`] | Pluggable secret resolution trait and registry |
//! | [`types`] | Core type system: header, value enum, composite encoder/decoder |
//! | [`limits`] | Resource limits for untrusted input safety |
//! | [`error`] | Error types and `Result` alias |
//! | [`convert`] | Conversion utilities from BCS `Value` to `serde_json::Value` |

pub mod convert;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod index;
pub mod inspect_ast;
pub mod limits;
pub mod scan;
pub mod schema;
pub mod secret_resolver;
pub mod security;
pub mod string_table;
pub mod types;

pub use decoder::{Decoder, PathAccessKind};
pub use encoder::{Encoder, EncoderConfig};
pub use error::{BCSError, Result};
pub use inspect_ast::{InspectMarker, InspectNode};
pub use scan::{scan_path, ScanFailOn, ScanFinding, ScanReport};
pub use schema::{
    find_sensitive_plaintext, find_sensitive_plaintext_under, redact_sensitive_plaintext,
    redact_sensitive_plaintext_under, AgentSafePath, AgentSafeSchema, Schema, SchemaEngine,
    SensitivePlaintextFinding, SENSITIVE_PLAINTEXT_MASK,
};
pub use string_table::{DedupMode, DedupThresholds, StringTable};
