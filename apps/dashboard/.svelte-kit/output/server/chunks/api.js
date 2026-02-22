const BASE = "/api";
async function fetcher(path, options) {
  const res = await fetch(`${BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options
  });
  if (!res.ok) throw new Error(`API ${res.status}: ${res.statusText}`);
  return res.json();
}
const api = {
  // Memories
  memories: {
    list: (params) => {
      const qs = params ? "?" + new URLSearchParams(params).toString() : "";
      return fetcher(`/memories${qs}`);
    },
    get: (id) => fetcher(`/memories/${id}`),
    delete: (id) => fetcher(`/memories/${id}`, { method: "DELETE" }),
    promote: (id) => fetcher(`/memories/${id}/promote`, { method: "POST" }),
    demote: (id) => fetcher(`/memories/${id}/demote`, { method: "POST" })
  },
  // Search
  search: (q, limit = 20) => fetcher(`/search?q=${encodeURIComponent(q)}&limit=${limit}`),
  // Stats & Health
  stats: () => fetcher("/stats"),
  health: () => fetcher("/health"),
  // Timeline
  timeline: (days = 7, limit = 200) => fetcher(`/timeline?days=${days}&limit=${limit}`),
  // Graph
  graph: (params) => {
    const qs = params ? "?" + new URLSearchParams(
      Object.entries(params).filter(([, v]) => v !== void 0).map(([k, v]) => [k, String(v)])
    ).toString() : "";
    return fetcher(`/graph${qs}`);
  },
  // Cognitive operations
  dream: () => fetcher("/dream", { method: "POST" }),
  explore: (fromId, action = "associations", toId, limit = 10) => fetcher("/explore", {
    method: "POST",
    body: JSON.stringify({ from_id: fromId, action, to_id: toId, limit })
  }),
  predict: () => fetcher("/predict", { method: "POST" }),
  importance: (content) => fetcher("/importance", {
    method: "POST",
    body: JSON.stringify({ content })
  }),
  consolidate: () => fetcher("/consolidate", { method: "POST" }),
  retentionDistribution: () => fetcher("/retention-distribution"),
  // Intentions
  intentions: (status = "active") => fetcher(`/intentions?status=${status}`)
};
export {
  api as a
};
