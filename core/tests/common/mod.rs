use std::fs;
use std::path::PathBuf;

/// Absolute path to the workspace `examples/` directory.
pub fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples")
}

/// Path to a fixture under workspace `examples/`.
pub fn example_path(name: &str) -> PathBuf {
    let path = examples_dir().join(name);
    assert!(
        path.is_file(),
        "missing example fixture at {} (expected under workspace examples/)",
        path.display()
    );
    path
}

/// Read a fixture from workspace `examples/`.
pub fn read_example(name: &str) -> String {
    fs::read_to_string(example_path(name))
        .unwrap_or_else(|e| panic!("failed to read example fixture {}: {}", name, e))
}
