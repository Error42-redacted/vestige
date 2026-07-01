import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('three', async () => {
	const mock = await import('./three-mock');
	return { ...mock };
});

import { EdgeManager } from '../edges';
import { Vector3 } from './three-mock';
import { makeEdge, resetNodeCounter } from './helpers';

describe('EdgeManager', () => {
	let manager: EdgeManager;
	let positions: Map<string, InstanceType<typeof Vector3>>;

	beforeEach(() => {
		resetNodeCounter();
		manager = new EdgeManager();
		positions = new Map([
			['a', new Vector3(0, 0, 0)],
			['b', new Vector3(10, 0, 0)],
			['c', new Vector3(0, 10, 0)],
			['d', new Vector3(10, 10, 0)],
		]);
	});

	describe('createEdges', () => {
		it('creates line objects for all edges', () => {
			const edges = [makeEdge('a', 'b'), makeEdge('b', 'c')];
			manager.createEdges(edges, positions);

			expect(manager.group.children.length).toBe(2);
		});

		it('skips edges with missing node positions', () => {
			const edges = [makeEdge('a', 'missing')];
			manager.createEdges(edges, positions);

			expect(manager.group.children.length).toBe(0);
		});

		it('stores source/target in userData', () => {
			const edges = [makeEdge('a', 'b')];
			manager.createEdges(edges, positions);

			const line = manager.group.children[0];
			expect(line.userData.source).toBe('a');
			expect(line.userData.target).toBe('b');
		});

		it('caps opacity at 0.8 (raised from 0.6 in v2.0.6 issue #31 fix)', () => {
			const edges = [makeEdge('a', 'b', { weight: 10.0 })];
			manager.createEdges(edges, positions);

			const line = manager.group.children[0] as any;
			expect(line.material.opacity).toBeLessThanOrEqual(0.8);
			// Baseline floor too — with weight 10 we should be at the cap, not below old 0.6
			expect(line.material.opacity).toBeGreaterThanOrEqual(0.6);
		});
	});

	describe('addEdge — growth animation', () => {
		it('adds a new line to the group', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			expect(manager.group.children.length).toBe(1);
		});

		it('starts with zero-length line at source position', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			const line = manager.group.children[0] as any;
			const attrs = line.geometry.attributes.position;

			// Both endpoints should be at source (a) position
			expect(attrs.getX(0)).toBe(0);
			expect(attrs.getX(1)).toBe(0); // not yet at target
		});

		it('starts with zero opacity', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			const line = manager.group.children[0] as any;
			expect(line.material.opacity).toBe(0);
		});

		it('grows to full length over 45 frames', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			// Animate through growth
			for (let f = 0; f < 45; f++) {
				manager.animateEdges(positions);
			}

			const line = manager.group.children[0] as any;
			const attrs = line.geometry.attributes.position;

			// End point should be at target position
			expect(attrs.getX(1)).toBeCloseTo(10, 0);
		});

		it('opacity increases during growth', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			for (let f = 0; f < 25; f++) {
				manager.animateEdges(positions);
			}

			const line = manager.group.children[0] as any;
			expect(line.material.opacity).toBeGreaterThan(0);
		});

		it('reaches final opacity after growth completes', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			for (let f = 0; f < 46; f++) {
				manager.animateEdges(positions);
			}

			const line = manager.group.children[0] as any;
			// v2.0.6 issue #31 fix raised final edge opacity 0.5 → 0.65
			expect(line.material.opacity).toBe(0.65);
		});

		it('uses easeOutCubic for smooth deceleration', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			// Record end point X at each frame
			const xValues: number[] = [];
			for (let f = 0; f < 45; f++) {
				manager.animateEdges(positions);
				const line = manager.group.children[0] as any;
				const attrs = line.geometry.attributes.position;
				xValues.push(attrs.getX(1));
			}

			// easeOutCubic: fast start, slow end
			// First half should cover more than 50% of distance
			const midIdx = Math.floor(xValues.length / 2);
			const midProgress = xValues[midIdx] / 10;
			expect(midProgress).toBeGreaterThan(0.5);
		});

		it('skips edges with missing positions', () => {
			const edge = makeEdge('a', 'missing');
			manager.addEdge(edge, positions);

			// Line should be created but with no geometry update
			expect(manager.group.children.length).toBe(0);
		});
	});

	describe('removeEdgesForNode', () => {
		it('marks connected edges for dissolution', () => {
			const edges = [makeEdge('a', 'b'), makeEdge('b', 'c'), makeEdge('c', 'd')];
			manager.createEdges(edges, positions);
			expect(manager.group.children.length).toBe(3);

			manager.removeEdgesForNode('b');

			// After dissolution animation completes
			for (let f = 0; f < 45; f++) {
				manager.animateEdges(positions);
			}

			// Only edge c->d should remain
			expect(manager.group.children.length).toBe(1);
			expect(manager.group.children[0].userData.source).toBe('c');
		});

		it('dissolving edges fade out over 40 frames', () => {
			const edges = [makeEdge('a', 'b')];
			manager.createEdges(edges, positions);

			const line = manager.group.children[0] as any;
			const initialOpacity = line.material.opacity;

			manager.removeEdgesForNode('a');

			// Midway through dissolution
			for (let f = 0; f < 20; f++) {
				manager.animateEdges(positions);
			}

			expect(line.material.opacity).toBeLessThan(initialOpacity);
			expect(line.material.opacity).toBeGreaterThan(0);
		});

		it('fully removes edge after dissolution completes', () => {
			const edges = [makeEdge('a', 'b')];
			manager.createEdges(edges, positions);

			manager.removeEdgesForNode('a');

			for (let f = 0; f < 45; f++) {
				manager.animateEdges(positions);
			}

			expect(manager.group.children.length).toBe(0);
		});

		it('cancels active growth animation if edge is dissolving', () => {
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			// Partially grow
			for (let f = 0; f < 10; f++) {
				manager.animateEdges(positions);
			}

			// Then dissolve
			manager.removeEdgesForNode('a');

			for (let f = 0; f < 45; f++) {
				manager.animateEdges(positions);
			}

			expect(manager.group.children.length).toBe(0);
		});
	});

	describe('updatePositions', () => {
		it('updates static edge endpoints', () => {
			const edges = [makeEdge('a', 'b')];
			manager.createEdges(edges, positions);

			// Move node a
			positions.set('a', new Vector3(5, 5, 5));
			manager.updatePositions(positions);

			const line = manager.group.children[0] as any;
			const attrs = line.geometry.attributes.position;
			expect(attrs.getX(0)).toBe(5);
			expect(attrs.getY(0)).toBe(5);
		});

		it('skips edges currently being animated', () => {
			// Add a growing edge
			const edge = makeEdge('a', 'b');
			manager.addEdge(edge, positions);

			// updatePositions should not override the animation
			manager.updatePositions(positions);

			// Growing edge should still be at its animated state
			const line = manager.group.children[0] as any;
			const attrs = line.geometry.attributes.position;
			// Point 1 should still be at source (zero-length start), not target
			expect(attrs.getX(1)).toBe(0);
		});
	});

	describe('multiple simultaneous edge animations', () => {
		it('handles multiple edges growing at once', () => {
			manager.addEdge(makeEdge('a', 'b'), positions);
			manager.addEdge(makeEdge('c', 'd'), positions);

			for (let f = 0; f < 50; f++) {
				manager.animateEdges(positions);
			}

			// Both should be fully grown
			expect(manager.group.children.length).toBe(2);
			manager.group.children.forEach((child) => {
				// v2.0.6 issue #31 fix raised final edge opacity 0.5 → 0.65
				expect((child as any).material.opacity).toBe(0.65);
			});
		});

		it('handles mixed growing and dissolving edges', () => {
			// Create a static edge
			manager.createEdges([makeEdge('a', 'b')], positions);

			// Add a growing edge
			manager.addEdge(makeEdge('c', 'd'), positions);

			// Dissolve the static edge
			manager.removeEdgesForNode('a');

			for (let f = 0; f < 50; f++) {
				manager.animateEdges(positions);
			}

			// Only the new edge should remain
			expect(manager.group.children.length).toBe(1);
			expect(manager.group.children[0].userData.source).toBe('c');
		});
	});

	describe('dispose', () => {
		it('clears animation queues and disposes materials without error', () => {
			manager.createEdges([makeEdge('a', 'b')], positions);
			manager.addEdge(makeEdge('c', 'd'), positions);

			// Dispose should not throw and should clean up materials
			expect(() => manager.dispose()).not.toThrow();

			// After dispose, adding new animations should not interact with old state
			manager.addEdge(makeEdge('a', 'c'), positions);
			expect(() => {
				for (let f = 0; f < 50; f++) {
					manager.animateEdges(positions);
				}
			}).not.toThrow();
		});
	});
});
