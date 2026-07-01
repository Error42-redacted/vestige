//! Search Module
//!
//! Provides high-performance search capabilities:
//! - Vector search using HNSW (USearch)
//! - Keyword search using BM25/FTS5
//! - Hybrid search with RRF fusion
//! - Temporal-aware search
//! - Reranking for precision (GOD TIER 2026)

mod hybrid;
pub mod hyde;
mod keyword;
mod reranker;
mod temporal;
mod vector;

pub use vector::{
    DEFAULT_CONNECTIVITY, DEFAULT_DIMENSIONS, VectorIndex, VectorIndexConfig, VectorIndexStats,
    VectorSearchError,
};

pub use keyword::{KeywordSearcher, sanitize_fts5_query};

pub use hybrid::{HybridSearchConfig, HybridSearcher, linear_combination, reciprocal_rank_fusion};

pub use temporal::TemporalSearcher;

// GOD TIER 2026: Reranking for +15-20% precision
pub use reranker::{
    DEFAULT_RERANK_COUNT, DEFAULT_RETRIEVAL_COUNT, RerankedResult, Reranker, RerankerConfig,
    RerankerError,
};

// v2.0: HyDE-inspired query expansion for improved semantic search
pub use hyde::{QueryIntent, centroid_embedding, classify_intent, expand_query};
