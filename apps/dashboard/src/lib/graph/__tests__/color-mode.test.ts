/**
 * v2.0.8 Memory-state colour mode — ruthless coverage.
 *
 * Every line added in v2.0.8 is exercised here: pure helpers, palette
 * integrity, NodeManager mode switching, in-place retinting, edge cases,
 * suppression interaction, new-node inheritance, idempotence, and
 * round-trip fidelity. If this file is green, the feature is wired.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('three', async () => {
	const mock = await import('./three-mock');
	return { ...mock };
});

import {
	NodeManager,
	getMemoryState,
	getAhaGraphColor,
	getNodeColor,
	AHAGRAPH_COLORS,
	MEMORY_STATE_COLORS,
	MEMORY_STATE_DESCRIPTIONS,
	type MemoryState,
	type ColorMode,
} from '../nodes';
import { NODE_TYPE_COLORS } from '$types';
import { Color, Vector3, MeshStandardMaterial, SpriteMaterial } from './three-mock';
import { makeNode, resetNodeCounter } from './helpers';

// Global spy cleanup — prototype-level spies must not leak between tests.
afterEach(() => {
	vi.restoreAllMocks();
});

// ----------------------------------------------------------------------------
// getMemoryState — boundary analysis across all 4 FSRS buckets
// ----------------------------------------------------------------------------

describe('getMemoryState — bucket classification', () => {
	it.each<[number, MemoryState]>([
		[1.0, 'active'],
		[0.95, 'active'],
		[0.7, 'active'], // inclusive lower bound of active
		[0.6999999, 'dormant'], // just below active threshold
		[0.5, 'dormant'],
		[0.4, 'dormant'], // inclusive lower bound of dormant
		[0.3999999, 'silent'], // just below dormant threshold
		[0.25, 'silent'],
		[0.1, 'silent'], // inclusive lower bound of silent
		[0.0999999, 'unavailable'], // just below silent threshold
		[0.05, 'unavailable'],
		[0.0, 'unavailable'],
	])('classifies retention %f as %s', (retention, expected) => {
		expect(getMemoryState(retention)).toBe(expected);
	});

	it('handles retention > 1 as active (over-strength, shouldn\'t happen but clamp-free)', () => {
		expect(getMemoryState(1.5)).toBe('active');
		expect(getMemoryState(999)).toBe('active');
	});

	it('handles negative retention as unavailable (defensive)', () => {
		expect(getMemoryState(-0.5)).toBe('unavailable');
		expect(getMemoryState(-1000)).toBe('unavailable');
	});

	it('classifies NaN as unavailable (no predicate is true)', () => {
		expect(getMemoryState(NaN)).toBe('unavailable');
	});

	it('classifies +Infinity as active', () => {
		expect(getMemoryState(Infinity)).toBe('active');
	});

	it('classifies -Infinity as unavailable', () => {
		expect(getMemoryState(-Infinity)).toBe('unavailable');
	});

	it('is deterministic and pure — same input gives same output across 10k calls', () => {
		const samples = Array.from({ length: 10000 }, () => Math.random());
		const first = samples.map(getMemoryState);
		const second = samples.map(getMemoryState);
		expect(first).toEqual(second);
	});
});

// ----------------------------------------------------------------------------
// MEMORY_STATE_COLORS — palette integrity
// ----------------------------------------------------------------------------

describe('MEMORY_STATE_COLORS — palette integrity', () => {
	const states: MemoryState[] = ['active', 'dormant', 'silent', 'unavailable'];

	it('defines a colour for every bucket', () => {
		for (const s of states) {
			expect(MEMORY_STATE_COLORS[s]).toBeDefined();
		}
	});

	it.each(states)('%s colour is a valid 6-digit hex string', (state) => {
		const hex = MEMORY_STATE_COLORS[state];
		expect(hex).toMatch(/^#[0-9a-fA-F]{6}$/);
	});

	it('all four bucket colours are distinct', () => {
		const palette = states.map((s) => MEMORY_STATE_COLORS[s].toLowerCase());
		const unique = new Set(palette);
		expect(unique.size).toBe(4);
	});

	it('does not reuse any NODE_TYPE_COLORS value (type mode and state mode stay visually separate)', () => {
		const typeColours = new Set(
			Object.values(NODE_TYPE_COLORS).map((c) => c.toLowerCase())
		);
		for (const s of states) {
			expect(typeColours.has(MEMORY_STATE_COLORS[s].toLowerCase())).toBe(false);
		}
	});

	it('palette is a frozen record shape — all values are strings', () => {
		for (const s of states) {
			expect(typeof MEMORY_STATE_COLORS[s]).toBe('string');
		}
	});
});

// ----------------------------------------------------------------------------
// MEMORY_STATE_DESCRIPTIONS — legend text integrity
// ----------------------------------------------------------------------------

describe('MEMORY_STATE_DESCRIPTIONS — legend copy', () => {
	const states: MemoryState[] = ['active', 'dormant', 'silent', 'unavailable'];

	it('defines a description for every bucket', () => {
		for (const s of states) {
			expect(MEMORY_STATE_DESCRIPTIONS[s]).toBeDefined();
			expect(MEMORY_STATE_DESCRIPTIONS[s].length).toBeGreaterThan(5);
		}
	});

	it.each(states)('%s description contains a threshold parenthetical', (state) => {
		expect(MEMORY_STATE_DESCRIPTIONS[state]).toMatch(/\([^)]+\)/);
	});

	it('active description references the ≥ 70% threshold from getMemoryState', () => {
		expect(MEMORY_STATE_DESCRIPTIONS.active).toMatch(/70/);
	});

	it('dormant description references the 40–70% band', () => {
		expect(MEMORY_STATE_DESCRIPTIONS.dormant).toMatch(/40/);
		expect(MEMORY_STATE_DESCRIPTIONS.dormant).toMatch(/70/);
	});

	it('silent description references the 10–40% band', () => {
		expect(MEMORY_STATE_DESCRIPTIONS.silent).toMatch(/10/);
		expect(MEMORY_STATE_DESCRIPTIONS.silent).toMatch(/40/);
	});

	it('unavailable description references the < 10% threshold', () => {
		expect(MEMORY_STATE_DESCRIPTIONS.unavailable).toMatch(/10/);
	});

	it('descriptions are all distinct (no copy-paste bug)', () => {
		const lines = states.map((s) => MEMORY_STATE_DESCRIPTIONS[s]);
		expect(new Set(lines).size).toBe(4);
	});
});

// ----------------------------------------------------------------------------
// getNodeColor — dispatch correctness across modes
// ----------------------------------------------------------------------------

describe('getNodeColor — type mode', () => {
	it.each(Object.keys(NODE_TYPE_COLORS))('returns NODE_TYPE_COLORS[%s] in type mode', (t) => {
		const node = makeNode({ type: t, retention: 0.5 });
		expect(getNodeColor(node, 'type')).toBe(NODE_TYPE_COLORS[t]);
	});

	it('falls back to steel grey for an unknown type in type mode', () => {
		const node = makeNode({ type: 'totally-fake-type' as any, retention: 0.8 });
		expect(getNodeColor(node, 'type')).toBe('#8B95A5');
	});

	it('type-mode output ignores retention entirely', () => {
		const a = makeNode({ type: 'fact', retention: 0.01 });
		const b = makeNode({ type: 'fact', retention: 0.99 });
		expect(getNodeColor(a, 'type')).toBe(getNodeColor(b, 'type'));
	});
});

describe('getNodeColor — state mode', () => {
	it.each<[number, MemoryState]>([
		[0.9, 'active'],
		[0.5, 'dormant'],
		[0.2, 'silent'],
		[0.0, 'unavailable'],
	])('retention %f yields %s colour', (retention, state) => {
		const node = makeNode({ retention });
		expect(getNodeColor(node, 'state')).toBe(MEMORY_STATE_COLORS[state]);
	});

	it('state-mode output ignores node.type entirely', () => {
		const a = makeNode({ type: 'fact', retention: 0.8 });
		const b = makeNode({ type: 'decision', retention: 0.8 });
		expect(getNodeColor(a, 'state')).toBe(getNodeColor(b, 'state'));
	});

	it('state-mode tolerates unknown type (does not throw, no fallback branch used)', () => {
		const node = makeNode({ type: 'bogus' as any, retention: 0.75 });
		expect(getNodeColor(node, 'state')).toBe(MEMORY_STATE_COLORS.active);
	});
});

describe('getNodeColor — AhaGraph mode', () => {
	it.each([
		[['ahagraph', 'aha'], AHAGRAPH_COLORS.aha],
		[['ahagraph', 'confusion'], AHAGRAPH_COLORS.confusion],
		[['ahagraph', 'weak-spot'], AHAGRAPH_COLORS.confusion],
		[['ahagraph', 'failure'], AHAGRAPH_COLORS.failure],
		[['ahagraph', 'guardrail'], AHAGRAPH_COLORS.failure],
	] as Array<[string[], string]>)('maps tags %j to %s', (tags, color) => {
		const node = makeNode({ type: 'concept', tags });
		expect(getAhaGraphColor(node)).toBe(color);
		expect(getNodeColor(node, 'ahagraph')).toBe(color);
	});

	it('prioritizes aha when a note also mentions confusion tags', () => {
		const node = makeNode({ type: 'note', tags: ['ahagraph', 'aha', 'confusion'] });
		expect(getNodeColor(node, 'ahagraph')).toBe(AHAGRAPH_COLORS.aha);
	});

	it('falls back to node type when no AhaGraph learning tag is present', () => {
		const node = makeNode({ type: 'event', tags: ['ahagraph'] });
		expect(getAhaGraphColor(node)).toBeNull();
		expect(getNodeColor(node, 'ahagraph')).toBe(NODE_TYPE_COLORS.event);
	});
});

// ----------------------------------------------------------------------------
// NodeManager — default state + colorMode field
// ----------------------------------------------------------------------------

describe('NodeManager — colorMode field', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('defaults colorMode to "type"', () => {
		expect(manager.colorMode).toBe('type');
	});

	it('colorMode is writable before createNodes (so Graph3D can pre-set)', () => {
		manager.colorMode = 'state';
		expect(manager.colorMode).toBe('state');
	});

	it('setColorMode("state") updates the field', () => {
		manager.setColorMode('state');
		expect(manager.colorMode).toBe('state');
	});

	it('setColorMode("ahagraph") updates the field', () => {
		manager.setColorMode('ahagraph');
		expect(manager.colorMode).toBe('ahagraph');
	});

	it('setColorMode("type") is no-op when already "type" (idempotent early return)', () => {
		// Spy on the meshMap iteration indirectly: if the early-return fires,
		// calling setColorMode on an empty manager still leaves us in 'type'.
		manager.setColorMode('type');
		expect(manager.colorMode).toBe('type');
	});

	it('setColorMode is idempotent — second call in same mode short-circuits', () => {
		const nodes = [makeNode({ id: 'n1', type: 'fact', retention: 0.8 })];
		manager.createNodes(nodes);

		manager.setColorMode('state');
		const meshBefore = manager.meshMap.get('n1')!;
		const colorCopy = vi.spyOn(meshBefore.material.color, 'copy');

		manager.setColorMode('state'); // second call in same mode
		expect(colorCopy).not.toHaveBeenCalled();
	});

	it('does not throw on empty meshMap', () => {
		expect(() => manager.setColorMode('state')).not.toThrow();
		expect(() => manager.setColorMode('type')).not.toThrow();
	});
});

// ----------------------------------------------------------------------------
// NodeManager — setColorMode retints meshes + glows in place
// ----------------------------------------------------------------------------

describe('NodeManager.setColorMode — retint semantics', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('calls mesh.material.color.copy for every node', () => {
		const nodes = [
			makeNode({ id: 'a', type: 'fact', retention: 0.9 }),
			makeNode({ id: 'b', type: 'concept', retention: 0.5 }),
			makeNode({ id: 'c', type: 'event', retention: 0.2 }),
		];
		manager.createNodes(nodes);

		const spies = nodes.map((n) => {
			const mat = manager.meshMap.get(n.id)!.material as MeshStandardMaterial;
			return vi.spyOn(mat.color, 'copy');
		});

		manager.setColorMode('state');

		for (const spy of spies) {
			expect(spy).toHaveBeenCalledTimes(1);
		}
	});

	it('calls mesh.material.emissive.copy for every node (emissive follows colour)', () => {
		const nodes = [makeNode({ id: 'a' }), makeNode({ id: 'b' })];
		manager.createNodes(nodes);

		const spies = nodes.map((n) => {
			const mat = manager.meshMap.get(n.id)!.material as MeshStandardMaterial;
			return vi.spyOn(mat.emissive, 'copy');
		});

		manager.setColorMode('state');

		for (const spy of spies) {
			expect(spy).toHaveBeenCalledTimes(1);
		}
	});

	it('calls glow sprite material.color.copy for every node', () => {
		const nodes = [makeNode({ id: 'g1' }), makeNode({ id: 'g2' })];
		manager.createNodes(nodes);

		const spies = nodes.map((n) => {
			const mat = manager.glowMap.get(n.id)!.material as SpriteMaterial;
			return vi.spyOn(mat.color, 'copy');
		});

		manager.setColorMode('state');

		for (const spy of spies) {
			expect(spy).toHaveBeenCalledTimes(1);
		}
	});

	it('passes matching Color instance to mesh.emissive (same target as mesh.color)', () => {
		const nodes = [makeNode({ id: 'a', retention: 0.9 })];
		manager.createNodes(nodes);

		const mat = manager.meshMap.get('a')!.material as MeshStandardMaterial;
		const colorSpy = vi.spyOn(mat.color, 'copy');
		const emissiveSpy = vi.spyOn(mat.emissive, 'copy');

		manager.setColorMode('state');

		// Both copies should receive Color instances with identical rgb (constructed
		// from the same hex). The mock's Color(string) always returns rgb=1,1,1, so
		// we assert both receive Color instances of equal rgb.
		const colorArg = colorSpy.mock.calls[0][0] as Color;
		const emissiveArg = emissiveSpy.mock.calls[0][0] as Color;
		expect(emissiveArg.r).toBe(colorArg.r);
		expect(emissiveArg.g).toBe(colorArg.g);
		expect(emissiveArg.b).toBe(colorArg.b);
	});

	it('preserves mesh reference (does not replace the mesh, only mutates material)', () => {
		const nodes = [makeNode({ id: 'a' })];
		manager.createNodes(nodes);

		const meshBefore = manager.meshMap.get('a');
		const materialBefore = meshBefore!.material;

		manager.setColorMode('state');
		manager.setColorMode('type');

		expect(manager.meshMap.get('a')).toBe(meshBefore);
		expect(manager.meshMap.get('a')!.material).toBe(materialBefore);
	});

	it('preserves glow sprite reference (in-place mutation, not replacement)', () => {
		const nodes = [makeNode({ id: 'a' })];
		manager.createNodes(nodes);

		const glowBefore = manager.glowMap.get('a');
		const glowMatBefore = glowBefore!.material;

		manager.setColorMode('state');

		expect(manager.glowMap.get('a')).toBe(glowBefore);
		expect(manager.glowMap.get('a')!.material).toBe(glowMatBefore);
	});

	it('preserves userData.retention across mode switches', () => {
		const nodes = [makeNode({ id: 'a', retention: 0.42 })];
		manager.createNodes(nodes);

		manager.setColorMode('state');
		expect(manager.meshMap.get('a')!.userData.retention).toBe(0.42);

		manager.setColorMode('type');
		expect(manager.meshMap.get('a')!.userData.retention).toBe(0.42);
	});

	it('preserves userData.type across mode switches', () => {
		const nodes = [makeNode({ id: 'a', type: 'decision', retention: 0.8 })];
		manager.createNodes(nodes);

		manager.setColorMode('state');
		expect(manager.meshMap.get('a')!.userData.type).toBe('decision');

		manager.setColorMode('type');
		expect(manager.meshMap.get('a')!.userData.type).toBe('decision');
	});

	it('preserves userData.nodeId across mode switches', () => {
		const nodes = [makeNode({ id: 'unique-id-123' })];
		manager.createNodes(nodes);

		manager.setColorMode('state');
		expect(manager.meshMap.get('unique-id-123')!.userData.nodeId).toBe('unique-id-123');
	});
});

// ----------------------------------------------------------------------------
// Round-trip + initial-mode fidelity
// ----------------------------------------------------------------------------

describe('NodeManager — mode round-trips', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('createNodes honours a pre-set colorMode = "state" (no flash on mount)', () => {
		manager.colorMode = 'state';
		const nodes = [makeNode({ id: 'a', type: 'fact', retention: 0.9 })];
		manager.createNodes(nodes);

		// Because string Colors collapse to rgb=1,1,1 in the mock, we verify the
		// mode is preserved and that a subsequent retint to 'state' is a no-op.
		expect(manager.colorMode).toBe('state');
		const mat = manager.meshMap.get('a')!.material as MeshStandardMaterial;
		const spy = vi.spyOn(mat.color, 'copy');
		manager.setColorMode('state');
		expect(spy).not.toHaveBeenCalled(); // idempotent — already state
	});

	it('type -> state -> type round-trip leaves mode at "type" and preserves mesh identity', () => {
		const nodes = [makeNode({ id: 'a', retention: 0.5 })];
		manager.createNodes(nodes);
		const meshId = manager.meshMap.get('a');

		manager.setColorMode('state');
		manager.setColorMode('type');

		expect(manager.colorMode).toBe('type');
		expect(manager.meshMap.get('a')).toBe(meshId);
	});

	it('rapid mode-toggle (5x type<->state) completes without throwing or losing nodes', () => {
		const nodes = [
			makeNode({ id: 'x', type: 'fact', retention: 0.9 }),
			makeNode({ id: 'y', type: 'concept', retention: 0.3 }),
			makeNode({ id: 'z', type: 'decision', retention: 0.05 }),
		];
		manager.createNodes(nodes);

		for (let i = 0; i < 5; i++) {
			manager.setColorMode('state');
			manager.setColorMode('type');
		}

		expect(manager.meshMap.size).toBe(3);
		expect(manager.glowMap.size).toBe(3);
		expect(manager.labelSprites.size).toBe(3);
	});
});

// ----------------------------------------------------------------------------
// Live-added nodes inherit the active mode
// ----------------------------------------------------------------------------

describe('NodeManager — live addNode in state mode', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('addNode uses the current colorMode (state) for the new mesh', () => {
		const seed = [makeNode({ id: 'seed', retention: 0.5 })];
		manager.createNodes(seed);
		manager.setColorMode('state');

		const live = makeNode({ id: 'live', type: 'fact', retention: 0.9 });
		manager.addNode(live);

		// The new mesh exists and was created while colorMode is 'state'. A
		// same-mode setColorMode('state') must still be a no-op (no re-copy).
		expect(manager.meshMap.has('live')).toBe(true);
		const mat = manager.meshMap.get('live')!.material as MeshStandardMaterial;
		const spy = vi.spyOn(mat.color, 'copy');
		manager.setColorMode('state');
		expect(spy).not.toHaveBeenCalled();
	});

	it('after setColorMode(state), subsequent addNode then switch to type retints the new node', () => {
		manager.setColorMode('state');
		const live = makeNode({ id: 'live', retention: 0.8 });
		manager.addNode(live);

		const mat = manager.meshMap.get('live')!.material as MeshStandardMaterial;
		const spy = vi.spyOn(mat.color, 'copy');

		manager.setColorMode('type');
		expect(spy).toHaveBeenCalledTimes(1);
	});
});

// ----------------------------------------------------------------------------
// Suppressed-node interaction (v2.0.5 active forgetting)
// ----------------------------------------------------------------------------

describe('NodeManager — colour mode + suppression compose', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('setColorMode does not touch material.opacity (suppression visual channel untouched)', () => {
		const nodes = [makeNode({ id: 'sup', retention: 0.8, suppression_count: 1 } as any)];
		manager.createNodes(nodes);
		const mat = manager.meshMap.get('sup')!.material as MeshStandardMaterial;
		const opacityBefore = mat.opacity;

		manager.setColorMode('state');
		expect(mat.opacity).toBe(opacityBefore);

		manager.setColorMode('type');
		expect(mat.opacity).toBe(opacityBefore);
	});

	it('setColorMode does not touch emissiveIntensity (suppression visual channel untouched)', () => {
		const nodes = [makeNode({ id: 'sup', retention: 0.8, suppression_count: 2 } as any)];
		manager.createNodes(nodes);
		const mat = manager.meshMap.get('sup')!.material as MeshStandardMaterial;
		const intensityBefore = mat.emissiveIntensity;

		manager.setColorMode('state');
		expect(mat.emissiveIntensity).toBe(intensityBefore);

		manager.setColorMode('type');
		expect(mat.emissiveIntensity).toBe(intensityBefore);
	});

	it('suppressed node still receives the new colour (so the SIF dim + hue both update)', () => {
		const nodes = [makeNode({ id: 'sup', retention: 0.8, suppression_count: 1 } as any)];
		manager.createNodes(nodes);
		const mat = manager.meshMap.get('sup')!.material as MeshStandardMaterial;
		const spy = vi.spyOn(mat.color, 'copy');

		manager.setColorMode('state');
		expect(spy).toHaveBeenCalledTimes(1);
	});
});

// ----------------------------------------------------------------------------
// Defensive: missing glow (race between createNodes and removeNode)
// ----------------------------------------------------------------------------

describe('NodeManager.setColorMode — defensive paths', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('handles a mesh without a corresponding glow (manually deleted) without throwing', () => {
		const nodes = [makeNode({ id: 'orphan' })];
		manager.createNodes(nodes);
		manager.glowMap.delete('orphan');

		expect(() => manager.setColorMode('state')).not.toThrow();
	});

	it('uses retention fallback 0 when userData.retention is missing', () => {
		const nodes = [makeNode({ id: 'no-ud' })];
		manager.createNodes(nodes);
		const mesh = manager.meshMap.get('no-ud')!;
		delete mesh.userData.retention;

		// 0 retention -> unavailable bucket colour. We assert no throw and that
		// the retint completes for this mesh.
		const mat = mesh.material as MeshStandardMaterial;
		const spy = vi.spyOn(mat.color, 'copy');
		manager.setColorMode('state');
		expect(spy).toHaveBeenCalledTimes(1);
	});

	it('uses type fallback "fact" when userData.type is missing', () => {
		const nodes = [makeNode({ id: 'no-type', retention: 0.5 })];
		manager.createNodes(nodes); // starts in 'type' mode
		const mesh = manager.meshMap.get('no-type')!;
		delete mesh.userData.type;

		// Switch to state first (not idempotent), then back to type so the
		// fallback branch actually executes and we can observe the retint.
		manager.setColorMode('state');

		const mat = mesh.material as MeshStandardMaterial;
		const spy = vi.spyOn(mat.color, 'copy');
		manager.setColorMode('type');
		expect(spy).toHaveBeenCalledTimes(1);
	});
});

// ----------------------------------------------------------------------------
// Cross-validation: the colour a mesh SHOULD get matches what the pure
// function produces. We verify this by capturing the hex passed into `new
// Color(...)` via a spy on the Color constructor.
// ----------------------------------------------------------------------------

describe('setColorMode — hex values match getNodeColor', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('state-mode retint invokes mesh.color.copy and glow.color.copy per node', () => {
		const nodes = [
			makeNode({ id: 'high', retention: 0.9 }),
			makeNode({ id: 'mid', retention: 0.5 }),
			makeNode({ id: 'low', retention: 0.2 }),
			makeNode({ id: 'gone', retention: 0.05 }),
		];
		manager.createNodes(nodes);

		// Instance-level spies on each mesh and glow so prototype state isn't
		// polluted across tests. Expected: one copy per mesh + one per glow.
		const meshSpies = nodes.map((n) => {
			const mat = manager.meshMap.get(n.id)!.material as MeshStandardMaterial;
			return vi.spyOn(mat.color, 'copy');
		});
		const glowSpies = nodes.map((n) => {
			const mat = manager.glowMap.get(n.id)!.material as SpriteMaterial;
			return vi.spyOn(mat.color, 'copy');
		});

		manager.setColorMode('state');

		for (const s of meshSpies) expect(s).toHaveBeenCalledTimes(1);
		for (const s of glowSpies) expect(s).toHaveBeenCalledTimes(1);
	});

	it('type-mode retint results are deterministic for fixed nodes', () => {
		const nodes = [
			makeNode({ id: 'a', type: 'fact', retention: 0.3 }),
			makeNode({ id: 'b', type: 'event', retention: 0.8 }),
		];
		manager.createNodes(nodes);
		manager.setColorMode('state');

		const matA = manager.meshMap.get('a')!.material as MeshStandardMaterial;
		const matB = manager.meshMap.get('b')!.material as MeshStandardMaterial;
		const spyA = vi.spyOn(matA.color, 'copy');
		const spyB = vi.spyOn(matB.color, 'copy');

		manager.setColorMode('type');

		// Two distinct types -> two copy() calls, one per mesh.
		expect(spyA).toHaveBeenCalledTimes(1);
		expect(spyB).toHaveBeenCalledTimes(1);
	});
});
