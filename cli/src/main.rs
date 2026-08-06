use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

mod commands;
mod kms_wrapper;
mod utils;

use commands::{
    benchmark, decode, encode, inspect, protect, reindex, run as run_cmd, scan, schema, show,
    validate,
};

#[derive(Parser)]
#[command(name = "bcs")]
#[command(version, about = "Binary Config Schema CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate a BCS file against its embedded schema
    Validate {
        /// Path to BCS file
        file: String,

        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,

        /// Fail when a sensitive path holds plaintext (default: warn only)
        #[arg(long)]
        fail_on_sensitive_plaintext: bool,
    },

    /// Encode configuration to BCS format
    Encode {
        /// Input file (JSON, YAML, or TOML)
        input: String,

        /// Output BCS file (defaults to input path with .bcs extension)
        #[arg(short, long)]
        output: Option<String>,

        /// Optional schema file for validation
        #[arg(short, long)]
        schema: Option<String>,

        /// Compact mode: minimize output size (disables embedded schema and index)
        #[arg(long)]
        compact: bool,

        /// Compress data layer with LZ4 (best size, slower full decode)
        #[arg(long)]
        compress_data: bool,

        /// Opt-in structural dedup: keys | strings | all
        #[arg(long)]
        dedup: Option<String>,

        /// Min repeats before a string is interned (default 2)
        #[arg(long, default_value_t = 2)]
        dedup_min_repeats: usize,

        /// Min string length to consider for dedup (default 4)
        #[arg(long, default_value_t = 4)]
        dedup_min_length: usize,

        /// Also index nested fields of structs/maps with at least N entries
        #[arg(long)]
        index_maps_over: Option<usize>,

        /// Comma-separated sensitive paths to protect (e.g. database.password,api.key)
        #[arg(long, alias = "protect")]
        protect_paths: Option<String>,

        /// File with sensitive paths to protect (one path per line)
        #[arg(long)]
        protect_paths_file: Option<String>,

        /// Comma-separated paths marked sensitive in schema only (no encryption)
        #[arg(long)]
        sensitive_paths: Option<String>,

        /// File with schema-only sensitive paths (one path per line)
        #[arg(long)]
        sensitive_paths_file: Option<String>,

        /// Password used to protect sensitive fields
        #[arg(long)]
        protect_password: Option<String>,

        /// Environment variable containing password used to protect sensitive fields
        #[arg(long)]
        protect_password_env: Option<String>,

        /// Protect scheme: `pbkdf2` (password) or `kms` (external command wrapper)
        #[arg(long, default_value = "pbkdf2")]
        protect_scheme: String,

        /// KMS provider label stored in the marker (used with `--protect-scheme kms`)
        #[arg(long)]
        kms_provider: Option<String>,

        /// KMS key locator stored in the marker (used with `--protect-scheme kms`)
        #[arg(long)]
        kms_key: Option<String>,
    },

    /// Decode BCS file to JSON/YAML
    Decode {
        /// BCS file to decode
        file: String,

        /// Output file (if not specified, prints to stdout)
        #[arg(short, long)]
        output: Option<String>,

        /// Output format (json or yaml)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Path to specific value (e.g., "a.b.c[0].d")
        #[arg(short, long)]
        path: Option<String>,

        /// Flatten nested list results for wildcard path queries
        #[arg(long)]
        path_flatten: bool,

        /// Stream mode (decode incrementally)
        #[arg(short, long)]
        stream: bool,

        /// Show decode progress and status logs
        #[arg(short, long)]
        verbose: bool,

        /// Password to reveal protected sensitive fields during decode
        #[arg(long)]
        password: Option<String>,

        /// Environment variable containing password to reveal protected fields
        #[arg(long)]
        password_env: Option<String>,

        /// Reveal `kms`-scheme fields via native KMS or `BCS_KMS_UNWRAP_CMD`
        #[arg(long)]
        unwrap_kms: bool,

        /// KMS provider for `--unwrap-kms` (aws, azure, gcp, vault, openbao, cmd).
        /// When omitted, all available native backends (+ cmd if configured) are tried by marker provider.
        #[arg(long)]
        kms_provider: Option<String>,

        /// Resolve `__bcs_secret_ref__:` markers via the selected secret provider
        #[arg(long)]
        resolve_secrets: bool,

        /// Secret provider used with `--resolve-secrets` (default: env).
        /// Examples: env, vault, openbao, aws, azure, gcp, doppler, infisical, akeyless, bitwarden
        /// (availability depends on build features). Also read from `BCS_SECRET_PROVIDER`.
        #[arg(long)]
        secret_provider: Option<String>,

        /// Replace schema-marked sensitive plaintext with `[SENSITIVE]` (requires embedded schema)
        #[arg(long)]
        redact_sensitive_plaintext: bool,

        /// Fail if schema-marked sensitive paths still hold plaintext (requires embedded schema)
        #[arg(long)]
        fail_on_sensitive_plaintext: bool,
    },

    /// Protect sensitive fields in an existing BCS file
    Protect {
        /// Input BCS file to protect
        file: String,

        /// Output protected BCS file (defaults to input path with .protected.bcs suffix)
        #[arg(short, long)]
        output: Option<String>,

        /// Comma-separated sensitive paths to protect
        #[arg(long)]
        paths: Option<String>,

        /// File with sensitive paths to protect (one path per line)
        #[arg(long)]
        paths_file: Option<String>,

        /// Password used to encrypt sensitive fields (`pbkdf2` scheme)
        #[arg(long)]
        password: Option<String>,

        /// Environment variable containing password used to encrypt sensitive fields
        #[arg(long)]
        password_env: Option<String>,

        /// Protect scheme: `pbkdf2` (password) or `kms` (external command wrapper)
        #[arg(long, default_value = "pbkdf2")]
        scheme: String,

        /// KMS provider label stored in the marker
        #[arg(long)]
        kms_provider: Option<String>,

        /// KMS key locator stored in the marker
        #[arg(long)]
        kms_key: Option<String>,

        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },

    /// Rebuild a BCS file with index support for path queries
    Reindex {
        /// Input BCS file
        file: String,

        /// Output BCS file with rebuilt index
        #[arg(short, long)]
        output: Option<String>,

        /// Also embed semantic/schema layer in output
        #[arg(long)]
        add_schema: bool,

        /// Keep data-layer compression in output
        #[arg(long)]
        compress_data: bool,

        /// Preview projected changes without writing output file
        #[arg(long)]
        dry_run: bool,

        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },

    /// Inspect BCS file metadata and structure
    Inspect {
        /// BCS file to inspect
        file: String,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,

        /// Print lazy inspect tree (masks protect/secret markers)
        #[arg(long)]
        tree: bool,
    },

    /// Extract and display schema from BCS file
    Schema {
        /// BCS file
        file: String,

        /// Export schema to JSON file (full Schema dump)
        #[arg(short, long)]
        export: Option<String>,

        /// Emit agent-safe schema JSON (paths, types, sensitive; never values)
        #[arg(long)]
        agent_safe: bool,
    },

    /// Benchmark BCS file operations
    Benchmark {
        /// BCS file to benchmark
        file: String,

        /// Compare against JSON/YAML/TOML file
        #[arg(short, long)]
        compare: Option<String>,

        /// Number of benchmark runs for percentile metrics
        #[arg(long, default_value_t = 5)]
        runs: usize,

        /// Benchmark mode: full (all metrics) or path-hot (repeated path query focus)
        #[arg(long, value_enum, default_value_t = BenchmarkMode::Full)]
        mode: BenchmarkMode,

        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },

    /// Scan sources or `.bcs` files for leaked secrets / sensitive plaintext
    Scan {
        /// File or directory to scan
        path: String,

        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,

        /// Fail on `finding` (default) or also on `warn`
        #[arg(long, default_value = "finding")]
        fail_on: String,
    },

    /// Decode BCS and run a command with config injected as environment
    Run {
        /// BCS file
        file: String,

        /// Flatten config into child process environment
        #[arg(long, default_value_t = true)]
        export_env: bool,

        /// Resolve secret-ref markers before injecting env
        #[arg(long)]
        resolve_secrets: bool,

        /// Secret provider for `--resolve-secrets`
        #[arg(long)]
        secret_provider: Option<String>,

        /// Password to reveal protect markers
        #[arg(long)]
        password: Option<String>,

        /// Env var containing protect password
        #[arg(long)]
        password_env: Option<String>,

        /// Unwrap KMS protect markers
        #[arg(long)]
        unwrap_kms: bool,

        /// KMS provider for unwrap
        #[arg(long)]
        kms_provider: Option<String>,

        /// Print env keys only (redact sensitive); do not exec
        #[arg(long)]
        dry_run: bool,

        /// Also set this env var to the full JSON document
        #[arg(long)]
        json_env: Option<String>,

        /// Optional subtree path to export
        #[arg(long)]
        path: Option<String>,

        /// Prefix prepended to every exported env key (e.g. APP_)
        #[arg(long)]
        prefix: Option<String>,

        /// Comma-separated dotted paths to export (allowlist)
        #[arg(long)]
        only: Option<String>,

        /// Command and args after `--`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Print KEY='value' lines for shell eval (redacts sensitive by default; no .env file)
    Env {
        /// BCS file
        file: String,

        /// Resolve secret-ref markers before printing
        #[arg(long)]
        resolve_secrets: bool,

        /// Secret provider for `--resolve-secrets`
        #[arg(long)]
        secret_provider: Option<String>,

        /// Password to reveal protect markers
        #[arg(long)]
        password: Option<String>,

        /// Env var containing protect password
        #[arg(long)]
        password_env: Option<String>,

        /// Unwrap KMS protect markers
        #[arg(long)]
        unwrap_kms: bool,

        /// KMS provider for unwrap
        #[arg(long)]
        kms_provider: Option<String>,

        /// Optional subtree path to export
        #[arg(long)]
        path: Option<String>,

        /// Prefix prepended to every exported env key (e.g. APP_)
        #[arg(long)]
        prefix: Option<String>,

        /// Comma-separated dotted paths to export (allowlist)
        #[arg(long)]
        only: Option<String>,

        /// Print sensitive values (operator-only; prefer `bcs run` for secrets)
        #[arg(long)]
        allow_sensitive: bool,
    },

    /// Show a path with segment argv (tree on TTY, json when piped)
    Show {
        /// BCS file
        file: String,

        /// Path segments (e.g. database host → database.host)
        segments: Vec<String>,

        /// Output format: tree or json (default: tree on TTY, json when piped)
        #[arg(short = 'f', long)]
        format: Option<String>,

        /// Replace schema-marked sensitive plaintext with `[SENSITIVE]` (requires embedded schema)
        #[arg(long)]
        redact_sensitive_plaintext: bool,

        /// Fail if schema-marked sensitive paths still hold plaintext (requires embedded schema)
        #[arg(long)]
        fail_on_sensitive_plaintext: bool,
    },

    /// Debug dump of a BCS file (not a wire format)
    Dump {
        /// BCS file
        file: String,

        /// Dump format (currently: debug-tree)
        #[arg(long, default_value = "debug-tree")]
        format: String,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum BenchmarkMode {
    Full,
    PathHot,
}

fn main() {
    if let Err(e) = run() {
        utils::print_error(&e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate {
            file,
            json,
            fail_on_sensitive_plaintext,
        } => validate::run(&file, json, fail_on_sensitive_plaintext),
        Commands::Encode {
            input,
            output,
            schema,
            compact,
            compress_data,
            dedup,
            dedup_min_repeats,
            dedup_min_length,
            index_maps_over,
            protect_paths,
            protect_paths_file,
            sensitive_paths,
            sensitive_paths_file,
            protect_password,
            protect_password_env,
            protect_scheme,
            kms_provider,
            kms_key,
        } => encode::run(
            &input,
            output.as_deref(),
            schema.as_deref(),
            compact,
            compress_data,
            dedup.as_deref(),
            dedup_min_repeats,
            dedup_min_length,
            index_maps_over,
            protect_paths.as_deref(),
            protect_paths_file.as_deref(),
            sensitive_paths.as_deref(),
            sensitive_paths_file.as_deref(),
            protect_password.as_deref(),
            protect_password_env.as_deref(),
            &protect_scheme,
            kms_provider.as_deref(),
            kms_key.as_deref(),
        ),
        Commands::Decode {
            file,
            output,
            format,
            path,
            path_flatten,
            stream,
            verbose,
            password,
            password_env,
            unwrap_kms,
            kms_provider,
            resolve_secrets,
            secret_provider,
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
        } => decode::run(
            &file,
            output.as_deref(),
            &format,
            path.as_deref(),
            path_flatten,
            stream,
            verbose,
            password.as_deref(),
            password_env.as_deref(),
            unwrap_kms,
            kms_provider.as_deref(),
            resolve_secrets,
            secret_provider.as_deref(),
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
        ),
        Commands::Protect {
            file,
            output,
            paths,
            paths_file,
            password,
            password_env,
            scheme,
            kms_provider,
            kms_key,
            json,
        } => protect::run(
            &file,
            output.as_deref(),
            paths.as_deref(),
            paths_file.as_deref(),
            password.as_deref(),
            password_env.as_deref(),
            &scheme,
            kms_provider.as_deref(),
            kms_key.as_deref(),
            json,
        ),
        Commands::Reindex {
            file,
            output,
            add_schema,
            compress_data,
            dry_run,
            json,
        } => reindex::run(
            &file,
            output.as_deref(),
            add_schema,
            compress_data,
            dry_run,
            json,
        ),
        Commands::Inspect {
            file,
            verbose,
            json,
            tree,
        } => inspect::run(&file, verbose, json, tree),
        Commands::Schema {
            file,
            export,
            agent_safe,
        } => schema::run(&file, export.as_deref(), agent_safe),
        Commands::Benchmark {
            file,
            compare,
            runs,
            mode,
            json,
        } => benchmark::run(
            &file,
            compare.as_deref(),
            runs,
            matches!(mode, BenchmarkMode::PathHot),
            json,
        ),
        Commands::Scan {
            path,
            json,
            fail_on,
        } => {
            let fail = match fail_on.to_ascii_lowercase().as_str() {
                "warn" => scan::FailOn::Warn,
                "finding" => scan::FailOn::Finding,
                other => anyhow::bail!("Invalid --fail-on '{}'. Use finding or warn", other),
            };
            scan::run(&path, json, fail)
        }
        Commands::Run {
            file,
            export_env,
            resolve_secrets,
            secret_provider,
            password,
            password_env,
            unwrap_kms,
            kms_provider,
            dry_run,
            json_env,
            path,
            prefix,
            only,
            command,
        } => run_cmd::run(
            &file,
            &command,
            export_env,
            resolve_secrets,
            secret_provider.as_deref(),
            password.as_deref(),
            password_env.as_deref(),
            unwrap_kms,
            kms_provider.as_deref(),
            dry_run,
            json_env.as_deref(),
            path.as_deref(),
            prefix.as_deref(),
            only.as_deref(),
        ),
        Commands::Env {
            file,
            resolve_secrets,
            secret_provider,
            password,
            password_env,
            unwrap_kms,
            kms_provider,
            path,
            prefix,
            only,
            allow_sensitive,
        } => run_cmd::env_print(
            &file,
            resolve_secrets,
            secret_provider.as_deref(),
            password.as_deref(),
            password_env.as_deref(),
            unwrap_kms,
            kms_provider.as_deref(),
            path.as_deref(),
            prefix.as_deref(),
            only.as_deref(),
            allow_sensitive,
        ),
        Commands::Show {
            file,
            segments,
            format,
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
        } => show::run(
            &file,
            &segments,
            format.as_deref(),
            redact_sensitive_plaintext,
            fail_on_sensitive_plaintext,
        ),
        Commands::Dump { file, format } => {
            if format != "debug-tree" {
                anyhow::bail!("Unsupported dump format '{}'. Use debug-tree", format);
            }
            show::dump_debug_tree(&file)
        }
    }
}
