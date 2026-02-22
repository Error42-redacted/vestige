//! Vestige Search Benchmarks
//!
//! Benchmarks for core search operations using Criterion.
//! Run with: cargo bench -p vestige-core

use criterion::{criterion_group, criterion_main, Criterion, black_box};
use vestige_core::search::hyde::{classify_intent, expand_query, centroid_embedding};
use vestige_core::search::{reciprocal_rank_fusion, linear_combination, sanitize_fts5_query};
use vestige_core::embeddings::cosine_similarity;

fn bench_classify_intent(c: &mut Criterion) {
    let queries = [
        "What is FSRS?",
        "how to configure embeddings",
        "why does retention decay",
        "fn main()",
        "vestige memory system",
    ];

    c.bench_function("classify_intent", |b| {
        b.iter(|| {
            for q in &queries {
                black_box(classify_intent(q));
            }
        })
    });
}

fn bench_expand_query(c: &mut Criterion) {
    c.bench_function("expand_query", |b| {
        b.iter(|| {
            black_box(expand_query("What is spaced repetition and how does FSRS work?"));
        })
    });
}

fn bench_centroid_embedding(c: &mut Criterion) {
    // Simulate 4 embeddings of 256 dimensions
    let embeddings: Vec<Vec<f32>> = (0..4)
        .map(|i| {
            (0..256)
                .map(|j| ((i * 256 + j) as f32).sin())
                .collect()
        })
        .collect();

    c.bench_function("centroid_256d_4vecs", |b| {
        b.iter(|| {
            black_box(centroid_embedding(&embeddings));
        })
    });
}

fn bench_rrf_fusion(c: &mut Criterion) {
    let keyword_results: Vec<(String, f32)> = (0..50)
        .map(|i| (format!("doc-{i}"), 1.0 - i as f32 / 50.0))
        .collect();
    let semantic_results: Vec<(String, f32)> = (0..50)
        .map(|i| (format!("doc-{}", 25 + i), 1.0 - i as f32 / 50.0))
        .collect();

    c.bench_function("rrf_50x50", |b| {
        b.iter(|| {
            black_box(reciprocal_rank_fusion(&keyword_results, &semantic_results, 60.0));
        })
    });
}

fn bench_linear_combination(c: &mut Criterion) {
    let keyword_results: Vec<(String, f32)> = (0..50)
        .map(|i| (format!("doc-{i}"), 1.0 - i as f32 / 50.0))
        .collect();
    let semantic_results: Vec<(String, f32)> = (0..50)
        .map(|i| (format!("doc-{}", 25 + i), 1.0 - i as f32 / 50.0))
        .collect();

    c.bench_function("linear_combo_50x50", |b| {
        b.iter(|| {
            black_box(linear_combination(&keyword_results, &semantic_results, 0.3, 0.7));
        })
    });
}

fn bench_sanitize_fts5(c: &mut Criterion) {
    c.bench_function("sanitize_fts5_query", |b| {
        b.iter(|| {
            black_box(sanitize_fts5_query("hello world \"exact phrase\" OR special-chars!@#"));
        })
    });
}

fn bench_cosine_similarity(c: &mut Criterion) {
    let a: Vec<f32> = (0..256).map(|i| (i as f32).sin()).collect();
    let b: Vec<f32> = (0..256).map(|i| (i as f32).cos()).collect();

    c.bench_function("cosine_similarity_256d", |b_bench| {
        b_bench.iter(|| {
            black_box(cosine_similarity(&a, &b));
        })
    });
}

criterion_group!(
    benches,
    bench_classify_intent,
    bench_expand_query,
    bench_centroid_embedding,
    bench_rrf_fusion,
    bench_linear_combination,
    bench_sanitize_fts5,
    bench_cosine_similarity,
);
criterion_main!(benches);
