import type {
	MemoryListResponse,
	Memory,
	SearchResult,
	SystemStats,
	HealthCheck,
	TimelineResponse,
	GraphResponse,
	DreamResult,
	ImportanceScore,
	RetentionDistribution,
	ConsolidationResult,
	IntentionItem
} from '$types';

const BASE = '/api';

async function fetcher<T>(path: string, options?: RequestInit): Promise<T> {
	const res = await fetch(`${BASE}${path}`, {
		headers: { 'Content-Type': 'application/json' },
		...options
	});
	if (!res.ok) throw new Error(`API ${res.status}: ${res.statusText}`);
	return res.json();
}

export const api = {
	// Memories
	memories: {
		list: (params?: Record<string, string>) => {
			const qs = params ? '?' + new URLSearchParams(params).toString() : '';
			return fetcher<MemoryListResponse>(`/memories${qs}`);
		},
		get: (id: string) => fetcher<Memory>(`/memories/${id}`),
		delete: (id: string) => fetcher<{ deleted: boolean }>(`/memories/${id}`, { method: 'DELETE' }),
		promote: (id: string) => fetcher<Memory>(`/memories/${id}/promote`, { method: 'POST' }),
		demote: (id: string) => fetcher<Memory>(`/memories/${id}/demote`, { method: 'POST' })
	},

	// Search
	search: (q: string, limit = 20) =>
		fetcher<SearchResult>(`/search?q=${encodeURIComponent(q)}&limit=${limit}`),

	// Stats & Health
	stats: () => fetcher<SystemStats>('/stats'),
	health: () => fetcher<HealthCheck>('/health'),

	// Timeline
	timeline: (days = 7, limit = 200) =>
		fetcher<TimelineResponse>(`/timeline?days=${days}&limit=${limit}`),

	// Graph
	graph: (params?: { query?: string; center_id?: string; depth?: number; max_nodes?: number }) => {
		const qs = params ? '?' + new URLSearchParams(
			Object.entries(params)
				.filter(([, v]) => v !== undefined)
				.map(([k, v]) => [k, String(v)])
		).toString() : '';
		return fetcher<GraphResponse>(`/graph${qs}`);
	},

	// Cognitive operations
	dream: () => fetcher<DreamResult>('/dream', { method: 'POST' }),

	explore: (fromId: string, action = 'associations', toId?: string, limit = 10) =>
		fetcher<Record<string, unknown>>('/explore', {
			method: 'POST',
			body: JSON.stringify({ from_id: fromId, action, to_id: toId, limit })
		}),

	predict: () => fetcher<Record<string, unknown>>('/predict', { method: 'POST' }),

	importance: (content: string) =>
		fetcher<ImportanceScore>('/importance', {
			method: 'POST',
			body: JSON.stringify({ content })
		}),

	consolidate: () => fetcher<ConsolidationResult>('/consolidate', { method: 'POST' }),

	retentionDistribution: () => fetcher<RetentionDistribution>('/retention-distribution'),

	// Intentions
	intentions: (status = 'active') =>
		fetcher<{ intentions: IntentionItem[]; total: number; filter: string }>(`/intentions?status=${status}`)
};
