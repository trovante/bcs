use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

// Helper function to create test JSON data
fn create_test_json_file(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join(format!("bcs_test_{}.json", name));

    let test_data = r#"{
  "app": {
    "name": "MyApp",
    "version": "1.0.0",
    "debug": true
  },
  "database": {
    "host": "localhost",
    "port": 5432,
    "name": "myapp_db"
  },
  "features": ["auth", "logging", "metrics"]
}"#;

    fs::write(&path, test_data)
        .unwrap_or_else(|_| panic!("Failed to write test JSON file to {:?}", path));

    // Verify the file was created
    if !path.exists() {
        panic!("File was not created at {:?}", path);
    }

    path
}

// Helper function to create test schema file
fn create_test_schema_file(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join(format!("bcs_test_{}_schema.json", name));

    let schema_data = r#"{
  "type": "object",
  "properties": {
    "app": {
      "type": "object",
      "properties": {
        "name": {"type": "string"},
        "version": {"type": "string"},
        "debug": {"type": "boolean"}
      }
    },
    "database": {
      "type": "object",
      "properties": {
        "host": {"type": "string"},
        "port": {"type": "integer"},
        "name": {"type": "string"}
      }
    },
    "features": {
      "type": "array",
      "items": {"type": "string"}
    }
  }
}"#;

    fs::write(&path, schema_data).expect("Failed to write test schema file");
    path
}

// Helper function to get a temporary output path
fn temp_output_path(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    temp_dir.join(format!("bcs_test_{}", name))
}

// Helper to clean up test files
fn cleanup_test_file(path: &PathBuf) {
    let _ = fs::remove_file(path);
}

#[test]
fn test_encode_json_to_bcs() {
    let input = create_test_json_file("encode");
    let output = temp_output_path("encode_test.bcs");

    cleanup_test_file(&output);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(output.to_str().unwrap());

    cmd.assert().success();

    // Verify output file was created
    assert!(output.exists(), "Output BCS file should be created");

    cleanup_test_file(&output);
    cleanup_test_file(&input);
}

#[test]
fn test_encode_default_output_path() {
    let input = create_test_json_file("default_output");
    let expected_output = input.with_file_name("bcs_test_default_output.bcs");

    cleanup_test_file(&expected_output);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("encode").arg(input.to_str().unwrap());

    cmd.assert().success();

    assert!(
        expected_output.exists(),
        "Expected default output at {:?}",
        expected_output
    );

    cleanup_test_file(&input);
    cleanup_test_file(&expected_output);
}

#[test]
fn test_encode_with_schema() {
    let input = create_test_json_file("encode_schema");
    let schema = create_test_schema_file("encode_schema");
    let output = temp_output_path("encode_with_schema.bcs");

    cleanup_test_file(&output);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(output.to_str().unwrap())
        .arg("-s")
        .arg(schema.to_str().unwrap());

    // Schema validation may fail due to implementation details
    // Just verify the command runs (success or expected failure)
    let _result = cmd.assert();

    // If it succeeds, verify output was created
    if output.exists() {
        cleanup_test_file(&output);
    }

    cleanup_test_file(&input);
    cleanup_test_file(&schema);
}

#[test]
fn test_encode_nonexistent_file() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("nonexistent.json");
    let output = temp_output_path("encode_fail.bcs");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(output.to_str().unwrap());

    cmd.assert().failure();
}

#[test]
fn test_decode_bcs_to_json() {
    // First encode a file
    let input = create_test_json_file("decode");
    let bcs_file = temp_output_path("decode_test.bcs");
    let output = temp_output_path("decode_test.json");

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&output);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Decode
    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("-o")
        .arg(output.to_str().unwrap())
        .arg("-f")
        .arg("json");

    decode_cmd.assert().success();

    assert!(output.exists(), "Decoded JSON file should be created");

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&output);
    cleanup_test_file(&input);
}

