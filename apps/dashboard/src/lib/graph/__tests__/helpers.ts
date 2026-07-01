/**
 * Test helpers: factories for creating test data.
 */
import type { GraphNode, GraphEdge, VestigeEvent, VestigeEventType } from '$types';

let nodeCounter = 0;

export function makeNode(overrides: Partial<GraphNode> = {}): GraphNode {
	nodeCounter++;
	return {
		id: `node-${nodeCounter}`,
		label: `Test Node ${nodeCounter}`,
		type: 'fact',
		retention: 0.8,
		tags: ['test'],
		createdAt: new Date().toISOString(),
		updatedAt: new Date().toISOString(),
		isCenter: false,
		...overrides,
	};
}

export function makeEdge(
	source: string,
	target: string,
	overrides: Partial<GraphEdge> = {}
): GraphEdge {
	return {
		source,
		target,
		weight: 0.5,
		type: 'semantic',
		...overrides,
	};
}

export function makeEvent(type: VestigeEventType, data: Record<string, unknown> = {}): VestigeEvent {
	return { type, data };
}

export function resetNodeCounter() {
	nodeCounter = 0;
}

/** Run simulation for N ticks */
export function tickN(sim: { tick: (edges: GraphEdge[]) => void }, edges: GraphEdge[], n: number) {
	for (let i = 0; i < n; i++) {
		sim.tick(edges);
	}
}
