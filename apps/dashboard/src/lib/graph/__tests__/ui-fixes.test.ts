/**
 * Regression tests for vestige issue #31 (v2.0.6 Phase 1 dashboard UI fix).
 *
 * Before v2.0.6 the graph view rendered "glowing cubes" instead of round halos,
 * with navy edges swallowed by heavy fog. Root cause: the node glow SpriteMaterial
 * had no `map` set, so THREE.Sprite rendered as a solid-coloured plane whose
 * square edges were then amplified by UnrealBloomPass into hard bright squares.
 *
 * These tests lock in every property that was broken so any regression surfaces
 * as a red test instead of shipping another ugly screenshot into the issue tracker.
 *
 * The scene.ts assertions are intentionally source-level (fs regex) because the
 * real scene.ts pulls in three/addons (OrbitControls, EffectComposer, UnrealBloomPass,
 * WebGLRenderer) which are painful to mock in isolation. Reading the .ts file and
 * regex-checking the magic numbers catches accidental revert/tweaks without needing
 * a full WebGL harness.
 */
import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

vi.mock('three', async () => {
	const mock = await import('./three-mock');
	return { ...mock };
});

import { NodeManager } from '../nodes';
import { EdgeManager } from '../edges';
import { Vector3, AdditiveBlending } from './three-mock';
import { makeNode, makeEdge, resetNodeCounter } from './helpers';

// ---------------------------------------------------------------------------
// 1. Node glow sprite — THE fix for the "glowing cubes" artifact
// ---------------------------------------------------------------------------
describe('issue #31 — node glow sprites render as round halos, not squares', () => {
	let manager: NodeManager;

	beforeEach(() => {
		resetNodeCounter();
		manager = new NodeManager();
	});

	it('glow SpriteMaterial has a map set (the root cause of the square artifact)', () => {
		manager.createNodes([makeNode({ id: 'a', retention: 0.8 })]);
		const glow = manager.glowMap.get('a')!;
		const mat = glow.material as any;

		// Without a map, THREE.Sprite renders as a solid coloured plane —
		// additive blend + bloom then turns it into a glowing square.
		// The fix generates a shared radial-gradient CanvasTexture and assigns
		// it here, so bloom has a soft circular shape to diffuse.
		expect(mat.map).not.toBeNull();
		expect(mat.map).toBeDefined();
	});

	it('glow sprites on multiple nodes SHARE the same texture instance (singleton cache)', () => {
		// The shared texture is a module-level cache — if a future refactor
		// accidentally creates one per-node we'll leak memory on large graphs.
		manager.createNodes([
			makeNode({ id: 'a' }),
			makeNode({ id: 'b' }),
			makeNode({ id: 'c' }),
		]);
		const a = (manager.glowMap.get('a')!.material as any).map;
		const b = (manager.glowMap.get('b')!.material as any).map;
		const c = (manager.glowMap.get('c')!.material as any).map;

		expect(a).toBe(b);
		expect(b).toBe(c);
	});

	it('glow sprite has depthWrite:false to prevent z-fighting with the sphere behind it', () => {
		manager.createNodes([makeNode({ id: 'a' })]);
		const mat = manager.glowMap.get('a')!.material as any;
		expect(mat.depthWrite).toBe(false);
	});

	it('glow sprite uses additive blending (required for bloom to read as light)', () => {
		manager.createNodes([makeNode({ id: 'a' })]);
		const mat = manager.glowMap.get('a')!.material as any;
		expect(mat.blending).toBe(AdditiveBlending);
	});

	it('glow sprite scale uses the new 6× multiplier (was 4× — gradient needed more footprint)', () => {
		// size = 0.5 + retention*2 → 0.5 + 1.0*2 = 2.5
		// glow scale with new formula: 2.5 * 6 * 1.0 = 15
		manager.createNodes([makeNode({ id: 'full', retention: 1.0 })]);
		const glow = manager.glowMap.get('full')!;
		expect(glow.scale.x).toBeCloseTo(15, 5);
		expect(glow.scale.y).toBeCloseTo(15, 5);
	});

	it('glow sprite base opacity is 0.3 + retention*0.35 (was 0.15 + retention*0.2)', () => {
		manager.createNodes([makeNode({ id: 'full', retention: 1.0 })]);
		const mat = manager.glowMap.get('full')!.material as any;
		// 0.3 + 1.0 * 0.35 = 0.65
		expect(mat.opacity).toBeCloseTo(0.65, 5);
	});

	it('suppressed node glow opacity drops to 0.1 (v2.0.5 active forgetting)', () => {
		manager.createNodes([makeNode({ id: 's', retention: 0.8, suppression_count: 2 })]);
		const mat = manager.glowMap.get('s')!.material as any;
		expect(mat.opacity).toBeCloseTo(0.1, 5);
	});
});