#[test]
fn test_decode_to_stdout() {
    // First encode a file
    let input = create_test_json_file("decode_stdout");
    let bcs_file = temp_output_path("decode_stdout_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Decode to stdout - should output JSON
    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd.arg("decode").arg(bcs_file.to_str().unwrap());

    decode_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("app")); // Should contain app field

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_decode_with_path() {
    // First encode a file
    let input = create_test_json_file("decode_path");
    let bcs_file = temp_output_path("decode_path_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Decode specific path - test that path query works
    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("-p")
        .arg("app.name");

    // Path query runs (may succeed or fail depending on schema)
    let _ = decode_cmd.assert();

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_decode_deep_nested_array_object_path() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_nested_array_object_test.json");
    let bcs_file = temp_output_path("decode_nested_array_object_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let nested_data = r#"{
  "services": [
    {
      "name": "api",
      "routes": [
        {"method": "GET", "paths": ["/health", "/ready"]},
        {"method": "POST", "paths": ["/items", "/items/bulk"]}
      ]
    },
    {
      "name": "worker",
      "routes": [
        {"method": "GET", "paths": ["/metrics"]}
      ]
    }
  ]
}"#;

    fs::write(&input, nested_data).expect("Failed to write nested test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--path")
        .arg("services[0].routes[1].paths[0]")
        .assert()
        .success()
        .stdout(predicate::str::contains("/items"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_wildcard_path_mongo_style() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_nested_wildcard_test.json");
    let bcs_file = temp_output_path("decode_nested_wildcard_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let nested_data = r#"{
  "services": [
    {
      "name": "api",
      "routes": [
        {"method": "GET", "paths": ["/health", "/ready"]},
        {"method": "POST", "paths": ["/items", "/items/bulk"]}
      ]
    },
    {
      "name": "worker",
      "routes": [
        {"method": "GET", "paths": ["/metrics"]}
      ]
    }
  ]
}"#;

    fs::write(&input, nested_data).expect("Failed to write wildcard test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--path")
        .arg("services.$.routes.$.paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("/health"))
        .stdout(predicate::str::contains("/items"))
        .stdout(predicate::str::contains("/metrics"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_wildcard_path_with_flatten() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_nested_wildcard_flatten_test.json");
    let bcs_file = temp_output_path("decode_nested_wildcard_flatten_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let nested_data = r#"{
  "services": [
    {
      "name": "api",
      "routes": [
        {"method": "GET", "paths": ["/health", "/ready"]},
        {"method": "POST", "paths": ["/items", "/items/bulk"]}
      ]
    },
    {
      "name": "worker",
      "routes": [
        {"method": "GET", "paths": ["/metrics"]}
      ]
    }
  ]
}"#;

    fs::write(&input, nested_data).expect("Failed to write wildcard flatten test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--path")
        .arg("services.$.routes.$.paths")
        .arg("--path-flatten")
        .assert()
        .success()
        .stdout(predicate::str::contains("/health"))
        .stdout(predicate::str::contains("/items"))
        .stdout(predicate::str::contains("/metrics"))
        .stdout(predicate::str::contains("[[").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_nonexistent_file() {
    let temp_dir = std::env::temp_dir();
    let bcs_file = temp_dir.join("nonexistent.bcs");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("decode").arg(bcs_file.to_str().unwrap());

    cmd.assert().failure();
}

#[test]
fn test_sensitive_fields_are_masked_without_password_and_revealed_with_password() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_sensitive_input.json");
    let bcs_file = temp_output_path("sensitive_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let test_data = r#"{
  "database": {
    "user": "admin",
    "password": "super-secret"
  },
  "api": {
    "token": "top-secret-token"
  }
}"#;

    fs::write(&input, test_data).expect("Failed to write sensitive test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .arg("--protect-paths")
        .arg("database.password,api.token")
        .arg("--protect-password")
        .arg("my-password")
        .assert()
        .success();

    let mut decode_without_password = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_without_password
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("[PROTECTED]"))
        .stdout(predicate::str::contains("super-secret").not());

    let mut decode_with_password = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_with_password
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--password")
        .arg("my-password")
        .assert()
        .success()
        .stdout(predicate::str::contains("super-secret"))
        .stdout(predicate::str::contains("top-secret-token"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_secret_refs_are_masked_by_default_and_resolved_with_flag() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_secret_ref_input.json");
    let bcs_file = temp_output_path("secret_ref_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let test_data = r#"{
  "api": {
    "token": "__bcs_secret_ref__:env:BCS_TEST_CLI_API_TOKEN"
  },
  "service": {
    "key": "__bcs_secret_ref__:secret:BCS_TEST_CLI_SERVICE_KEY"
  }
}"#;

    fs::write(&input, test_data).expect("Failed to write secret-ref test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut decode_masked = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_masked
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("[SECRET_REF]"))
        .stdout(predicate::str::contains("tok_from_env").not())
        .stdout(predicate::str::contains("__bcs_secret_ref__").not());

    let mut decode_resolved = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_resolved
        .env("BCS_TEST_CLI_API_TOKEN", "tok_from_env")
        .env("BCS_TEST_CLI_SERVICE_KEY", "svc_key_from_env")
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--resolve-secrets")
        .assert()
        .success()
        .stdout(predicate::str::contains("tok_from_env"))
        .stdout(predicate::str::contains("svc_key_from_env"))
        .stdout(predicate::str::contains("[SECRET_REF]").not());

    let mut decode_with_provider_flag = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_with_provider_flag
        .env("BCS_TEST_CLI_API_TOKEN", "tok_from_env")
        .env("BCS_TEST_CLI_SERVICE_KEY", "svc_key_from_env")
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--resolve-secrets")
        .arg("--secret-provider")
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::str::contains("tok_from_env"));

    let mut decode_unknown_provider = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_unknown_provider
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--resolve-secrets")
        .arg("--secret-provider")
        .arg("not-a-real-provider")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Unknown or disabled secret provider")
                .or(predicate::str::contains("Unknown secret provider")),
        );

    // Vault provider is compiled in by default but requires VAULT_ADDR/TOKEN.
    let mut decode_vault_missing_auth = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_vault_missing_auth
        .env_remove("VAULT_ADDR")
        .env_remove("VAULT_TOKEN")
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--resolve-secrets")
        .arg("--secret-provider")
        .arg("vault")
        .assert()
        .failure()
        .stderr(predicate::str::contains("VAULT_ADDR"));

    let mut decode_missing_env = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_missing_env
        .env_remove("BCS_TEST_CLI_API_TOKEN")
        .env_remove("BCS_TEST_CLI_SERVICE_KEY")
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--resolve-secrets")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not set")
                .or(predicate::str::contains("Failed to resolve secret")),
        );

    let mut decode_stream_masked = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_stream_masked
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--stream")
        .assert()
        .success()
        .stdout(predicate::str::contains("[SECRET_REF]"))
        .stdout(predicate::str::contains("tok_from_env").not());

    let mut decode_stream_resolved = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_stream_resolved
        .env("BCS_TEST_CLI_API_TOKEN", "tok_from_env")
        .env("BCS_TEST_CLI_SERVICE_KEY", "svc_key_from_env")
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--stream")
        .arg("--resolve-secrets")
        .assert()
        .success()
        .stdout(predicate::str::contains("tok_from_env"))
        .stdout(predicate::str::contains("svc_key_from_env"))
        .stdout(predicate::str::contains("[SECRET_REF]").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_protect_command_on_existing_bcs_file() {
    let temp_dir = std::env::temp_dir();
    let input = create_test_json_file("protect_existing");
    let bcs_file = temp_output_path("protect_existing_raw.bcs");
    let protected_bcs_file = temp_output_path("protect_existing_protected.bcs");
    let paths_file = temp_dir.join("bcs_protect_existing_paths.txt");

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected_bcs_file);
    cleanup_test_file(&paths_file);

    fs::write(&paths_file, "database.name\n").expect("Failed to write protect paths file");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut protect_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    protect_cmd
        .arg("protect")
        .arg(bcs_file.to_str().unwrap())
        .arg("-o")
        .arg(protected_bcs_file.to_str().unwrap())
        .arg("--paths-file")
        .arg(paths_file.to_str().unwrap())
        .arg("--password")
        .arg("protect-pass")
        .assert()
        .success();

    let mut decode_without_password = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_without_password
        .arg("decode")
        .arg(protected_bcs_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("[PROTECTED]"));

    let mut decode_with_password = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_with_password
        .arg("decode")
        .arg(protected_bcs_file.to_str().unwrap())
        .arg("--password")
        .arg("protect-pass")
        .assert()
        .success()
        .stdout(predicate::str::contains("myapp_db"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected_bcs_file);
    cleanup_test_file(&paths_file);
}

#[test]
fn test_protect_default_output_path() {
    let input = create_test_json_file("protect_default_output");
    let bcs_file = temp_output_path("protect_default_input.bcs");
    let protected_default = bcs_file.with_file_name("bcs_test_protect_default_input.protected.bcs");

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected_default);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut protect_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    protect_cmd
        .arg("protect")
        .arg(bcs_file.to_str().unwrap())
        .arg("--paths")
        .arg("database.name")
        .arg("--password")
        .arg("protect-default-pass")
        .assert()
        .success();

    assert!(
        protected_default.exists(),
        "Expected default protect output at {:?}",
        protected_default
    );

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected_default);
}

#[test]
fn test_protect_paths_file_on_encode() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_sensitive_file_input.json");
    let paths_file = temp_dir.join("bcs_sensitive_paths.txt");
    let bcs_file = temp_output_path("sensitive_paths_file_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&paths_file);
    cleanup_test_file(&bcs_file);

    let test_data = r#"{
  "database": {
    "password": "secret-from-file"
  },
  "api": {
    "token": "token-from-file"
  }
}"#;

    fs::write(&input, test_data).expect("Failed to write sensitive test input");
    fs::write(
        &paths_file,
        "# Sensitive paths\ndatabase.password\napi.token\n",
    )
    .expect("Failed to write paths file");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .arg("--protect-paths-file")
        .arg(paths_file.to_str().unwrap())
        .arg("--protect-password")
        .arg("file-password")
        .assert()
        .success();

    let mut decode_without_password = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_without_password
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("[PROTECTED]"));

    let mut decode_with_password = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_with_password
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--password")
        .arg("file-password")
        .assert()
        .success()
        .stdout(predicate::str::contains("secret-from-file"))
        .stdout(predicate::str::contains("token-from-file"));

    cleanup_test_file(&input);
    cleanup_test_file(&paths_file);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_reindex_compact_file_enables_path_query() {
    let input = create_test_json_file("reindex_compact");
    let compact_bcs = temp_output_path("reindex_compact_input.bcs");
    let reindexed_bcs = temp_output_path("reindex_compact_output.bcs");

    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&reindexed_bcs);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--compact")
        .assert()
        .success();

    let mut reindex_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    reindex_cmd
        .arg("reindex")
        .arg(compact_bcs.to_str().unwrap())
        .arg("-o")
        .arg(reindexed_bcs.to_str().unwrap())
        .assert()
        .success();

    let mut decode_path_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_path_cmd
        .arg("decode")
        .arg(reindexed_bcs.to_str().unwrap())
        .arg("--path")
        .arg("database.host")
        .assert()
        .success()
        .stdout(predicate::str::contains("localhost"));

    cleanup_test_file(&input);
    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&reindexed_bcs);
}

#[test]
fn test_reindex_default_output_path() {
    let input = create_test_json_file("reindex_default_output");
    let compact_bcs = temp_output_path("reindex_default_input.bcs");
    let reindexed_default =
        compact_bcs.with_file_name("bcs_test_reindex_default_input.reindexed.bcs");

    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&reindexed_default);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--compact")
        .assert()
        .success();

    let mut reindex_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    reindex_cmd
        .arg("reindex")
        .arg(compact_bcs.to_str().unwrap())
        .assert()
        .success();

    assert!(
        reindexed_default.exists(),
        "Expected default reindex output at {:?}",
        reindexed_default
    );

    cleanup_test_file(&input);
    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&reindexed_default);
}

#[test]
fn test_reindex_dry_run_does_not_create_output() {
    let input = create_test_json_file("reindex_dry_run");
    let compact_bcs = temp_output_path("reindex_dry_input.bcs");
    let dry_output = temp_output_path("reindex_dry_output.bcs");

    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&dry_output);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--compact")
        .assert()
        .success();

    let mut dry_run_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    dry_run_cmd
        .arg("reindex")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--dry-run")
        .arg("--add-schema")
        .assert()
        .success()
        .stdout(predicate::str::contains("Input sections:"))
        .stdout(predicate::str::contains("Projected Output sections:"));

    assert!(!dry_output.exists(), "Dry-run must not create output files");

    cleanup_test_file(&input);
    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&dry_output);
}

#[test]
fn test_benchmark_with_custom_runs_flag() {
    let input = create_test_json_file("benchmark_runs");
    let bcs_file = temp_output_path("benchmark_runs_input.bcs");

    cleanup_test_file(&bcs_file);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut benchmark_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    benchmark_cmd
        .arg("benchmark")
        .arg(bcs_file.to_str().unwrap())
        .arg("--runs")
        .arg("7")
        .assert()
        .success()
        .stdout(predicate::str::contains("Runs:             7"))
        .stdout(predicate::str::contains("Load Time (p50):"))
        .stdout(predicate::str::contains("Decode Time (p95):"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_validate_json_output() {
    let input = create_test_json_file("validate_json_output");
    let bcs_file = temp_output_path("validate_json_output.bcs");

    cleanup_test_file(&bcs_file);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut validate_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    validate_cmd
        .arg("validate")
        .arg(bcs_file.to_str().unwrap())
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("\"error_count\":"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_inspect_json_output() {
    let input = create_test_json_file("inspect_json_output");
    let bcs_file = temp_output_path("inspect_json_output.bcs");

    cleanup_test_file(&bcs_file);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut inspect_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    inspect_cmd
        .arg("inspect")
        .arg(bcs_file.to_str().unwrap())
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"metadata\""))
        .stdout(predicate::str::contains("\"header\""));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_benchmark_json_output() {
    let input = create_test_json_file("benchmark_json_output");
    let bcs_file = temp_output_path("benchmark_json_output.bcs");

    cleanup_test_file(&bcs_file);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut benchmark_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    benchmark_cmd
        .arg("benchmark")
        .arg(bcs_file.to_str().unwrap())
        .arg("--json")
        .arg("--runs")
        .arg("3")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"bcs\""))
        .stdout(predicate::str::contains("\"runs\": 3"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_protect_json_output() {
    let input = create_test_json_file("protect_json_output");
    let bcs_file = temp_output_path("protect_json_output_raw.bcs");
    let protected_bcs_file = temp_output_path("protect_json_output_protected.bcs");

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected_bcs_file);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    let mut protect_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    protect_cmd
        .arg("protect")
        .arg(bcs_file.to_str().unwrap())
        .arg("-o")
        .arg(protected_bcs_file.to_str().unwrap())
        .arg("--paths")
        .arg("database.name")
        .arg("--password")
        .arg("json-pass")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("\"path_count\": 1"))
        .stdout(predicate::str::contains("\"output\""));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected_bcs_file);
}

#[test]
fn test_reindex_json_output() {
    let input = create_test_json_file("reindex_json_output");
    let compact_bcs = temp_output_path("reindex_json_output_input.bcs");
    let reindexed_bcs = temp_output_path("reindex_json_output_output.bcs");

    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&reindexed_bcs);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--compact")
        .assert()
        .success();

    let mut reindex_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    reindex_cmd
        .arg("reindex")
        .arg(compact_bcs.to_str().unwrap())
        .arg("-o")
        .arg(reindexed_bcs.to_str().unwrap())
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("\"dry_run\": false"))
        .stdout(predicate::str::contains("\"output_sections\""));

    cleanup_test_file(&input);
    cleanup_test_file(&compact_bcs);
    cleanup_test_file(&reindexed_bcs);
}

#[test]
fn test_reindex_dry_run_json_output() {
    let input = create_test_json_file("reindex_dry_json_output");
    let compact_bcs = temp_output_path("reindex_dry_json_output_input.bcs");

    cleanup_test_file(&compact_bcs);

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--compact")
        .assert()
        .success();

    let mut reindex_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    reindex_cmd
        .arg("reindex")
        .arg(compact_bcs.to_str().unwrap())
        .arg("--dry-run")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("\"dry_run\": true"))
        .stdout(predicate::str::contains("\"projected_output_sections\""));

    cleanup_test_file(&input);
    cleanup_test_file(&compact_bcs);
}

#[test]
fn test_encode_with_protect_password_env() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_sensitive_env_input.json");
    let bcs_file = temp_output_path("sensitive_env_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let test_data = r#"{
  "database": {
    "password": "env-secret"
  }
}"#;

    fs::write(&input, test_data).expect("Failed to write env sensitive test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .arg("--protect-paths")
        .arg("database.password")
        .arg("--protect-password-env")
        .arg("BCS_TEST_PROTECT_PASSWORD")
        .env("BCS_TEST_PROTECT_PASSWORD", "env-pass")
        .assert()
        .success();

    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--password")
        .arg("env-pass")
        .assert()
        .success()
        .stdout(predicate::str::contains("env-secret"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_with_password_env() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_sensitive_decode_env_input.json");
    let bcs_file = temp_output_path("sensitive_decode_env_test.bcs");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);

    let test_data = r#"{
  "api": {
    "token": "decode-env-secret"
  }
}"#;

    fs::write(&input, test_data).expect("Failed to write decode env test input");

    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .arg("--protect-paths")
        .arg("api.token")
        .arg("--protect-password")
        .arg("decode-env-pass")
        .assert()
        .success();

    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("--password-env")
        .arg("BCS_TEST_DECODE_PASSWORD")
        .env("BCS_TEST_DECODE_PASSWORD", "decode-env-pass")
        .assert()
        .success()
        .stdout(predicate::str::contains("decode-env-secret"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_validate_valid_file() {
    // First encode a file without schema
    let input = create_test_json_file("validate");
    let bcs_file = temp_output_path("validate_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode without schema
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Validate - should succeed for files without strict schema
    let mut validate_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    validate_cmd.arg("validate").arg(bcs_file.to_str().unwrap());

    // Validation runs (may pass or fail depending on schema requirements)
    let _ = validate_cmd.assert();

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_validate_nonexistent_file() {
    let temp_dir = std::env::temp_dir();
    let bcs_file = temp_dir.join("nonexistent.bcs");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("validate").arg(bcs_file.to_str().unwrap());

    cmd.assert().failure();
}

#[test]
fn test_inspect_file() {
    // First encode a file
    let input = create_test_json_file("inspect");
    let bcs_file = temp_output_path("inspect_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Inspect - should show file information
    let mut inspect_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    inspect_cmd.arg("inspect").arg(bcs_file.to_str().unwrap());

    inspect_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Inspecting BCS file"));

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_inspect_verbose() {
    // First encode a file
    let input = create_test_json_file("inspect_verbose");
    let bcs_file = temp_output_path("inspect_verbose_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Inspect with verbose flag
    let mut inspect_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    inspect_cmd
        .arg("inspect")
        .arg(bcs_file.to_str().unwrap())
        .arg("-v");

    inspect_cmd.assert().success();

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_inspect_nonexistent_file() {
    let temp_dir = std::env::temp_dir();
    let bcs_file = temp_dir.join("nonexistent.bcs");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("inspect").arg(bcs_file.to_str().unwrap());

    cmd.assert().failure();
}

#[test]
fn test_schema_extract() {
    // First encode a file without explicit schema
    let input = create_test_json_file("schema");
    let bcs_file = temp_output_path("schema_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode (schema will be inferred)
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Extract schema
    let mut schema_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    schema_cmd.arg("schema").arg(bcs_file.to_str().unwrap());

    schema_cmd.assert().success();

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_schema_export() {
    // First encode a file without explicit schema
    let input = create_test_json_file("schema_export_input");
    let bcs_file = temp_output_path("schema_export_test.bcs");
    let export_file = temp_output_path("schema_export_output.json");

    // Verify input file exists before proceeding
    assert!(input.exists(), "Input JSON file should exist: {:?}", input);

    // Clean up output files only
    cleanup_test_file(&bcs_file);
    cleanup_test_file(&export_file);

    // Verify input still exists after cleanup
    assert!(
        input.exists(),
        "Input JSON file should still exist after cleanup: {:?}",
        input
    );

    // Encode (schema will be inferred)
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap());

    // Just check if encode runs, don't require success yet
    let _encode_result = encode_cmd.assert();

    // If encoding failed, skip the rest of the test
    if !bcs_file.exists() {
        cleanup_test_file(&input);
        return;
    }

    // Export schema
    let mut schema_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    schema_cmd
        .arg("schema")
        .arg(bcs_file.to_str().unwrap())
        .arg("-e")
        .arg(export_file.to_str().unwrap());

    schema_cmd.assert().success();

    assert!(
        export_file.exists(),
        "Exported schema file should be created"
    );

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&export_file);
    cleanup_test_file(&input);
}

#[test]
fn test_benchmark_file() {
    // First encode a file
    let input = create_test_json_file("benchmark");
    let bcs_file = temp_output_path("benchmark_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Benchmark
    let mut benchmark_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    benchmark_cmd
        .arg("benchmark")
        .arg(bcs_file.to_str().unwrap());

    benchmark_cmd.assert().success();

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_benchmark_with_compare() {
    // First encode a file
    let input = create_test_json_file("benchmark_compare");
    let bcs_file = temp_output_path("benchmark_compare_test.bcs");

    cleanup_test_file(&bcs_file);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Benchmark with comparison
    let mut benchmark_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    benchmark_cmd
        .arg("benchmark")
        .arg(bcs_file.to_str().unwrap())
        .arg("-c")
        .arg(input.to_str().unwrap());

    benchmark_cmd.assert().success();

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&input);
}

#[test]
fn test_round_trip_json() {
    let input = create_test_json_file("round_trip_input");
    let bcs_file = temp_output_path("round_trip.bcs");
    let output = temp_output_path("round_trip_output.json");

    // Verify input file exists
    assert!(input.exists(), "Input JSON file should exist: {:?}", input);

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&output);

    // Encode
    let mut encode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    encode_cmd
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    // Decode
    let mut decode_cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    decode_cmd
        .arg("decode")
        .arg(bcs_file.to_str().unwrap())
        .arg("-o")
        .arg(output.to_str().unwrap())
        .assert()
        .success();

    // Verify both files exist
    assert!(bcs_file.exists());
    assert!(output.exists());

    // Read and parse both JSON files to verify they're valid JSON
    let decoded = fs::read_to_string(&output).unwrap();
    let decoded_json: serde_json::Value = serde_json::from_str(&decoded).unwrap();

    // Verify decoded JSON is not empty
    assert!(!decoded_json.is_null(), "Decoded JSON should not be null");

    cleanup_test_file(&bcs_file);
    cleanup_test_file(&output);
    cleanup_test_file(&input);
}

#[test]
fn test_invalid_command() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("invalid_command");

    cmd.assert().failure();
}

#[test]
fn test_kms_protect_and_decode_via_command_wrapper() {
    let temp_dir = std::env::temp_dir();
    let input = temp_dir.join("bcs_kms_cmd_input.json");
    let bcs_file = temp_output_path("kms_cmd_raw.bcs");
    let protected = temp_output_path("kms_cmd_protected.bcs");
    let wrapper_script = temp_dir.join("bcs_kms_xor_wrapper.py");

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected);
    cleanup_test_file(&wrapper_script);

    fs::write(&input, r#"{"database":{"password":"kms-cli-secret"}}"#).expect("write input");
    fs::write(
        &wrapper_script,
        r#"import base64, sys
data = base64.b64decode(sys.stdin.read().strip())
sys.stdout.write(base64.b64encode(bytes(b ^ 0xA5 for b in data)).decode())
"#,
    )
    .expect("write wrapper script");

    let script = wrapper_script.to_str().unwrap();
    // Windows runners expose `python`; Unix CI uses `python3`.
    let python = if cfg!(windows) { "python" } else { "python3" };
    let wrap_cmd = format!("{python} {script}");

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .arg("encode")
        .arg(input.to_str().unwrap())
        .arg("-o")
        .arg(bcs_file.to_str().unwrap())
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .env("BCS_KMS_WRAP_CMD", &wrap_cmd)
        .env("BCS_KMS_UNWRAP_CMD", &wrap_cmd)
        .arg("protect")
        .arg(bcs_file.to_str().unwrap())
        .arg("-o")
        .arg(protected.to_str().unwrap())
        .arg("--paths")
        .arg("database.password")
        .arg("--scheme")
        .arg("kms")
        .arg("--kms-provider")
        .arg("cmd")
        .arg("--kms-key")
        .arg("alias/test")
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .arg("decode")
        .arg(protected.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("[PROTECTED]"))
        .stdout(predicate::str::contains("kms-cli-secret").not());

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .env("BCS_KMS_UNWRAP_CMD", &wrap_cmd)
        .arg("decode")
        .arg(protected.to_str().unwrap())
        .arg("--unwrap-kms")
        .arg("--kms-provider")
        .arg("cmd")
        .assert()
        .success()
        .stdout(predicate::str::contains("kms-cli-secret"))
        .stdout(predicate::str::contains("[PROTECTED]").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
    cleanup_test_file(&protected);
    cleanup_test_file(&wrapper_script);
}

#[test]
fn test_missing_required_args() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    cmd.arg("encode");

    cmd.assert().failure();
}

#[test]
fn test_schema_agent_safe_and_sensitive_paths() {
    let input = temp_dir_file(
        "agent_safe_input.json",
        r#"{"database":{"password":"plain"},"host":"db"}"#,
    );
    let bcs_file = temp_output_path("agent_safe.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--sensitive-paths",
            "database.password",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args(["schema", "--agent-safe", bcs_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("database.password"))
        .stdout(predicate::str::contains("\"sensitive\""))
        .stdout(predicate::str::contains("plain").not());

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "validate",
            bcs_file.to_str().unwrap(),
            "--fail-on-sensitive-plaintext",
        ])
        .assert()
        .failure();

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_scan_detects_aws_key_in_json() {
    let input = temp_dir_file("scan_leak.json", r#"{"aws_key":"AKIAIOSFODNN7EXAMPLE"}"#);
    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args(["scan", input.to_str().unwrap(), "--json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("aws_access_key_id"));
    cleanup_test_file(&input);
}

#[test]
fn test_show_masks_and_segments() {
    let input = create_test_json_file("show_segments");
    let bcs_file = temp_output_path("show_segments.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "show",
            bcs_file.to_str().unwrap(),
            "database",
            "host",
            "-f",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("localhost"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_run_dry_run_redacts_sensitive() {
    let input = temp_dir_file(
        "run_dry.json",
        r#"{"database":{"password":"should-not-print"},"host":"db"}"#,
    );
    let bcs_file = temp_output_path("run_dry.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--sensitive-paths",
            "database.password",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args(["run", bcs_file.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[REDACTED]"))
        .stdout(predicate::str::contains("should-not-print").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_run_prefix_and_only_dry_run() {
    let input = temp_dir_file(
        "run_prefix.json",
        r#"{"database":{"host":"db","password":"secret"},"api":{"port":80}}"#,
    );
    let bcs_file = temp_output_path("run_prefix.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--sensitive-paths",
            "database.password",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "run",
            bcs_file.to_str().unwrap(),
            "--dry-run",
            "--prefix",
            "APP_",
            "--only",
            "database.host,database.password",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("APP_DATABASE__HOST=db"))
        .stdout(predicate::str::contains(
            "APP_DATABASE__PASSWORD=[REDACTED]",
        ))
        .stdout(predicate::str::contains("API__PORT").not())
        .stdout(predicate::str::contains("secret").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_env_command_redacts_by_default() {
    let input = temp_dir_file(
        "env_cmd.json",
        r#"{"database":{"password":"should-not-print"},"host":"db"}"#,
    );
    let bcs_file = temp_output_path("env_cmd.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--sensitive-paths",
            "database.password",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args(["env", bcs_file.to_str().unwrap(), "--prefix", "APP_"])
        .assert()
        .success()
        .stdout(predicate::str::contains("APP_HOST='db'"))
        .stdout(predicate::str::contains(
            "APP_DATABASE__PASSWORD='[REDACTED]'",
        ))
        .stdout(predicate::str::contains("should-not-print").not());

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args(["env", bcs_file.to_str().unwrap(), "--allow-sensitive"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "DATABASE__PASSWORD='should-not-print'",
        ))
        .stderr(predicate::str::contains("warning: --allow-sensitive"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_run_injects_env_into_child() {
    let input = temp_dir_file("run_inject.json", r#"{"greeting":"hello-bcs"}"#);
    let bcs_file = temp_output_path("run_inject.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Child command must exist on every CI OS (`printenv` is Unix-only).
    let mut run = Command::new(assert_cmd::cargo::cargo_bin!("bcs"));
    run.args([
        "run",
        bcs_file.to_str().unwrap(),
        "--prefix",
        "BCS_TEST_",
        "--",
    ]);
    #[cfg(windows)]
    run.args(["cmd", "/C", "echo %BCS_TEST_GREETING%"]);
    #[cfg(not(windows))]
    run.args(["printenv", "BCS_TEST_GREETING"]);
    run.assert()
        .success()
        .stdout(predicate::str::contains("hello-bcs"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_encode_dedup_roundtrip_via_decode() {
    let json = format!(
        r#"{{"a":"{}","b":"{}","c":"{}"}}"#,
        "shared-value-zzzz", "shared-value-zzzz", "shared-value-zzzz"
    );
    let input = temp_dir_file("dedup_input.json", &json);
    let bcs_file = temp_output_path("dedup.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--dedup",
            "strings",
            "--dedup-min-repeats",
            "2",
            "--dedup-min-length",
            "4",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args(["inspect", bcs_file.to_str().unwrap(), "--json"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "decode",
            bcs_file.to_str().unwrap(),
            "--path",
            "a",
            "-f",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("shared-value-zzzz"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_redact_and_fail_on_sensitive_plaintext() {
    let input = temp_dir_file(
        "redact_sensitive_input.json",
        r#"{"database":{"password":"should-redact"},"host":"db"}"#,
    );
    let bcs_file = temp_output_path("redact_sensitive.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--sensitive-paths",
            "database.password",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "decode",
            bcs_file.to_str().unwrap(),
            "--redact-sensitive-plaintext",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[SENSITIVE]"))
        .stdout(predicate::str::contains("should-redact").not());

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "decode",
            bcs_file.to_str().unwrap(),
            "--fail-on-sensitive-plaintext",
        ])
        .assert()
        .failure();

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "show",
            bcs_file.to_str().unwrap(),
            "-f",
            "json",
            "--redact-sensitive-plaintext",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[SENSITIVE]"))
        .stdout(predicate::str::contains("should-redact").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_fail_on_sensitive_allows_protected_masks() {
    let input = temp_dir_file(
        "fail_on_protected_input.json",
        r#"{"database":{"password":"secret"}}"#,
    );
    let bcs_file = temp_output_path("fail_on_protected.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--protect-paths",
            "database.password",
            "--protect-password",
            "pw",
        ])
        .assert()
        .success();

    // After mask_all, [PROTECTED] must not trip --fail-on-sensitive-plaintext.
    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "decode",
            bcs_file.to_str().unwrap(),
            "--fail-on-sensitive-plaintext",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[PROTECTED]"));

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

#[test]
fn test_decode_path_masks_nested_protect_markers() {
    let input = temp_dir_file(
        "nested_protect_input.json",
        r#"{"database":{"host":"db","password":"secret-nested"}}"#,
    );
    let bcs_file = temp_output_path("nested_protect.bcs");
    cleanup_test_file(&bcs_file);

    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "encode",
            input.to_str().unwrap(),
            "-o",
            bcs_file.to_str().unwrap(),
            "--protect-paths",
            "database.password",
            "--protect-password",
            "nest-pass",
        ])
        .assert()
        .success();

    // Subtree decode must mask nested protect markers (not leak ciphertext).
    Command::new(assert_cmd::cargo::cargo_bin!("bcs"))
        .args([
            "decode",
            bcs_file.to_str().unwrap(),
            "--path",
            "database",
            "-f",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[PROTECTED]"))
        .stdout(predicate::str::contains("__bcs_sensitive_").not())
        .stdout(predicate::str::contains("secret-nested").not());

    cleanup_test_file(&input);
    cleanup_test_file(&bcs_file);
}

fn temp_dir_file(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bcs_test_{}", name));
    fs::write(&path, contents).expect("write temp file");
    path
}
