// Vestige API Types — auto-matched to Rust backend

export interface Memory {
	id: string;
	content: string;
	nodeType: string;
	tags: string[];
	retentionStrength: number;
	storageStrength: number;
	retrievalStrength: number;
	createdAt: string;
	updatedAt: string;
	source?: string;
	reviewCount?: number;
	combinedScore?: number;
	sentimentScore?: number;
	sentimentMagnitude?: number;
	lastAccessedAt?: string;
	nextReviewAt?: string;
	validFrom?: string;
	validUntil?: string;
}

export interface SearchResult {
	query: string;
	total: number;
	durationMs: number;
	results: Memory[];
}

export interface MemoryListResponse {
	total: number;
	memories: Memory[];
}

export interface SystemStats {
	totalMemories: number;
	dueForReview: number;
	averageRetention: number;
	averageStorageStrength: number;
	averageRetrievalStrength: number;
	withEmbeddings: number;
	embeddingCoverage: number;
	embeddingModel: string;
	oldestMemory?: string;
	newestMemory?: string;
}

export interface HealthCheck {
	status: 'healthy' | 'degraded' | 'critical' | 'empty';
	totalMemories: number;
	averageRetention: number;
	version: string;
}

export interface TimelineDay {
	date: string;
	count: number;
	memories: Memory[];
}

export interface TimelineResponse {
	days: number;
	totalMemories: number;
	timeline: TimelineDay[];
}

export interface GraphNode {
	id: string;
	label: string;
	type: string;
	retention: number;
	tags: string[];
	createdAt: string;
	updatedAt: string;
	isCenter: boolean;
}

export interface GraphEdge {
	source: string;
	target: string;
	weight: number;
	type: string;
}

export interface GraphResponse {
	nodes: GraphNode[];
	edges: GraphEdge[];
	center_id: string;
	depth: number;
	nodeCount: number;
	edgeCount: number;
}

export interface DreamResult {
	status: string;
	memoriesReplayed: number;
	connectionsPersisted: number;
	insights: DreamInsight[];
	stats: {
		newConnectionsFound: number;
		connectionsPersisted: number;
		memoriesStrengthened: number;
		memoriesCompressed: number;
		insightsGenerated: number;
		durationMs: number;
	};
}

export interface DreamInsight {
	type: string;
	insight: string;
	sourceMemories: string[];
	confidence: number;
	noveltyScore: number;
}

export interface ImportanceScore {
	composite: number;
	channels: {
		novelty: number;
		arousal: number;
		reward: number;
		attention: number;
	};
	recommendation: 'save' | 'skip';
}

export interface RetentionDistribution {
	distribution: { range: string; count: number }[];
	byType: Record<string, number>;
	endangered: Memory[];
	total: number;
}

export interface ConsolidationResult {
	nodesProcessed: number;
	decayApplied: number;
	embeddingsGenerated: number;
	duplicatesMerged: number;
	activationsComputed: number;
	durationMs: number;
}

// WebSocket event types
export type VestigeEventType =
	| 'Connected'
	| 'MemoryCreated'
	| 'MemoryUpdated'
	| 'MemoryDeleted'
	| 'MemoryPromoted'
	| 'MemoryDemoted'
	| 'SearchPerformed'
	| 'DreamStarted'
	| 'DreamProgress'
	| 'DreamCompleted'
	| 'ConsolidationStarted'
	| 'ConsolidationCompleted'
	| 'RetentionDecayed'
	| 'ConnectionDiscovered'
	| 'ActivationSpread'
	| 'ImportanceScored'
	| 'Heartbeat';

export interface VestigeEvent {
	type: VestigeEventType;
	data: Record<string, unknown>;
}

// Intentions (prospective memory)
export interface IntentionItem {
	id: string;
	content: string;
	trigger_type: string;
	trigger_value: string;
	status: string;
	priority: string;
	created_at: string;
	deadline?: string;
	snoozed_until?: string;
}

// Node type colors for visualization
export const NODE_TYPE_COLORS: Record<string, string> = {
	fact: '#3b82f6',      // blue
	concept: '#8b5cf6',   // purple
	event: '#f59e0b',     // amber
	person: '#10b981',    // emerald
	place: '#06b6d4',     // cyan
	note: '#6b7280',      // gray
	pattern: '#ec4899',   // pink
	decision: '#ef4444',  // red
};

export const EVENT_TYPE_COLORS: Record<string, string> = {
	MemoryCreated: '#10b981',
	MemoryUpdated: '#3b82f6',
	MemoryDeleted: '#ef4444',
	MemoryPromoted: '#22c55e',
	MemoryDemoted: '#f97316',
	SearchPerformed: '#6366f1',
	DreamStarted: '#8b5cf6',
	DreamProgress: '#7c3aed',
	DreamCompleted: '#a855f7',
	ConsolidationStarted: '#f59e0b',
	ConsolidationCompleted: '#f97316',
	RetentionDecayed: '#ef4444',
	ConnectionDiscovered: '#06b6d4',
	ActivationSpread: '#14b8a6',
	ImportanceScored: '#ec4899',
	Heartbeat: '#6b7280',
};
