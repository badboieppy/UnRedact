use std::fs;
use std::path::Path;

use unredact::redaction_finder::dependency::hayro_renderer::HayroRenderer;
use unredact::redaction_finder::types::PdfRenderer;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let base = Path::new("example");
    let inputs = [
        (base.join("EFTA02238592.pdf"), base.join("EFTA02238592.png")),
        (
            base.join("EFTA02238592.visualized.pdf"),
            base.join("EFTA02238592.visualized.png"),
        ),
    ];

    for (input, output) in inputs {
        render_first_page(&input, &output)?;
    }

    Ok(())
}

fn render_first_page(input: &Path, output: &Path) -> Result<(), String> {
    let bytes = fs::read(input)
        .map_err(|e| format!("failed to read {}: {e}", input.display()))?;
    let renderer =
        HayroRenderer::new_from_bytes(&bytes).map_err(|e| format!("render init failed: {e}"))?;
    let page = renderer
        .render_page_to_rgba(0, 150.0)
        .map_err(|e| format!("render failed: {e}"))?;
    image::save_buffer(
        output,
        &page.pixels,
        page.width_px,
        page.height_px,
        image::ColorType::Rgba8,
    )
    .map_err(|e| format!("failed to write {}: {e}", output.display()))?;
    Ok(())
}
