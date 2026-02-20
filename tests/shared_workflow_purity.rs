use std::path::Path;

const SHARED_WORKFLOW_FILES: [&str; 8] = [
    "src/logic/dictionary_list_convertion_component.rs",
    "src/logic/file_byte_convertion_component.rs",
    "src/logic/redaction_guessing_component.rs",
    "src/logic/types/mod.rs",
    "src/data/dictionary_data.rs",
    "src/data/fonts_data.rs",
    "src/data/redactions_data.rs",
    "src/data/visualization_data.rs",
];

const DISALLOWED_PATTERNS: [&str; 6] = [
    "std::fs",
    "use std::path::Path",
    "PathBuf",
    "clap::",
    "wasm_bindgen",
    "serde_wasm_bindgen",
];

#[test]
fn shared_workflow_sources_avoid_native_only_dependencies() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::<String>::new();
    for relative in SHARED_WORKFLOW_FILES {
        let source_path = repo_root.join(relative);
        let source = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source_path.display()));
        let non_test_source = strip_cfg_test_modules(&source);
        for pattern in DISALLOWED_PATTERNS {
            for (index, line) in non_test_source.lines().enumerate() {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{}:{} contains disallowed pattern `{}`",
                        relative,
                        index + 1,
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "shared workflow purity violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn web_entry_does_not_reference_local_file_workflow_exports() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/service/unredact_web_entry.rs");
    let source = std::fs::read_to_string(&source_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", source_path.display()));
    let disallowed_tokens = [
        "run_from_paths",
        "run_batch_from_paths",
        "read_input_pdf_bytes",
        "write_encoded_outputs",
        "OutputFilePaths",
        "FileStore",
    ];

    let mut violations = Vec::<String>::new();
    for token in disallowed_tokens {
        if source.contains(token) {
            violations.push(token.to_owned());
        }
    }

    assert!(
        violations.is_empty(),
        "web entry references local-only workflow tokens: {}",
        violations.join(", ")
    );
}

fn strip_cfg_test_modules(source: &str) -> String {
    let mut out = Vec::<String>::new();
    let mut in_test_module = false;
    let mut test_module_depth: i32 = 0;
    let mut pending_cfg_test = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if in_test_module {
            test_module_depth += line.matches('{').count() as i32;
            test_module_depth -= line.matches('}').count() as i32;
            if test_module_depth <= 0 {
                in_test_module = false;
                test_module_depth = 0;
            }
            continue;
        }

        if trimmed.starts_with("#[cfg(test)]") {
            pending_cfg_test = true;
            continue;
        }

        if pending_cfg_test {
            if trimmed.starts_with("mod ") && trimmed.ends_with('{') {
                in_test_module = true;
                test_module_depth = line.matches('{').count() as i32;
                test_module_depth -= line.matches('}').count() as i32;
                if test_module_depth <= 0 {
                    in_test_module = false;
                    test_module_depth = 0;
                }
                pending_cfg_test = false;
                continue;
            }

            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                pending_cfg_test = false;
            }
        }

        out.push(line.to_owned());
    }

    out.join("\n")
}
