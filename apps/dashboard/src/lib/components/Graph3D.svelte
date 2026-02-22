<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as THREE from 'three';
	import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
	import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
	import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
	import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
	import type { GraphNode, GraphEdge, VestigeEvent } from '$types';
	import { NODE_TYPE_COLORS } from '$types';

	interface Props {
		nodes: GraphNode[];
		edges: GraphEdge[];
		centerId: string;
		events?: VestigeEvent[];
		isDreaming?: boolean;
		onSelect?: (nodeId: string) => void;
	}

	let { nodes, edges, centerId, events = [], isDreaming = false, onSelect }: Props = $props();

	let container: HTMLDivElement;
	let renderer: THREE.WebGLRenderer;
	let scene: THREE.Scene;
	let camera: THREE.PerspectiveCamera;
	let controls: OrbitControls;
	let composer: EffectComposer;
	let bloomPass: UnrealBloomPass;
	let raycaster: THREE.Raycaster;
	let mouse: THREE.Vector2;
	let animationId: number;
	let nodeGroup: THREE.Group;
	let edgeGroup: THREE.Group;
	let particleSystem: THREE.Points;
	let starField: THREE.Points;

	// Maps for lookup
	let nodeMeshMap = new Map<string, THREE.Mesh>();
	let nodePositions = new Map<string, THREE.Vector3>();
	let labelSprites = new Map<string, THREE.Sprite>();
	let hoveredNode: string | null = null;
	let selectedNode: string | null = null;

	// Force simulation state
	let velocities = new Map<string, THREE.Vector3>();
	let simulationRunning = true;
	let simulationStep = 0;

	// Event-driven animation state
	let processedEventCount = 0;
	let pulseEffects: { nodeId: string; intensity: number; color: THREE.Color; decay: number }[] = [];
	let connectionFlashes: { line: THREE.Line; intensity: number }[] = [];
	let spawnBursts: { position: THREE.Vector3; age: number; particles: THREE.Points }[] = [];
	let dreamTrails: { points: THREE.Vector3[]; line: THREE.Line; age: number }[] = [];
	let shockwaves: { mesh: THREE.Mesh; age: number; maxAge: number }[] = [];

	onMount(() => {
		initScene();
		createStarField();
		createGraph();
		createParticleSystem();
		animate();

		window.addEventListener('resize', onResize);
		container.addEventListener('pointermove', onPointerMove);
		container.addEventListener('click', onClick);
	});

	onDestroy(() => {
		cancelAnimationFrame(animationId);
		window.removeEventListener('resize', onResize);
		container?.removeEventListener('pointermove', onPointerMove);
		container?.removeEventListener('click', onClick);
		// Dispose Three.js resources to prevent GPU memory leaks
		scene?.traverse((obj: THREE.Object3D) => {
			if (obj instanceof THREE.Mesh || obj instanceof THREE.InstancedMesh) {
				obj.geometry?.dispose();
				if (Array.isArray(obj.material)) {
					obj.material.forEach((m: THREE.Material) => m.dispose());
				} else if (obj.material) {
					(obj.material as THREE.Material).dispose();
				}
			}
		});
		renderer?.dispose();
		composer?.dispose();
	});

	function initScene() {
		// Scene
		scene = new THREE.Scene();
		scene.fog = new THREE.FogExp2(0x050510, 0.008);

		// Camera
		camera = new THREE.PerspectiveCamera(60, container.clientWidth / container.clientHeight, 0.1, 2000);
		camera.position.set(0, 30, 80);

		// Renderer (WebGL2 — WebGPU requires async init, use WebGL for now with bloom)
		renderer = new THREE.WebGLRenderer({
			antialias: true,
			alpha: true,
			powerPreference: 'high-performance'
		});
		renderer.setSize(container.clientWidth, container.clientHeight);
		renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
		renderer.toneMapping = THREE.ACESFilmicToneMapping;
		renderer.toneMappingExposure = 1.2;
		container.appendChild(renderer.domElement);

		// Controls
		controls = new OrbitControls(camera, renderer.domElement);
		controls.enableDamping = true;
		controls.dampingFactor = 0.05;
		controls.rotateSpeed = 0.5;
		controls.zoomSpeed = 0.8;
		controls.minDistance = 10;
		controls.maxDistance = 500;
		controls.autoRotate = true;
		controls.autoRotateSpeed = 0.3;

		// Post-processing: Bloom
		composer = new EffectComposer(renderer);
		composer.addPass(new RenderPass(scene, camera));
		bloomPass = new UnrealBloomPass(
			new THREE.Vector2(container.clientWidth, container.clientHeight),
			0.8,  // strength
			0.4,  // radius
			0.85  // threshold
		);
		composer.addPass(bloomPass);

		// Lighting
		const ambient = new THREE.AmbientLight(0x1a1a3a, 0.5);
		scene.add(ambient);

		const point1 = new THREE.PointLight(0x6366f1, 1.5, 200);
		point1.position.set(50, 50, 50);
		scene.add(point1);

		const point2 = new THREE.PointLight(0xa855f7, 1, 200);
		point2.position.set(-50, -30, -50);
		scene.add(point2);

		// Raycaster
		raycaster = new THREE.Raycaster();
		raycaster.params.Points = { threshold: 2 };
		mouse = new THREE.Vector2();
	}

	function createStarField() {
		const starGeo = new THREE.BufferGeometry();
		const starCount = 3000;
		const positions = new Float32Array(starCount * 3);
		const sizes = new Float32Array(starCount);

		for (let i = 0; i < starCount; i++) {
			positions[i * 3] = (Math.random() - 0.5) * 1000;
			positions[i * 3 + 1] = (Math.random() - 0.5) * 1000;
			positions[i * 3 + 2] = (Math.random() - 0.5) * 1000;
			sizes[i] = Math.random() * 1.5;
		}

		starGeo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
		starGeo.setAttribute('size', new THREE.BufferAttribute(sizes, 1));

		const starMat = new THREE.PointsMaterial({
			color: 0x6366f1,
			size: 0.5,
			transparent: true,
			opacity: 0.4,
			sizeAttenuation: true,
			blending: THREE.AdditiveBlending
		});

		starField = new THREE.Points(starGeo, starMat);
		scene.add(starField);
	}

	function createGraph() {
		nodeGroup = new THREE.Group();
		edgeGroup = new THREE.Group();

		// Position nodes using force-directed initial layout
		const nodeCount = nodes.length;
		const phi = (1 + Math.sqrt(5)) / 2; // Golden ratio for sphere distribution

		nodes.forEach((node, i) => {
			// Fibonacci sphere distribution for initial positions
			const y = 1 - (2 * i) / (nodeCount - 1 || 1);
			const radius = Math.sqrt(1 - y * y);
			const theta = 2 * Math.PI * i / phi;
			const spread = 30 + nodeCount * 0.5;

			const pos = new THREE.Vector3(
				radius * Math.cos(theta) * spread,
				y * spread,
				radius * Math.sin(theta) * spread
			);

			// Center node at origin
			if (node.isCenter) pos.set(0, 0, 0);

			nodePositions.set(node.id, pos);
			velocities.set(node.id, new THREE.Vector3());

			// Create node mesh
			const size = 0.5 + node.retention * 2;
			const color = NODE_TYPE_COLORS[node.type] || '#6b7280';

			const geometry = new THREE.SphereGeometry(size, 16, 16);
			const material = new THREE.MeshStandardMaterial({
				color: new THREE.Color(color),
				emissive: new THREE.Color(color),
				emissiveIntensity: 0.3 + node.retention * 0.5,
				roughness: 0.3,
				metalness: 0.1,
				transparent: true,
				opacity: 0.3 + node.retention * 0.7,
			});

			const mesh = new THREE.Mesh(geometry, material);
			mesh.position.copy(pos);
			mesh.userData = { nodeId: node.id, type: node.type, retention: node.retention };

			nodeMeshMap.set(node.id, mesh);
			nodeGroup.add(mesh);

			// Glow sprite
			const spriteMat = new THREE.SpriteMaterial({
				color: new THREE.Color(color),
				transparent: true,
				opacity: 0.15 + node.retention * 0.2,
				blending: THREE.AdditiveBlending,
			});
			const sprite = new THREE.Sprite(spriteMat);
			sprite.scale.set(size * 4, size * 4, 1);
			sprite.position.copy(pos);
			sprite.userData = { isGlow: true, nodeId: node.id };
			nodeGroup.add(sprite);

			// Text label sprite (distance-faded)
			const labelText = node.label || node.type;
			const labelSprite = createTextSprite(labelText, '#e2e8f0');
			labelSprite.position.copy(pos);
			labelSprite.position.y += size * 2 + 1.5;
			labelSprite.userData = { isLabel: true, nodeId: node.id, offset: size * 2 + 1.5 };
			nodeGroup.add(labelSprite);
			labelSprites.set(node.id, labelSprite);
		});

		// Create edges
		edges.forEach(edge => {
			const sourcePos = nodePositions.get(edge.source);
			const targetPos = nodePositions.get(edge.target);
			if (!sourcePos || !targetPos) return;

			const points = [sourcePos, targetPos];
			const geometry = new THREE.BufferGeometry().setFromPoints(points);
			const material = new THREE.LineBasicMaterial({
				color: 0x4a4a7a,
				transparent: true,
				opacity: Math.min(0.1 + edge.weight * 0.5, 0.6),
				blending: THREE.AdditiveBlending,
			});

			const line = new THREE.Line(geometry, material);
			line.userData = { source: edge.source, target: edge.target };
			edgeGroup.add(line);
		});

		scene.add(edgeGroup);
		scene.add(nodeGroup);
	}

	function createParticleSystem() {
		const particleCount = 500;
		const geometry = new THREE.BufferGeometry();
		const positions = new Float32Array(particleCount * 3);
		const colors = new Float32Array(particleCount * 3);

		for (let i = 0; i < particleCount; i++) {
			positions[i * 3] = (Math.random() - 0.5) * 100;
			positions[i * 3 + 1] = (Math.random() - 0.5) * 100;
			positions[i * 3 + 2] = (Math.random() - 0.5) * 100;
			// Purple-blue neural particles
			colors[i * 3] = 0.4 + Math.random() * 0.3;
			colors[i * 3 + 1] = 0.3 + Math.random() * 0.2;
			colors[i * 3 + 2] = 0.8 + Math.random() * 0.2;
		}

		geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
		geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));

		const material = new THREE.PointsMaterial({
			size: 0.3,
			vertexColors: true,
			transparent: true,
			opacity: 0.4,
			blending: THREE.AdditiveBlending,
			sizeAttenuation: true,
		});

		particleSystem = new THREE.Points(geometry, material);
		scene.add(particleSystem);
	}

	function createTextSprite(text: string, color: string): THREE.Sprite {
		const canvas = document.createElement('canvas');
		const ctx = canvas.getContext('2d')!;
		canvas.width = 512;
		canvas.height = 64;

		// Truncate text
		const label = text.length > 40 ? text.slice(0, 37) + '...' : text;

		ctx.clearRect(0, 0, canvas.width, canvas.height);
		ctx.font = 'bold 28px -apple-system, BlinkMacSystemFont, sans-serif';
		ctx.textAlign = 'center';
		ctx.textBaseline = 'middle';

		// Shadow for readability
		ctx.shadowColor = 'rgba(0, 0, 0, 0.8)';
		ctx.shadowBlur = 6;
		ctx.shadowOffsetX = 0;
		ctx.shadowOffsetY = 2;

		ctx.fillStyle = color;
		ctx.fillText(label, canvas.width / 2, canvas.height / 2);

		const texture = new THREE.CanvasTexture(canvas);
		texture.needsUpdate = true;

		const mat = new THREE.SpriteMaterial({
			map: texture,
			transparent: true,
			opacity: 0,
			depthTest: false,
			sizeAttenuation: true,
		});

		const sprite = new THREE.Sprite(mat);
		sprite.scale.set(12, 1.5, 1);
		return sprite;
	}

	function runForceSimulation() {
		if (!simulationRunning || simulationStep > 300) return;
		simulationStep++;

		const alpha = Math.max(0.001, 1 - simulationStep / 300);
		const repulsionStrength = 500;
		const attractionStrength = 0.01;
		const dampening = 0.9;

		// Repulsion between all nodes
		const nodeIds = Array.from(nodePositions.keys());
		for (let i = 0; i < nodeIds.length; i++) {
			for (let j = i + 1; j < nodeIds.length; j++) {
				const posA = nodePositions.get(nodeIds[i])!;
				const posB = nodePositions.get(nodeIds[j])!;
				const diff = new THREE.Vector3().subVectors(posA, posB);
				const dist = diff.length() || 1;
				const force = repulsionStrength / (dist * dist) * alpha;
				const dir = diff.normalize().multiplyScalar(force);

				velocities.get(nodeIds[i])!.add(dir);
				velocities.get(nodeIds[j])!.sub(dir);
			}
		}

		// Attraction along edges
		edges.forEach(edge => {
			const posA = nodePositions.get(edge.source);
			const posB = nodePositions.get(edge.target);
			if (!posA || !posB) return;

			const diff = new THREE.Vector3().subVectors(posB, posA);
			const dist = diff.length();
			const force = dist * attractionStrength * edge.weight * alpha;
			const dir = diff.normalize().multiplyScalar(force);

			velocities.get(edge.source)!.add(dir);
			velocities.get(edge.target)!.sub(dir);
		});

		// Centering force
		nodeIds.forEach(id => {
			const pos = nodePositions.get(id)!;
			const vel = velocities.get(id)!;
			vel.sub(pos.clone().multiplyScalar(0.001 * alpha));
			vel.multiplyScalar(dampening);
			pos.add(vel);

			// Update mesh positions
			const mesh = nodeMeshMap.get(id);
			if (mesh) mesh.position.copy(pos);
		});

		// Update glow sprite and label positions
		nodeGroup.children.forEach(child => {
			if (child.userData.nodeId) {
				const pos = nodePositions.get(child.userData.nodeId);
				if (!pos) return;
				if (child.userData.isGlow) {
					child.position.copy(pos);
				} else if (child.userData.isLabel) {
					child.position.copy(pos);
					child.position.y += child.userData.offset;
				}
			}
		});

		// Update edge positions
		edgeGroup.children.forEach(child => {
			const line = child as THREE.Line;
			const sourcePos = nodePositions.get(line.userData.source);
			const targetPos = nodePositions.get(line.userData.target);
			if (sourcePos && targetPos) {
				const positions = line.geometry.attributes.position as THREE.BufferAttribute;
				positions.setXYZ(0, sourcePos.x, sourcePos.y, sourcePos.z);
				positions.setXYZ(1, targetPos.x, targetPos.y, targetPos.z);
				positions.needsUpdate = true;
			}
		});
	}

	function animate() {
		animationId = requestAnimationFrame(animate);

		const time = performance.now() * 0.001;

		// Force simulation
		runForceSimulation();

		// Animate particles
		if (particleSystem) {
			const positions = particleSystem.geometry.attributes.position as THREE.BufferAttribute;
			for (let i = 0; i < positions.count; i++) {
				positions.setY(i, positions.getY(i) + Math.sin(time + i * 0.1) * 0.02);
				positions.setX(i, positions.getX(i) + Math.cos(time + i * 0.05) * 0.01);
			}
			positions.needsUpdate = true;
		}

		// Slow star rotation
		if (starField) {
			starField.rotation.y += 0.0001;
			starField.rotation.x += 0.00005;
		}

		// Node breathing (retention-based pulse)
		nodeMeshMap.forEach((mesh, id) => {
			const node = nodes.find(n => n.id === id);
			if (!node) return;
			const breathe = 1 + Math.sin(time * 1.5 + nodes.indexOf(node) * 0.5) * 0.05 * node.retention;
			mesh.scale.setScalar(breathe);

			// Highlight hovered
			const mat = mesh.material as THREE.MeshStandardMaterial;
			if (id === hoveredNode) {
				mat.emissiveIntensity = 1.0;
			} else if (id === selectedNode) {
				mat.emissiveIntensity = 0.8;
			} else {
				mat.emissiveIntensity = 0.3 + node.retention * 0.5;
			}
		});

		// Distance-based label visibility
		labelSprites.forEach((sprite, id) => {
			const pos = nodePositions.get(id);
			if (!pos) return;
			const dist = camera.position.distanceTo(pos);
			const mat = sprite.material as THREE.SpriteMaterial;
			// Fade in when close (< 40 units), fade out when far (> 80 units)
			const targetOpacity = id === hoveredNode || id === selectedNode
				? 1.0
				: dist < 40 ? 0.9 : dist < 80 ? 0.9 * (1 - (dist - 40) / 40) : 0;
			mat.opacity += (targetOpacity - mat.opacity) * 0.1;
		});

		// Dream mode: slower rotation, purple tint, stronger bloom
		if (isDreaming) {
			controls.autoRotateSpeed = 0.1;
			bloomPass.strength = 1.5;
			scene.fog = new THREE.FogExp2(0x0a0520, 0.006);
		} else {
			controls.autoRotateSpeed = 0.3;
			bloomPass.strength = 0.8;
		}

		// Process incoming events
		processEvents();

		// Update visual effects
		updateEffects(time);

		controls.update();
		composer.render();
	}

	function processEvents() {
		if (!events || events.length <= processedEventCount) return;

		const newEvents = events.slice(processedEventCount);
		processedEventCount = events.length;

		for (const event of newEvents) {
			switch (event.type) {
				case 'MemoryCreated': {
					// Spawn burst: ring of particles expanding outward
					const nodeId = (event.data as { id?: string })?.id;
					const pos = nodeId ? nodePositions.get(nodeId) : null;
					const burstPos = pos?.clone() ?? new THREE.Vector3(
						(Math.random() - 0.5) * 40,
						(Math.random() - 0.5) * 40,
						(Math.random() - 0.5) * 40
					);
					createSpawnBurst(burstPos, new THREE.Color(0x10b981));

					// Also create a shockwave ring
					createShockwave(burstPos, new THREE.Color(0x10b981));
					break;
				}
				case 'SearchPerformed': {
					// Pulse all visible nodes with blue ripple
					const query = (event.data as { query?: string })?.query;
					nodeMeshMap.forEach((_, id) => {
						pulseEffects.push({
							nodeId: id,
							intensity: 0.6 + Math.random() * 0.4,
							color: new THREE.Color(0x3b82f6),
							decay: 0.02
						});
					});
					break;
				}
				case 'DreamStarted': {
					// Dramatic: pulse everything purple, slow time
					nodeMeshMap.forEach((_, id) => {
						pulseEffects.push({
							nodeId: id,
							intensity: 1.0,
							color: new THREE.Color(0xa855f7),
							decay: 0.005
						});
					});
					break;
				}
				case 'DreamProgress': {
					// Light up specific memories as they're "replayed"
					const memoryId = (event.data as { memory_id?: string })?.memory_id;
					if (memoryId && nodeMeshMap.has(memoryId)) {
						pulseEffects.push({
							nodeId: memoryId,
							intensity: 1.5,
							color: new THREE.Color(0xc084fc),
							decay: 0.01
						});
					}
					break;
				}
				case 'DreamCompleted': {
					// Celebration burst from center
					createSpawnBurst(new THREE.Vector3(0, 0, 0), new THREE.Color(0xa855f7));
					createShockwave(new THREE.Vector3(0, 0, 0), new THREE.Color(0xa855f7));
					break;
				}
				case 'ConnectionDiscovered': {
					const data = event.data as { source_id?: string; target_id?: string };
					const srcPos = data.source_id ? nodePositions.get(data.source_id) : null;
					const tgtPos = data.target_id ? nodePositions.get(data.target_id) : null;
					if (srcPos && tgtPos) {
						createConnectionFlash(srcPos, tgtPos, new THREE.Color(0xf59e0b));
					}
					break;
				}
				case 'RetentionDecayed': {
					const decayId = (event.data as { id?: string })?.id;
					if (decayId && nodeMeshMap.has(decayId)) {
						pulseEffects.push({
							nodeId: decayId,
							intensity: 0.8,
							color: new THREE.Color(0xef4444),
							decay: 0.03
						});
					}
					break;
				}
				case 'MemoryPromoted': {
					const promoId = (event.data as { id?: string })?.id;
					if (promoId && nodeMeshMap.has(promoId)) {
						pulseEffects.push({
							nodeId: promoId,
							intensity: 1.2,
							color: new THREE.Color(0x10b981),
							decay: 0.01
						});
						const promoPos = nodePositions.get(promoId);
						if (promoPos) createShockwave(promoPos, new THREE.Color(0x10b981));
					}
					break;
				}
				case 'ConsolidationCompleted': {
					// Global shimmer effect
					nodeMeshMap.forEach((_, id) => {
						pulseEffects.push({
							nodeId: id,
							intensity: 0.4 + Math.random() * 0.3,
							color: new THREE.Color(0xf59e0b),
							decay: 0.015
						});
					});
					break;
				}
			}
		}
	}

	function createSpawnBurst(position: THREE.Vector3, color: THREE.Color) {
		const count = 60;
		const geo = new THREE.BufferGeometry();
		const positions = new Float32Array(count * 3);
		const velocitiesArr = new Float32Array(count * 3);

		for (let i = 0; i < count; i++) {
			positions[i * 3] = position.x;
			positions[i * 3 + 1] = position.y;
			positions[i * 3 + 2] = position.z;
			// Random outward velocity
			const theta = Math.random() * Math.PI * 2;
			const phi = Math.acos(2 * Math.random() - 1);
			const speed = 0.3 + Math.random() * 0.5;
			velocitiesArr[i * 3] = Math.sin(phi) * Math.cos(theta) * speed;
			velocitiesArr[i * 3 + 1] = Math.sin(phi) * Math.sin(theta) * speed;
			velocitiesArr[i * 3 + 2] = Math.cos(phi) * speed;
		}

		geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
		geo.setAttribute('velocity', new THREE.BufferAttribute(velocitiesArr, 3));

		const mat = new THREE.PointsMaterial({
			color,
			size: 0.6,
			transparent: true,
			opacity: 1.0,
			blending: THREE.AdditiveBlending,
			sizeAttenuation: true,
		});

		const pts = new THREE.Points(geo, mat);
		scene.add(pts);
		spawnBursts.push({ position: position.clone(), age: 0, particles: pts });
	}

	function createShockwave(position: THREE.Vector3, color: THREE.Color) {
		const geo = new THREE.RingGeometry(0.1, 0.5, 64);
		const mat = new THREE.MeshBasicMaterial({
			color,
			transparent: true,
			opacity: 0.8,
			side: THREE.DoubleSide,
			blending: THREE.AdditiveBlending,
		});
		const ring = new THREE.Mesh(geo, mat);
		ring.position.copy(position);
		// Face camera
		ring.lookAt(camera.position);
		scene.add(ring);
		shockwaves.push({ mesh: ring, age: 0, maxAge: 60 });
	}

	function createConnectionFlash(from: THREE.Vector3, to: THREE.Vector3, color: THREE.Color) {
		const points = [from.clone(), to.clone()];
		const geo = new THREE.BufferGeometry().setFromPoints(points);
		const mat = new THREE.LineBasicMaterial({
			color,
			transparent: true,
			opacity: 1.0,
			blending: THREE.AdditiveBlending,
		});
		const line = new THREE.Line(geo, mat);
		scene.add(line);
		connectionFlashes.push({ line, intensity: 1.0 });
	}

	function updateEffects(time: number) {
		// Update pulse effects on nodes
		for (let i = pulseEffects.length - 1; i >= 0; i--) {
			const pulse = pulseEffects[i];
			pulse.intensity -= pulse.decay;
			if (pulse.intensity <= 0) {
				pulseEffects.splice(i, 1);
				continue;
			}
			const mesh = nodeMeshMap.get(pulse.nodeId);
			if (mesh) {
				const mat = mesh.material as THREE.MeshStandardMaterial;
				mat.emissive.lerp(pulse.color, pulse.intensity * 0.3);
				mat.emissiveIntensity = Math.max(mat.emissiveIntensity, pulse.intensity);
			}
		}

		// Update spawn burst particles
		for (let i = spawnBursts.length - 1; i >= 0; i--) {
			const burst = spawnBursts[i];
			burst.age++;
			if (burst.age > 120) {
				scene.remove(burst.particles);
				burst.particles.geometry.dispose();
				(burst.particles.material as THREE.Material).dispose();
				spawnBursts.splice(i, 1);
				continue;
			}
			const positions = burst.particles.geometry.attributes.position as THREE.BufferAttribute;
			const vels = burst.particles.geometry.attributes.velocity as THREE.BufferAttribute;
			for (let j = 0; j < positions.count; j++) {
				positions.setX(j, positions.getX(j) + vels.getX(j));
				positions.setY(j, positions.getY(j) + vels.getY(j));
				positions.setZ(j, positions.getZ(j) + vels.getZ(j));
				// Dampen velocity
				vels.setX(j, vels.getX(j) * 0.97);
				vels.setY(j, vels.getY(j) * 0.97);
				vels.setZ(j, vels.getZ(j) * 0.97);
			}
			positions.needsUpdate = true;
			const mat = burst.particles.material as THREE.PointsMaterial;
			mat.opacity = Math.max(0, 1 - burst.age / 120);
			mat.size = 0.6 * (1 - burst.age / 200);
		}

		// Update shockwave rings
		for (let i = shockwaves.length - 1; i >= 0; i--) {
			const sw = shockwaves[i];
			sw.age++;
			if (sw.age > sw.maxAge) {
				scene.remove(sw.mesh);
				sw.mesh.geometry.dispose();
				(sw.mesh.material as THREE.Material).dispose();
				shockwaves.splice(i, 1);
				continue;
			}
			const progress = sw.age / sw.maxAge;
			const scale = 1 + progress * 20;
			sw.mesh.scale.setScalar(scale);
			(sw.mesh.material as THREE.MeshBasicMaterial).opacity = 0.8 * (1 - progress);
			sw.mesh.lookAt(camera.position);
		}

		// Update connection flash lines
		for (let i = connectionFlashes.length - 1; i >= 0; i--) {
			const flash = connectionFlashes[i];
			flash.intensity -= 0.015;
			if (flash.intensity <= 0) {
				scene.remove(flash.line);
				flash.line.geometry.dispose();
				(flash.line.material as THREE.Material).dispose();
				connectionFlashes.splice(i, 1);
				continue;
			}
			(flash.line.material as THREE.LineBasicMaterial).opacity = flash.intensity;
		}
	}

	function onResize() {
		if (!container) return;
		const w = container.clientWidth;
		const h = container.clientHeight;
		camera.aspect = w / h;
		camera.updateProjectionMatrix();
		renderer.setSize(w, h);
		composer.setSize(w, h);
	}

	function onPointerMove(event: PointerEvent) {
		const rect = container.getBoundingClientRect();
		mouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
		mouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;

		raycaster.setFromCamera(mouse, camera);
		const meshes = Array.from(nodeMeshMap.values());
		const intersects = raycaster.intersectObjects(meshes);

		if (intersects.length > 0) {
			hoveredNode = intersects[0].object.userData.nodeId;
			container.style.cursor = 'pointer';
		} else {
			hoveredNode = null;
			container.style.cursor = 'grab';
		}
	}

	function onClick() {
		if (hoveredNode) {
			selectedNode = hoveredNode;
			onSelect?.(hoveredNode);

			// Fly camera to selected node
			const pos = nodePositions.get(hoveredNode);
			if (pos) {
				const target = pos.clone();
				controls.target.lerp(target, 0.5);
			}
		}
	}
</script>

<div bind:this={container} class="w-full h-full"></div>
