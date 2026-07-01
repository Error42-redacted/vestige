//! Keyword Search (BM25/FTS5)
//!
//! Provides keyword-based search using SQLite FTS5.
//! Query sanitization lives in `crate::fts` (always available, even without vector-search).

// Re-export from the always-available fts module
pub use crate::fts::sanitize_fts5_query;

// ============================================================================
// KEYWORD SEARCHER
// ============================================================================

/// Keyword search configuration
#[derive(Debug, Clone)]
pub struct KeywordSearchConfig {
    /// Maximum query length
    pub max_query_length: usize,
    /// Enable stemming
    pub enable_stemming: bool,
    /// Boost factor for title matches
    pub title_boost: f32,
    /// Boost factor for tag matches
    pub tag_boost: f32,
}

impl Default for KeywordSearchConfig {
    fn default() -> Self {
        Self {
            max_query_length: 1000,
            enable_stemming: true,
            title_boost: 2.0,
            tag_boost: 1.5,
        }
    }
}

/// Keyword searcher for FTS5 queries
pub struct KeywordSearcher {
    #[allow(dead_code)] // Config will be used when FTS5 stemming/boosting is implemented
    config: KeywordSearchConfig,
}

impl Default for KeywordSearcher {
    fn default() -> Self {
        Self::new()
    }
}

impl KeywordSearcher {
    /// Create a new keyword searcher
    pub fn new() -> Self {
        Self {
            config: KeywordSearchConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: KeywordSearchConfig) -> Self {
        Self { config }
    }

    /// Prepare a query for FTS5
    pub fn prepare_query(&self, query: &str) -> String {
        sanitize_fts5_query(query)
    }

    /// Tokenize a query into terms
    pub fn tokenize(&self, query: &str) -> Vec<String> {
        query
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .filter(|s| s.len() >= 2) // Skip very short terms
            .collect()
    }

    /// Build a proximity query (terms must appear near each other)
    pub fn proximity_query(&self, terms: &[&str], distance: usize) -> String {
        let cleaned: Vec<String> = terms
            .iter()
            .map(|t| t.replace(|c: char| !c.is_alphanumeric(), ""))
            .filter(|t| !t.is_empty())
            .collect();

        if cleaned.is_empty() {
            return "\"\"".to_string();
        }

        if cleaned.len() == 1 {
            return format!("\"{}\"", cleaned[0]);
        }

        // FTS5 NEAR query: NEAR(term1 term2, distance)
        format!("NEAR({}, {})", cleaned.join(" "), distance)
    }

    /// Build a prefix query (for autocomplete)
    pub fn prefix_query(&self, prefix: &str) -> String {
        let cleaned = prefix.replace(|c: char| !c.is_alphanumeric(), "");
        if cleaned.is_empty() {
            return "\"\"".to_string();
        }
        format!("\"{}\"*", cleaned)
    }

    /// Highlight matched terms in text
    pub fn highlight(&self, text: &str, terms: &[String]) -> String {
        let mut result = text.to_string();

        for term in terms {
            // Case-insensitive replacement with highlighting
            let lower_text = result.to_lowercase();
            let lower_term = term.to_lowercase();

            if let Some(byte_pos) = lower_text.find(&lower_term) {
                // Convert byte position to char position for safe slicing
                let char_pos = lower_text[..byte_pos].chars().count();
                let term_char_len = lower_term.chars().count();

                // Extract matched portion using char indices
                let prefix: String = result.chars().take(char_pos).collect();
                let matched: String = result.chars().skip(char_pos).take(term_char_len).collect();
                let suffix: String = result.chars().skip(char_pos + term_char_len).collect();

                let highlighted = format!("**{}**", matched);
                result = format!("{}{}{}", prefix, highlighted, suffix);
            }
        }

        result
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // FTS5 sanitization tests are in crate::fts::tests

    #[test]
    fn test_tokenize() {
        let searcher = KeywordSearcher::new();
        let terms = searcher.tokenize("Hello World Test");

        assert_eq!(terms, vec!["hello", "world", "test"]);
    }

    #[test]
    fn test_tokenize_filters_short() {
        let searcher = KeywordSearcher::new();
        let terms = searcher.tokenize("a is the test");

        assert_eq!(terms, vec!["is", "the", "test"]);
    }

    #[test]
    fn test_prefix_query() {
        let searcher = KeywordSearcher::new();

        assert_eq!(searcher.prefix_query("hel"), "\"hel\"*");
        assert_eq!(searcher.prefix_query(""), "\"\"");
    }

    #[test]
    fn test_highlight() {
        let searcher = KeywordSearcher::new();
        let terms = vec!["hello".to_string()];

        let highlighted = searcher.highlight("Hello world", &terms);
        assert!(highlighted.contains("**Hello**"));
    }
}