// ---------------------------------------------------------------------------
// 2. Edge materials — dark navy → brand violet, higher opacity
// ---------------------------------------------------------------------------
describe('issue #31 — edges are brand violet and actually visible', () => {
	let manager: EdgeManager;
	let positions: Map<string, InstanceType<typeof Vector3>>;

	beforeEach(() => {
		manager = new EdgeManager();
		positions = new Map([
			['a', new Vector3(0, 0, 0)],
			['b', new Vector3(10, 0, 0)],
		]);
	});

	it('edge color is brand violet 0x8b5cf6, not the old dark navy 0x4a4a7a', () => {
		manager.createEdges([makeEdge('a', 'b', { weight: 0.5 })], positions);
		const line = manager.group.children[0] as any;
		const c = line.material.color;

		// 0x8b5cf6 → r=139/255, g=92/255, b=246/255
		expect(c.r).toBeCloseTo(0x8b / 255, 3);
		expect(c.g).toBeCloseTo(0x5c / 255, 3);
		expect(c.b).toBeCloseTo(0xf6 / 255, 3);

		// And definitely NOT the old navy 0x4a4a7a (74/255, 74/255, 122/255)
		expect(c.r).not.toBeCloseTo(0x4a / 255, 3);
	});

	it('edges have depthWrite:false so they additively blend through fog cleanly', () => {
		manager.createEdges([makeEdge('a', 'b')], positions);
		const line = manager.group.children[0] as any;
		expect(line.material.depthWrite).toBe(false);
	});

	it('edge opacity base is 0.25 + weight*0.5 (was 0.1 + weight*0.5)', () => {
		manager.createEdges([makeEdge('a', 'b', { weight: 0.5 })], positions);
		const line = manager.group.children[0] as any;
		// 0.25 + 0.5 * 0.5 = 0.5
		expect(line.material.opacity).toBeCloseTo(0.5, 5);
	});

	it('edge opacity with low weight still reads (new floor catches regressions)', () => {
		manager.createEdges([makeEdge('a', 'b', { weight: 0.0 })], positions);
		const line = manager.group.children[0] as any;
		// Floor is 0.25 — used to be 0.1 which was invisible through fog
		expect(line.material.opacity).toBeGreaterThanOrEqual(0.25);
	});

	it('edge opacity cap is 0.8 (was 0.6)', () => {
		manager.createEdges([makeEdge('a', 'b', { weight: 100.0 })], positions);
		const line = manager.group.children[0] as any;
		expect(line.material.opacity).toBeCloseTo(0.8, 5);
	});
});

// ---------------------------------------------------------------------------
// 3. Scene config — source-level regex assertions (scene.ts needs three/addons)
// ---------------------------------------------------------------------------
describe('issue #31 — scene.ts bloom/fog/starfield config is locked in', () => {
	const __dirname = dirname(fileURLToPath(import.meta.url));
	let src: string;

	beforeAll(() => {
		src = readFileSync(resolve(__dirname, '../scene.ts'), 'utf-8');
	});

	it('fog density is reduced from 0.008 → 0.0035', () => {
		// Positive match: the new density appears inside a FogExp2 call
		expect(src).toMatch(/FogExp2\(\s*0x[0-9a-f]+,\s*0\.0035/i);
		// Negative match: the old aggressive density is gone
		expect(src).not.toMatch(/FogExp2\(\s*0x[0-9a-f]+,\s*0\.008\b/i);
	});

	it('bloom strength is 0.55 (was 0.8 — was blown out)', () => {
		// Match on the constructor signature: (size, strength, radius, threshold)
		expect(src).toMatch(
			/new UnrealBloomPass\([\s\S]*?,\s*0\.55,\s*0\.6,\s*0\.2\s*\)/
		);
		// Old values must be gone
		expect(src).not.toMatch(/new UnrealBloomPass\([\s\S]*?,\s*0\.8,\s*0\.4,\s*0\.85\s*\)/);
	});

	it('scene.background is explicitly set (not left as default black void)', () => {
		expect(src).toMatch(/scene\.background\s*=/);
	});

	it('a starfield is created and added to the scene', () => {
		// createStarfield helper exists and is called at least once
		expect(src).toMatch(/function\s+createStarfield\s*\(/);
		expect(src).toMatch(/createStarfield\s*\(\s*\)/);
		expect(src).toMatch(/scene\.add\(\s*starfield\s*\)/);
	});

	it('starfield is exposed on SceneContext (so dispose/update can touch it later)', () => {
		expect(src).toMatch(/starfield:\s*THREE\.Points/);
	});

	it('ACESFilmicToneMapping still active (did not accidentally revert tone map)', () => {
		expect(src).toMatch(/ACESFilmicToneMapping/);
	});
});

// ---------------------------------------------------------------------------
// 4. Source-level checks on nodes.ts — the shared glow texture helper
// ---------------------------------------------------------------------------
describe('issue #31 — nodes.ts glow texture helper exists and is a singleton', () => {
	const __dirname = dirname(fileURLToPath(import.meta.url));
	let src: string;

	beforeAll(() => {
		src = readFileSync(resolve(__dirname, '../nodes.ts'), 'utf-8');
	});

	it('shared glow texture cache exists at module level', () => {
		expect(src).toMatch(/let\s+sharedGlowTexture/);
		expect(src).toMatch(/function\s+getGlowTexture\s*\(/);
	});

	it('radial gradient has a transparent outer stop (not hard edge)', () => {
		// The key insight — colour stops must go to rgba(255,255,255,0) at the edge
		expect(src).toMatch(/createRadialGradient/);
		expect(src).toMatch(/rgba\(255,\s*255,\s*255,\s*0(?:\.0)?\)/);
	});

	it('SpriteMaterial is constructed with a map parameter', () => {
		expect(src).toMatch(/new THREE\.SpriteMaterial\(\{[\s\S]*?map:\s*getGlowTexture\(\)/);
	});
});
