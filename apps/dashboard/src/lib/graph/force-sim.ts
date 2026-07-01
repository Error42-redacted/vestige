import * as THREE from 'three';
import type { GraphEdge } from '$types';

export class ForceSimulation {
	positions: Map<string, THREE.Vector3>;
	velocities: Map<string, THREE.Vector3>;
	running = true;
	step = 0;

	private readonly repulsionStrength = 500;
	private readonly attractionStrength = 0.01;
	private readonly dampening = 0.9;
	private readonly baseMaxSteps = 300;
	private maxSteps = 300;
	private cooldownExtension = 0;

	constructor(positions: Map<string, THREE.Vector3>) {
		this.positions = positions;
		this.velocities = new Map();
		for (const id of positions.keys()) {
			this.velocities.set(id, new THREE.Vector3());
		}
	}

	addNode(id: string, position: THREE.Vector3) {
		this.positions.set(id, position.clone());
		this.velocities.set(id, new THREE.Vector3());
		// Re-energize: rewind step count to keep physics alive for 100 more frames
		this.cooldownExtension = 100;
		this.maxSteps = Math.max(this.maxSteps, this.step + this.cooldownExtension);
		this.running = true;
	}

	removeNode(id: string) {
		this.positions.delete(id);
		this.velocities.delete(id);
	}

	tick(edges: GraphEdge[]) {
		if (!this.running) return;
		if (this.step > this.maxSteps) {
			// Decay cooldown extension, settle back to base
			if (this.cooldownExtension > 0) {
				this.cooldownExtension = 0;
				this.maxSteps = this.baseMaxSteps;
			}
			return;
		}
		this.step++;

		const alpha = Math.max(0.001, 1 - this.step / this.maxSteps);
		const nodeIds = Array.from(this.positions.keys());

		// Repulsion between all nodes
		for (let i = 0; i < nodeIds.length; i++) {
			for (let j = i + 1; j < nodeIds.length; j++) {
				const posA = this.positions.get(nodeIds[i])!;
				const posB = this.positions.get(nodeIds[j])!;
				const diff = new THREE.Vector3().subVectors(posA, posB);
				const dist = diff.length() || 1;
				const force = (this.repulsionStrength / (dist * dist)) * alpha;
				const dir = diff.normalize().multiplyScalar(force);

				this.velocities.get(nodeIds[i])!.add(dir);
				this.velocities.get(nodeIds[j])!.sub(dir);
			}
		}

		// Attraction along edges
		for (const edge of edges) {
			const posA = this.positions.get(edge.source);
			const posB = this.positions.get(edge.target);
			if (!posA || !posB) continue;

			const diff = new THREE.Vector3().subVectors(posB, posA);
			const dist = diff.length();
			const force = dist * this.attractionStrength * edge.weight * alpha;
			const dir = diff.normalize().multiplyScalar(force);

			this.velocities.get(edge.source)!.add(dir);
			this.velocities.get(edge.target)!.sub(dir);
		}

		// Centering force + velocity integration
		for (const id of nodeIds) {
			const pos = this.positions.get(id)!;
			const vel = this.velocities.get(id)!;
			vel.sub(pos.clone().multiplyScalar(0.001 * alpha));
			vel.multiplyScalar(this.dampening);
			pos.add(vel);
		}
	}

	reset() {
		this.step = 0;
		this.running = true;
		for (const vel of this.velocities.values()) {
			vel.set(0, 0, 0);
		}
	}
}
