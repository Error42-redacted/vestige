import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock three.js before any imports that use it
vi.mock('three', async () => {
	const mock = await import('./three-mock');
	return { ...mock };
});

import { ForceSimulation } from '../force-sim';
import { Vector3 } from './three-mock';
import { makeNode, makeEdge, resetNodeCounter, tickN } from './helpers';

describe('ForceSimulation', () => {
	beforeEach(() => resetNodeCounter());

	function createSim(nodeCount: number) {
		const positions = new Map<string, InstanceType<typeof Vector3>>();
		for (let i = 0; i < nodeCount; i++) {
			positions.set(`n${i}`, new Vector3(i * 10, 0, 0));
		}
		return new ForceSimulation(positions);
	}

	describe('initialization', () => {
		it('creates velocities for all positions', () => {
			const sim = createSim(5);
			expect(sim.velocities.size).toBe(5);
			for (const vel of sim.velocities.values()) {
				expect(vel.x).toBe(0);
				expect(vel.y).toBe(0);
				expect(vel.z).toBe(0);
			}
		});

		it('starts running at step 0', () => {
			const sim = createSim(3);
			expect(sim.running).toBe(true);
			expect(sim.step).toBe(0);
		});
	});

	describe('tick', () => {
		it('increments step count each tick', () => {
			const sim = createSim(3);
			sim.tick([]);
			expect(sim.step).toBe(1);
			sim.tick([]);
			expect(sim.step).toBe(2);
		});

		it('stops ticking after maxSteps', () => {
			const sim = createSim(2);
			tickN(sim, [], 301);
			const posAfter300 = sim.positions.get('n0')!.clone();
			tickN(sim, [], 10);
			expect(sim.positions.get('n0')!.x).toBe(posAfter300.x);
		});

		it('does not tick when not running', () => {
			const sim = createSim(2);
			sim.running = false;
			const posBefore = sim.positions.get('n0')!.clone();
			sim.tick([]);
			expect(sim.step).toBe(0);
			expect(sim.positions.get('n0')!.x).toBe(posBefore.x);
		});

		it('applies repulsion between nodes', () => {
			const positions = new Map<string, InstanceType<typeof Vector3>>();
			positions.set('a', new Vector3(0, 0, 0));
			positions.set('b', new Vector3(1, 0, 0));
			const sim = new ForceSimulation(positions);

			sim.tick([]);

			// After repulsion, nodes should have moved apart
			const a = sim.positions.get('a')!;
			const b = sim.positions.get('b')!;
			expect(b.x - a.x).toBeGreaterThan(1); // farther apart
		});

		it('applies attraction along edges', () => {
			const positions = new Map<string, InstanceType<typeof Vector3>>();
			positions.set('a', new Vector3(0, 0, 0));
			positions.set('b', new Vector3(100, 0, 0));
			const sim = new ForceSimulation(positions);

			const edges = [makeEdge('a', 'b', { weight: 1.0 })];
			tickN(sim, edges, 50);

			// After many ticks with attraction, nodes should be closer
			const dist = sim.positions.get('a')!.distanceTo(sim.positions.get('b')!);
			expect(dist).toBeLessThan(100);
		});

		it('applies centering force toward origin', () => {
			const positions = new Map<string, InstanceType<typeof Vector3>>();
			positions.set('far', new Vector3(1000, 1000, 1000));
			const sim = new ForceSimulation(positions);

			tickN(sim, [], 100);

			const pos = sim.positions.get('far')!;
			// Should have moved closer to origin
			expect(pos.length()).toBeLessThan(1000 * Math.sqrt(3));
		});
	});

	describe('addNode', () => {
		it('adds position and velocity entries', () => {
			const sim = createSim(2);
			const newPos = new Vector3(5, 5, 5);
			sim.addNode('new', newPos);

			expect(sim.positions.has('new')).toBe(true);
			expect(sim.velocities.has('new')).toBe(true);
			expect(sim.positions.get('new')!.x).toBe(5);
		});

		it('clones the input position', () => {
			const sim = createSim(1);
			const input = new Vector3(10, 10, 10);
			sim.addNode('new', input);

			input.x = 999;
			expect(sim.positions.get('new')!.x).toBe(10);
		});

		it('re-energizes physics so simulation stays alive', () => {
			const sim = createSim(2);
			// Exhaust the simulation
			tickN(sim, [], 305);
			expect(sim.step).toBe(301); // stopped at maxSteps+1

			// Add a node
			sim.addNode('live', new Vector3(0, 0, 0));
			expect(sim.running).toBe(true);

			// Should be able to tick again
			const stepBefore = sim.step;
			sim.tick([]);
			expect(sim.step).toBe(stepBefore + 1);
		});

		it('extends maxSteps by cooldown amount', () => {
			const sim = createSim(2);
			tickN(sim, [], 250);
			const stepAtAdd = sim.step;

			sim.addNode('live', new Vector3(0, 0, 0));

			// Should be able to tick at least 100 more times
			let ticked = 0;
			for (let i = 0; i < 100; i++) {
				const stepBefore = sim.step;
				sim.tick([]);
				if (sim.step > stepBefore) ticked++;
			}
			expect(ticked).toBe(100);
		});
	});

	describe('removeNode', () => {
		it('removes position and velocity entries', () => {
			const sim = createSim(3);
			sim.removeNode('n1');

			expect(sim.positions.has('n1')).toBe(false);
			expect(sim.velocities.has('n1')).toBe(false);
			expect(sim.positions.size).toBe(2);
		});

		it('simulation continues without removed node', () => {
			const sim = createSim(3);
			sim.removeNode('n1');

			// Should not throw
			tickN(sim, [], 10);
			expect(sim.positions.has('n0')).toBe(true);
			expect(sim.positions.has('n2')).toBe(true);
		});
	});

	describe('reset', () => {
		it('resets step count and running state', () => {
			const sim = createSim(3);
			tickN(sim, [], 100);
			sim.running = false;
			sim.reset();

			expect(sim.step).toBe(0);
			expect(sim.running).toBe(true);
		});

		it('zeroes all velocities', () => {
			const sim = createSim(3);
			tickN(sim, [], 10);
			sim.reset();

			for (const vel of sim.velocities.values()) {
				expect(vel.x).toBe(0);
				expect(vel.y).toBe(0);
				expect(vel.z).toBe(0);
			}
		});
	});

	describe('physics convergence', () => {
		it('two connected nodes reach equilibrium', () => {
			const positions = new Map<string, InstanceType<typeof Vector3>>();
			positions.set('a', new Vector3(-50, 0, 0));
			positions.set('b', new Vector3(50, 0, 0));
			const sim = new ForceSimulation(positions);

			const edges = [makeEdge('a', 'b', { weight: 0.5 })];
			tickN(sim, edges, 300);

			// Should reach equilibrium — velocities near zero
			const velA = sim.velocities.get('a')!;
			const velB = sim.velocities.get('b')!;
			expect(Math.abs(velA.x)).toBeLessThan(0.01);
			expect(Math.abs(velB.x)).toBeLessThan(0.01);
		});

		it('unconnected nodes repel to stable separation', () => {
			const positions = new Map<string, InstanceType<typeof Vector3>>();
			positions.set('a', new Vector3(0, 0, 0));
			positions.set('b', new Vector3(5, 0, 0));
			const sim = new ForceSimulation(positions);

			tickN(sim, [], 300);

			const dist = sim.positions.get('a')!.distanceTo(sim.positions.get('b')!);
			expect(dist).toBeGreaterThan(5); // repelled farther apart
		});

		it('multiple nodes form a spread-out cluster', () => {
			const positions = new Map<string, InstanceType<typeof Vector3>>();
			for (let i = 0; i < 10; i++) {
				positions.set(`n${i}`, new Vector3(Math.random() * 2, Math.random() * 2, Math.random() * 2));
			}
			const sim = new ForceSimulation(positions);

			const edges = [makeEdge('n0', 'n1'), makeEdge('n2', 'n3'), makeEdge('n4', 'n5')];
			tickN(sim, edges, 300);

			// All nodes should be spread out, no two should overlap
			const ids = Array.from(positions.keys());
			for (let i = 0; i < ids.length; i++) {
				for (let j = i + 1; j < ids.length; j++) {
					const dist = sim.positions.get(ids[i])!.distanceTo(sim.positions.get(ids[j])!);
					expect(dist).toBeGreaterThan(0.1);
				}
			}
		});
	});
});
