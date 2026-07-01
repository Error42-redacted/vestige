import * as THREE from 'three';

export class ParticleSystem {
	starField: THREE.Points;
	neuralParticles: THREE.Points;

	constructor(scene: THREE.Scene) {
		this.starField = this.createStarField();
		this.neuralParticles = this.createNeuralParticles();
		scene.add(this.starField);
		scene.add(this.neuralParticles);
	}

	private createStarField(): THREE.Points {
		const count = 3000;
		const geometry = new THREE.BufferGeometry();
		const positions = new Float32Array(count * 3);
		const sizes = new Float32Array(count);

		for (let i = 0; i < count; i++) {
			positions[i * 3] = (Math.random() - 0.5) * 1000;
			positions[i * 3 + 1] = (Math.random() - 0.5) * 1000;
			positions[i * 3 + 2] = (Math.random() - 0.5) * 1000;
			sizes[i] = Math.random() * 1.5;
		}

		geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
		geometry.setAttribute('size', new THREE.BufferAttribute(sizes, 1));

		const material = new THREE.PointsMaterial({
			color: 0x6366f1,
			size: 0.5,
			transparent: true,
			opacity: 0.4,
			sizeAttenuation: true,
			blending: THREE.AdditiveBlending,
		});

		return new THREE.Points(geometry, material);
	}

	private createNeuralParticles(): THREE.Points {
		const count = 500;
		const geometry = new THREE.BufferGeometry();
		const positions = new Float32Array(count * 3);
		const colors = new Float32Array(count * 3);

		for (let i = 0; i < count; i++) {
			positions[i * 3] = (Math.random() - 0.5) * 100;
			positions[i * 3 + 1] = (Math.random() - 0.5) * 100;
			positions[i * 3 + 2] = (Math.random() - 0.5) * 100;
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

		return new THREE.Points(geometry, material);
	}

	animate(time: number) {
		// Star rotation
		this.starField.rotation.y += 0.0001;
		this.starField.rotation.x += 0.00005;

		// Neural particle motion
		const positions = this.neuralParticles.geometry.attributes.position as THREE.BufferAttribute;
		for (let i = 0; i < positions.count; i++) {
			positions.setY(i, positions.getY(i) + Math.sin(time + i * 0.1) * 0.02);
			positions.setX(i, positions.getX(i) + Math.cos(time + i * 0.05) * 0.01);
		}
		positions.needsUpdate = true;
	}

	dispose() {
		this.starField.geometry.dispose();
		(this.starField.material as THREE.Material).dispose();
		this.neuralParticles.geometry.dispose();
		(this.neuralParticles.material as THREE.Material).dispose();
	}
}
