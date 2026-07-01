import * as THREE from 'three';
import type { GraphEdge } from '$types';

function easeOutCubic(t: number): number {
	return 1 - Math.pow(1 - t, 3);
}

interface GrowingEdge {
	line: THREE.Line;
	source: string;
	target: string;
	frame: number;
	totalFrames: number;
}

interface DissolvingEdge {
	line: THREE.Line;
	frame: number;
	totalFrames: number;
}

export class EdgeManager {
	group: THREE.Group;
	private growingEdges: GrowingEdge[] = [];
	private dissolvingEdges: DissolvingEdge[] = [];

	constructor() {
		this.group = new THREE.Group();
	}

	createEdges(edges: GraphEdge[], positions: Map<string, THREE.Vector3>) {
		for (const edge of edges) {
			const sourcePos = positions.get(edge.source);
			const targetPos = positions.get(edge.target);
			if (!sourcePos || !targetPos) continue;

			const points = [sourcePos, targetPos];
			const geometry = new THREE.BufferGeometry().setFromPoints(points);
			// Brand violet (#8b5cf6) instead of the old dark navy 0x4a4a7a
			// which was invisible against the fog. Higher opacity base so
			// edges actually read as a graph.
			const material = new THREE.LineBasicMaterial({
				color: 0x8b5cf6,
				transparent: true,
				opacity: Math.min(0.25 + edge.weight * 0.5, 0.8),
				blending: THREE.AdditiveBlending,
				depthWrite: false,
			});

			const line = new THREE.Line(geometry, material);
			line.userData = { source: edge.source, target: edge.target };
			this.group.add(line);
		}
	}

	addEdge(edge: GraphEdge, positions: Map<string, THREE.Vector3>) {
		const sourcePos = positions.get(edge.source);
		const targetPos = positions.get(edge.target);
		if (!sourcePos || !targetPos) return;

		// Start with zero-length line at source position
		const points = [sourcePos.clone(), sourcePos.clone()];
		const geometry = new THREE.BufferGeometry().setFromPoints(points);
		const material = new THREE.LineBasicMaterial({
			color: 0x8b5cf6,
			transparent: true,
			opacity: 0,
			blending: THREE.AdditiveBlending,
			depthWrite: false,
		});

		const line = new THREE.Line(geometry, material);
		line.userData = { source: edge.source, target: edge.target };
		this.group.add(line);

		this.growingEdges.push({
			line,
			source: edge.source,
			target: edge.target,
			frame: 0,
			totalFrames: 45,
		});
	}

	removeEdgesForNode(nodeId: string) {
		const toDissolve: THREE.Line[] = [];
		this.group.children.forEach((child) => {
			const line = child as THREE.Line;
			if (line.userData.source === nodeId || line.userData.target === nodeId) {
				toDissolve.push(line);
			}
		});

		for (const line of toDissolve) {
			// Remove from growing if still animating
			this.growingEdges = this.growingEdges.filter((g) => g.line !== line);
			this.dissolvingEdges.push({ line, frame: 0, totalFrames: 40 });
		}
	}

	animateEdges(positions: Map<string, THREE.Vector3>) {
		// Growing edges — interpolate endpoint from source to target
		for (let i = this.growingEdges.length - 1; i >= 0; i--) {
			const g = this.growingEdges[i];
			g.frame++;
			const progress = easeOutCubic(Math.min(g.frame / g.totalFrames, 1));

			const sourcePos = positions.get(g.source);
			const targetPos = positions.get(g.target);
			if (!sourcePos || !targetPos) continue;

			const currentEnd = sourcePos.clone().lerp(targetPos, progress);
			const attrs = g.line.geometry.attributes.position as THREE.BufferAttribute;
			attrs.setXYZ(0, sourcePos.x, sourcePos.y, sourcePos.z);
			attrs.setXYZ(1, currentEnd.x, currentEnd.y, currentEnd.z);
			attrs.needsUpdate = true;

			const mat = g.line.material as THREE.LineBasicMaterial;
			mat.opacity = progress * 0.65;

			if (g.frame >= g.totalFrames) {
				// Final opacity matches new createEdges baseline
				mat.opacity = 0.65;
				this.growingEdges.splice(i, 1);
			}
		}

		// Dissolving edges — fade out
		for (let i = this.dissolvingEdges.length - 1; i >= 0; i--) {
			const d = this.dissolvingEdges[i];
			d.frame++;
			const progress = d.frame / d.totalFrames;
			const mat = d.line.material as THREE.LineBasicMaterial;
			mat.opacity = Math.max(0, 0.65 * (1 - progress));

			if (d.frame >= d.totalFrames) {
				this.group.remove(d.line);
				d.line.geometry.dispose();
				(d.line.material as THREE.Material).dispose();
				this.dissolvingEdges.splice(i, 1);
			}
		}
	}

	updatePositions(positions: Map<string, THREE.Vector3>) {
		this.group.children.forEach((child) => {
			const line = child as THREE.Line;
			// Skip lines currently being animated by animateEdges
			if (this.growingEdges.some((g) => g.line === line)) return;
			if (this.dissolvingEdges.some((d) => d.line === line)) return;

			const sourcePos = positions.get(line.userData.source);
			const targetPos = positions.get(line.userData.target);
			if (sourcePos && targetPos) {
				const attrs = line.geometry.attributes.position as THREE.BufferAttribute;
				attrs.setXYZ(0, sourcePos.x, sourcePos.y, sourcePos.z);
				attrs.setXYZ(1, targetPos.x, targetPos.y, targetPos.z);
				attrs.needsUpdate = true;
			}
		});
	}

	dispose() {
		this.group.children.forEach((child) => {
			const line = child as THREE.Line;
			line.geometry?.dispose();
			(line.material as THREE.Material)?.dispose();
		});
		this.growingEdges = [];
		this.dissolvingEdges = [];
	}
}
