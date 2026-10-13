//! Cross-modal retrieval check for an EmbeddingGemma 2 bundle.
//!
//! Text-only output cannot show whether the vision and audio encoders work, so
//! this embeds each supplied medium and ranks text probes against it. A working
//! multimodal pipeline ranks the probe that actually describes the medium
//! first; a pipeline that silently ignores the medium returns probes in an
//! arbitrary order. The closing matrix compares the media against each other:
//! if the encoders were dead, every medium would collapse onto the same vector
//! and the off-diagonal entries would sit near 1.0.
//!
//! ```text
//! cargo run -p litert-lm-edge --example embedding_multimodal -- \
//!   "$EMBEDDING_MODEL" \
//!   --image /path/to/photo.jpg \
//!   --audio /path/to/speech.wav
//! ```
//!
//! `--image` and `--audio` may be repeated to compare several media at once.

use litert_lm_edge::{EmbeddingEngine, EmbeddingOptions, EmbeddingTask, InputData};

/// Probes chosen to cover the fixtures upstream ships in `runtime/testdata`
/// and `support/preprocessor/testdata`, so the default invocation is
/// meaningful without extra arguments.
const DEFAULT_PROBES: [&str; 5] = [
    "a photo of a red apple in a wooden bowl",
    "a green rectangle on a white background",
    "a person speaking the words have a wonderful day",
    "a chart of falling stock prices",
    "an empty quiet room",
];

struct Args {
    model_path: String,
    images: Vec<String>,
    audios: Vec<String>,
    probes: Vec<String>,
}

fn parse_args() -> Args {
    let mut model_path = None;
    let mut images = Vec::new();
    let mut audios = Vec::new();
    let mut probes = Vec::new();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--image" => images.extend(args.next()),
            "--audio" => audios.extend(args.next()),
            "--probe" => {
                if let Some(probe) = args.next() {
                    probes.push(probe);
                }
            }
            other if model_path.is_none() => model_path = Some(other.to_owned()),
            other => {
                eprintln!("unexpected argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let model_path = model_path.unwrap_or_else(|| {
        eprintln!(
            "usage: embedding_multimodal <model.litertlm> \
             [--image PATH]... [--audio PATH]... [--probe TEXT]..."
        );
        std::process::exit(2);
    });

    Args {
        model_path,
        images,
        audios,
        probes,
    }
}

fn main() -> litert_lm_edge::Result<()> {
    let args = parse_args();
    let probes = if args.probes.is_empty() {
        DEFAULT_PROBES.map(str::to_owned).to_vec()
    } else {
        args.probes.clone()
    };

    let mut engine = EmbeddingEngine::builder(&args.model_path)
        .num_threads(4)
        .build()?;
    let options = EmbeddingOptions::new().normalize(true);

    // Text probes are queries, so they carry the query prefix. The media
    // themselves are raw bytes and take no prefix.
    let probe_requests: Vec<Vec<InputData>> = probes
        .iter()
        .map(|probe| vec![InputData::text(EmbeddingTask::Query.prompt(probe))])
        .collect();
    let probe_vectors = engine.compute_embedding_batch_with_options(&probe_requests, options)?;
    println!(
        "model: {}\nembedding dimension: {}\n",
        args.model_path,
        probe_vectors.first().map_or(0, Vec::len)
    );

    if args.images.is_empty() && args.audios.is_empty() {
        println!("no media supplied; pass --image and/or --audio to test a modality");
        return Ok(());
    }

    let mut embedded: Vec<(String, Vec<f32>)> = Vec::new();
    for path in &args.images {
        let vector = rank_image(path, &mut engine, options, &probes, &probe_vectors)?;
        embedded.push((format!("image {path}"), vector));
    }
    for path in &args.audios {
        let vector = rank_audio(path, &mut engine, options, &probes, &probe_vectors)?;
        embedded.push((format!("audio {path}"), vector));
    }

    if embedded.len() > 1 {
        print_similarity_matrix(&embedded);
    }

    Ok(())
}

fn rank_image(
    path: &str,
    engine: &mut EmbeddingEngine,
    options: EmbeddingOptions,
    probes: &[String],
    probe_vectors: &[Vec<f32>],
) -> litert_lm_edge::Result<Vec<f32>> {
    // EmbeddingGemma 2 is trained with a learnable end-of-image embedding, so
    // the marker is part of the expected input format rather than a delimiter
    // this wrapper invented.
    let media = vec![InputData::image_file(path), InputData::ImageEnd];
    rank("image", path, media, engine, options, probes, probe_vectors)
}

fn rank_audio(
    path: &str,
    engine: &mut EmbeddingEngine,
    options: EmbeddingOptions,
    probes: &[String],
    probe_vectors: &[Vec<f32>],
) -> litert_lm_edge::Result<Vec<f32>> {
    let media = vec![InputData::audio_file(path), InputData::AudioEnd];
    rank("audio", path, media, engine, options, probes, probe_vectors)
}

/// Embeds one medium, prints the probes ordered by similarity to it, and
/// returns the medium's own vector for the cross-media comparison.
#[allow(clippy::too_many_arguments)]
fn rank(
    kind: &str,
    path: &str,
    media: Vec<InputData>,
    engine: &mut EmbeddingEngine,
    options: EmbeddingOptions,
    probes: &[String],
    probe_vectors: &[Vec<f32>],
) -> litert_lm_edge::Result<Vec<f32>> {
    let vector = engine.compute_embedding_with_options(&media, options)?;
    println!("{kind}: {path}  ({} dimensions)", vector.len());

    let mut scored: Vec<(f32, &str)> = probe_vectors
        .iter()
        .zip(probes.iter())
        .map(|(probe_vector, probe)| (cosine_similarity(&vector, probe_vector), probe.as_str()))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    for (score, probe) in scored {
        println!("  {score:.4}  {probe}");
    }
    println!();

    Ok(vector)
}

/// Prints pairwise cosine similarity between the media themselves. Distinct
/// media must land on distinct vectors, so a healthy run keeps every
/// off-diagonal entry well below 1.0.
fn print_similarity_matrix(embedded: &[(String, Vec<f32>)]) {
    // Indent every label under a short numeric index so the table stays narrow
    // even when the fixture paths are long.
    println!("pairwise similarity between media:");
    for (index, (label, _)) in embedded.iter().enumerate() {
        println!("  [{index}] {label}");
    }
    for i in 0..embedded.len() {
        for j in (i + 1)..embedded.len() {
            let score = cosine_similarity(&embedded[i].1, &embedded[j].1);
            println!("  [{i}] <-> [{j}]: {score:.4}");
        }
    }
    println!();
}

/// Returns 0.0 when the shapes disagree or either vector is degenerate, so a
/// broken embedding shows up as a score of 0 rather than a NaN.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}
