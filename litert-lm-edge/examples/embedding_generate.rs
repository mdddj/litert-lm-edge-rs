use litert_lm_edge::{EmbeddingEngine, EmbeddingOptions, EmbeddingTask, InputData};

const DEFAULT_QUERY: &str = "a cat resting on a window";
const DEFAULT_DOCUMENTS: [&str; 3] = [
    "A kitten is dozing on the windowsill in the afternoon sun.",
    "The stock market fell sharply today amid rate concerns.",
    "A grey cat sits on a wooden window ledge, watching birds.",
];

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args.next().unwrap_or_else(|| {
        panic!("usage: embedding_generate <model.litertlm> [query] [document...]")
    });
    let query = args.next().unwrap_or_else(|| DEFAULT_QUERY.to_owned());
    let documents: Vec<String> = args.collect();
    let documents = if documents.is_empty() {
        DEFAULT_DOCUMENTS.map(str::to_owned).to_vec()
    } else {
        documents
    };

    let mut engine = EmbeddingEngine::builder(model_path)
        .num_threads(4)
        .build()?;

    // normalize(true) makes the dot product equal cosine similarity. The task
    // prefixes differ for queries and documents on purpose: EmbeddingGemma 2 is
    // trained with asymmetric prefixes, and using one prefix for both sides
    // noticeably degrades retrieval quality.
    let options = EmbeddingOptions::new().normalize(true);

    let query_vector = engine.compute_embedding_with_options(
        &[InputData::text(EmbeddingTask::Query.prompt(&query))],
        options,
    )?;
    println!("query: {query}");
    println!("embedding dimension: {}", query_vector.len());

    let requests: Vec<Vec<InputData>> = documents
        .iter()
        .map(|document| vec![InputData::text(EmbeddingTask::Document.prompt(document))])
        .collect();
    let document_vectors = engine.compute_embedding_batch_with_options(&requests, options)?;

    let mut scored: Vec<(f32, &str)> = document_vectors
        .iter()
        .zip(documents.iter())
        .map(|(vector, document)| (cosine_similarity(&query_vector, vector), document.as_str()))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    println!("\nranked by cosine similarity:");
    for (score, document) in scored {
        println!("  {score:.4}  {document}");
    }

    Ok(())
}

/// Returns 0.0 when either vector is empty or zero-length, so a degenerate
/// embedding shows up as a score of 0 rather than a NaN.
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
