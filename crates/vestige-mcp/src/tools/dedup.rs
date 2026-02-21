//! Find Duplicates Tool
//!
//! Detects duplicate and near-duplicate memory clusters using
//! cosine similarity on stored embeddings. Uses union-find for
//! efficient clustering.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;


use vestige_core::Storage;
#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use vestige_core::cosine_similarity;

/// Input schema for find_duplicates tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "similarity_threshold": {
                "type": "number",
                "description": "Minimum cosine similarity to consider as duplicate (0.0-1.0, default: 0.80)",
                "default": 0.80,
                "minimum": 0.5,
                "maximum": 1.0
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of duplicate clusters to return (default: 20)",
                "default": 20,
                "minimum": 1,
                "maximum": 100
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional: only check memories with these tags (ANY match)"
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DedupArgs {
    similarity_threshold: Option<f64>,
    limit: Option<usize>,
    tags: Option<Vec<String>>,
}

/// Simple union-find for clustering
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        if self.rank[rx] < self.rank[ry] {
            self.parent[rx] = ry;
        } else if self.rank[rx] > self.rank[ry] {
            self.parent[ry] = rx;
        } else {
            self.parent[ry] = rx;
            self.rank[rx] += 1;
        }
    }
}

pub async fn execute(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: DedupArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => DedupArgs {
            similarity_threshold: None,
            limit: None,
            tags: None,
        },
    };

    let threshold = args.similarity_threshold.unwrap_or(0.80) as f32;
    let limit = args.limit.unwrap_or(20);
    let tag_filter = args.tags.unwrap_or_default();

    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {

        // Load all embeddings
        let all_embeddings = storage
            .get_all_embeddings()
            .map_err(|e| format!("Failed to load embeddings: {}", e))?;

        if all_embeddings.is_empty() {
            return Ok(serde_json::json!({
                "clusters": [],
                "totalMemories": 0,
                "totalWithEmbeddings": 0,
                "message": "No embeddings found. Run consolidation first."
            }));
        }

        // Load nodes for metadata (content preview, retention, tags)
        let mut all_nodes = Vec::new();
        let mut offset = 0;
        loop {
            let batch = storage
                .get_all_nodes(500, offset)
                .map_err(|e| format!("Failed to load nodes: {}", e))?;
            let batch_len = batch.len();
            all_nodes.extend(batch);
            if batch_len < 500 {
                break;
            }
            offset += 500;
        }

        // Build node lookup
        let node_map: HashMap<String, &vestige_core::KnowledgeNode> =
            all_nodes.iter().map(|n| (n.id.clone(), n)).collect();

        // Filter by tags if specified
        let filtered_embeddings: Vec<(usize, &String, &Vec<f32>)> = all_embeddings
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| {
                if tag_filter.is_empty() {
                    return true;
                }
                if let Some(node) = node_map.get(id) {
                    tag_filter.iter().any(|t| node.tags.contains(t))
                } else {
                    false
                }
            })
            .map(|(i, (id, vec))| (i, id, vec))
            .collect();

        let n = filtered_embeddings.len();

        if n > 2000 {
            return Ok(serde_json::json!({
                "warning": format!("Too many memories to scan ({} with embeddings). Filter by tags to reduce scope.", n),
                "totalMemories": all_nodes.len(),
                "totalWithEmbeddings": n
            }));
        }

        // O(n^2) pairwise similarity + union-find clustering
        let mut uf = UnionFind::new(n);
        let mut similarities: Vec<(usize, usize, f32)> = Vec::new();

        for i in 0..n {
            for j in (i + 1)..n {
                let sim = cosine_similarity(filtered_embeddings[i].2, filtered_embeddings[j].2);
                if sim >= threshold {
                    uf.union(i, j);
                    similarities.push((i, j, sim));
                }
            }
        }

        // Group into clusters
        let mut cluster_map: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..n {
            let root = uf.find(i);
            cluster_map.entry(root).or_default().push(i);
        }

        // Only keep clusters with >1 member, sorted by size descending
        let mut clusters: Vec<Vec<usize>> = cluster_map
            .into_values()
            .filter(|c| c.len() > 1)
            .collect();
        clusters.sort_by_key(|b| std::cmp::Reverse(b.len()));
        clusters.truncate(limit);

        // Build similarity lookup for formatting
        let mut sim_lookup: HashMap<(usize, usize), f32> = HashMap::new();
        for &(i, j, sim) in &similarities {
            sim_lookup.insert((i, j), sim);
            sim_lookup.insert((j, i), sim);
        }

        // Format output
        let cluster_results: Vec<Value> = clusters
            .iter()
            .enumerate()
            .map(|(ci, members)| {
                let anchor = members[0];
                let member_results: Vec<Value> = members
                    .iter()
                    .map(|&idx| {
                        let id = &filtered_embeddings[idx].1;
                        let node = node_map.get(id.as_str());
                        let content_preview = node
                            .map(|n| {
                                let c = n.content.replace('\n', " ");
                                if c.len() > 120 {
                                    format!("{}...", &c[..120])
                                } else {
                                    c
                                }
                            })
                            .unwrap_or_default();

                        let sim_to_anchor = if idx == anchor {
                            1.0
                        } else {
                            sim_lookup
                                .get(&(anchor, idx))
                                .copied()
                                .unwrap_or(0.0)
                        };

                        serde_json::json!({
                            "id": id,
                            "contentPreview": content_preview,
                            "retention": node.map(|n| n.retention_strength).unwrap_or(0.0),
                            "createdAt": node.map(|n| n.created_at.to_rfc3339()).unwrap_or_default(),
                            "tags": node.map(|n| &n.tags).unwrap_or(&vec![]),
                            "similarityToAnchor": format!("{:.3}", sim_to_anchor)
                        })
                    })
                    .collect();

                serde_json::json!({
                    "clusterId": ci,
                    "size": members.len(),
                    "members": member_results,
                    "suggestedAction": if members.len() > 3 { "review" } else { "merge" }
                })
            })
            .collect();

        Ok(serde_json::json!({
            "clusters": cluster_results,
            "totalClusters": cluster_results.len(),
            "totalMemories": all_nodes.len(),
            "totalWithEmbeddings": n,
            "threshold": threshold,
            "pairsChecked": n * (n - 1) / 2
        }))
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        Ok(serde_json::json!({
            "error": "Embeddings feature not enabled. Cannot compute similarities.",
            "clusters": []
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema() {
        let schema = schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["similarity_threshold"].is_object());
    }

    #[test]
    fn test_union_find() {
        let mut uf = UnionFind::new(5);
        uf.union(0, 1);
        uf.union(2, 3);
        uf.union(1, 3);
        assert_eq!(uf.find(0), uf.find(3));
        assert_ne!(uf.find(0), uf.find(4));
    }

    #[tokio::test]
    async fn test_empty_storage() {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        let storage = Arc::new(storage);
        let result = execute(&storage, None).await;
        assert!(result.is_ok());
    }
}
