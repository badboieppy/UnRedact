#[test]
fn web_ui_batch_benchmark_opt_in() {
    let should_run = std::env::var("UNREDACT_RUN_WEB_UI_BENCHMARK")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !should_run {
        eprintln!("skipping web ui benchmark test; set UNREDACT_RUN_WEB_UI_BENCHMARK=1 to run it");
        return;
    }

    let npm = if cfg!(target_os = "windows") {
        "npm.cmd"
    } else {
        "npm"
    };

    let status = std::process::Command::new(npm)
        .args(["run", "test:web-ui-benchmark"])
        .status()
        .unwrap_or_else(|error| panic!("failed to launch npm for web ui benchmark test: {error}"));
    assert!(
        status.success(),
        "web ui benchmark test failed with status: {status}"
    );
}
