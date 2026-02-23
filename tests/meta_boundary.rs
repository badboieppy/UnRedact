use std::path::Path;

const DISALLOWED_IMPORT_PREFIXES: [&str; 3] = [
    "use unredact::logic::",
    "use unredact::dependency::",
    "use unredact::data::",
];

#[test]
fn integration_tests_only_use_service_and_public_types_interfaces() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut violations = Vec::<String>::new();

    let entries = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", tests_dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read test entry: {error}"));
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|value| value.to_str())
            == Some("meta_boundary.rs")
        {
            continue;
        }

        let content = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read integration test {}: {error}",
                path.display()
            )
        });

        for prefix in DISALLOWED_IMPORT_PREFIXES {
            for (index, line) in content.lines().enumerate() {
                if line.trim_start().starts_with(prefix) {
                    violations.push(format!(
                        "{}:{} uses disallowed import `{}`",
                        path.display(),
                        index + 1,
                        prefix
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "integration tests must stay black-box at the service boundary:\n{}",
        violations.join("\n")
    );
}

#[test]
fn source_code_architecture_layers_are_respected() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::<String>::new();

    // Define allowed dependencies for each layer.
    // Keys are the layer names (subdirectories in src/).
    // Values are the list of other layers they can import.
    let layers = [
        ("service", vec!["logic", "types"]),
        ("logic", vec!["data", "types"]),
        ("data", vec!["dependency", "types"]),
        ("dependency", vec!["types"]),
        ("types", vec![]),
    ];

    for (layer_name, allowed_deps) in layers {
        let layer_path = src_dir.join(layer_name);
        if !layer_path.exists() {
            continue;
        }

        let mut stack = vec![layer_path];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()));

            for entry in entries {
                let entry =
                    entry.unwrap_or_else(|error| panic!("failed to read src entry: {error}"));
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }

                if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                    continue;
                }

                let content = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("failed to read source file {}: {error}", path.display())
                });

                for (line_idx, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    if !trimmed.starts_with("use crate::") {
                        continue;
                    }

                    // Check what module is being imported
                    // Format: use crate::module::... or use crate::module;
                    let parts: Vec<&str> = trimmed
                        .trim_start_matches("use crate::")
                        .split("::")
                        .collect();

                    if parts.is_empty() {
                        continue;
                    }

                    // Handle simple imports; complex grouped imports are out of scope for this simple check
                    // but usually frowned upon in strict layering anyway.
                    let imported_module_raw = parts[0];
                    let imported_module = imported_module_raw
                         .trim_end_matches(';')
                         .trim_end_matches(',')
                         .trim_start_matches('{'); // Handle basic single-item groups if any

                    // We only care about imports from our main architectural layers
                    let known_layers = ["service", "logic", "data", "dependency", "types"];
                    if !known_layers.contains(&imported_module) {
                        continue;
                    }

                    if imported_module == layer_name {
                        continue;
                    }

                    if !allowed_deps.contains(&imported_module) {
                        violations.push(format!(
                            "{}:{} in layer '{}' illegally imports '{}' (allowed: {:?})",
                            path.strip_prefix(&src_dir).unwrap().display(),
                            line_idx + 1,
                            layer_name,
                            imported_module,
                            allowed_deps
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "architectural layer violations found:\n{}",
        violations.join("\n")
    );
}
