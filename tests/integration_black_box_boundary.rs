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
            == Some("integration_black_box_boundary.rs")
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
