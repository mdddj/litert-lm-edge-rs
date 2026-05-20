use litert_lm_edge::{KnownModel, ModelDownloader};
use std::io::{self, Write};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model = args
        .next()
        .as_deref()
        .map(parse_model)
        .transpose()?
        .unwrap_or(KnownModel::Gemma4E2B);
    let output_dir = args.next().unwrap_or_else(|| "models".to_owned());

    let path =
        ModelDownloader::new().download_to_dir_with_progress(model, output_dir, |progress| {
            if let Some(total) = progress.total_bytes {
                let percent = progress.downloaded_bytes as f64 * 100.0 / total as f64;
                eprint!(
                    "\rDownloading {:.1}% ({}/{})",
                    percent,
                    human_bytes(progress.downloaded_bytes),
                    human_bytes(total)
                );
            } else {
                eprint!("\rDownloading {}", human_bytes(progress.downloaded_bytes));
            }
            io::stderr().flush().ok();
        })?;

    eprintln!();
    println!("{}", path.display());
    Ok(())
}

fn parse_model(value: &str) -> litert_lm_edge::Result<KnownModel> {
    match value {
        "e2b" | "E2B" | "gemma-4-e2b" | "gemma-4-E2B" => Ok(KnownModel::Gemma4E2B),
        "e4b" | "E4B" | "gemma-4-e4b" | "gemma-4-E4B" => Ok(KnownModel::Gemma4E4B),
        _ => Err(litert_lm_edge::Error::InvalidResponse(format!(
            "unknown model {value}; use e2b or e4b"
        ))),
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for next_unit in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }
    format!("{value:.1} {unit}")
}
