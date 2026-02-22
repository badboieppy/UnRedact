use clap::Parser;
use std::path::{Path, PathBuf};

use unredact::service::tooling_entry::ToolingPdfRenderer;

#[derive(Debug, Parser)]
#[command(
    name = "pdf_to_png",
    about = "Render PDF pages to PNG files using the hayro renderer"
)]
struct Args {
    input: PathBuf,

    /// Render this 1-based page number when --all-pages is not set.
    #[arg(long, default_value_t = 1)]
    page: usize,

    /// Render all pages in the PDF.
    #[arg(long, default_value_t = false)]
    all_pages: bool,

    /// Output PNG path for single-page rendering.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Output directory. Defaults to the input file directory.
    #[arg(long)]
    output_dir: Option<PathBuf>,

    #[arg(long, default_value_t = 200.0_f32)]
    dpi: f32,
}

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
    let args = Args::parse();

    if !args.dpi.is_finite() || args.dpi <= 0.0 {
        return Err(format!("invalid --dpi value: {}", args.dpi));
    }
    if args.page == 0 {
        return Err("--page must be >= 1".to_owned());
    }
    if args.all_pages && args.output.is_some() {
        return Err("--output cannot be used with --all-pages".to_owned());
    }

    let pdf_bytes = std::fs::read(&args.input)
        .map_err(|e| format!("failed to read input pdf {}: {e}", args.input.display()))?;
    let renderer = ToolingPdfRenderer::new_from_bytes(&pdf_bytes)?;
    let page_count = renderer.page_count();
    if page_count == 0 {
        return Err("input PDF has zero pages".to_owned());
    }

    if args.all_pages {
        let output_dir = args
            .output_dir
            .clone()
            .unwrap_or_else(|| default_output_dir(&args.input));
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| format!("failed to create output dir {}: {e}", output_dir.display()))?;
        for page_index in 0..page_count {
            let rendered = renderer.render_page_to_rgba(page_index, args.dpi)?;
            let path = default_all_pages_path(&args.input, &output_dir, page_index + 1);
            write_png(
                &path,
                rendered.width_px,
                rendered.height_px,
                rendered.pixels,
            )?;
            println!("{}", path.display());
        }
        return Ok(());
    }

    let page_index = args.page - 1;
    if page_index >= page_count {
        return Err(format!(
            "page out of bounds: requested={} page_count={}",
            args.page, page_count
        ));
    }
    let rendered = renderer.render_page_to_rgba(page_index, args.dpi)?;
    let output_path = if let Some(path) = args.output {
        path
    } else {
        let output_dir = args
            .output_dir
            .clone()
            .unwrap_or_else(|| default_output_dir(&args.input));
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| format!("failed to create output dir {}: {e}", output_dir.display()))?;
        default_single_page_path(&args.input, &output_dir, args.page)
    };
    write_png(
        &output_path,
        rendered.width_px,
        rendered.height_px,
        rendered.pixels,
    )?;
    println!("{}", output_path.display());
    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, pixels: Vec<u8>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output dir {}: {e}", parent.display()))?;
        }
    }
    let image = image::RgbaImage::from_raw(width, height, pixels).ok_or_else(|| {
        format!(
            "invalid RGBA buffer size for image dimensions {}x{}",
            width, height
        )
    })?;
    image
        .save(path)
        .map_err(|e| format!("failed to write png {}: {e}", path.display()))
}

pub fn default_output_dir(input: &Path) -> PathBuf {
    input
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn default_single_page_path(input: &Path, output_dir: &Path, page: usize) -> PathBuf {
    let stem = file_stem_or_default(input);
    if page == 1 {
        output_dir.join(format!("{stem}.png"))
    } else {
        output_dir.join(format!("{stem}.page_{page}.png"))
    }
}

pub fn default_all_pages_path(input: &Path, output_dir: &Path, page: usize) -> PathBuf {
    let stem = file_stem_or_default(input);
    output_dir.join(format!("{stem}.p{page:03}.png"))
}

fn file_stem_or_default(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("output")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::default_all_pages_path;
    use super::default_single_page_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn single_page_default_path_uses_plain_png_for_page_one() {
        let out = default_single_page_path(Path::new("docs/sample.pdf"), Path::new("out"), 1);
        assert_eq!(out, PathBuf::from("out/sample.png"));
    }

    #[test]
    fn single_page_default_path_includes_page_for_non_first_page() {
        let out = default_single_page_path(Path::new("docs/sample.pdf"), Path::new("out"), 3);
        assert_eq!(out, PathBuf::from("out/sample.page_3.png"));
    }

    #[test]
    fn all_pages_default_path_is_zero_padded() {
        let out = default_all_pages_path(Path::new("docs/sample.pdf"), Path::new("out"), 7);
        assert_eq!(out, PathBuf::from("out/sample.p007.png"));
    }
}
