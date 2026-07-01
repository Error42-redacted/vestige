import * as THREE from 'three';
import type { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import type { OrbitControls } from 'three/addons/controls/OrbitControls.js';

export interface DreamConfig {
	bloomStrength: number;
	rotateSpeed: number;
	fogColor: number;
	fogDensity: number;
	nebulaIntensity: number;
	chromaticIntensity: number;
	vignetteRadius: number;
	breatheAmplitude: number;
}

const NORMAL_CONFIG: DreamConfig = {
	bloomStrength: 0.8,
	rotateSpeed: 0.3,
	fogColor: 0x050510,
	fogDensity: 0.008,
	nebulaIntensity: 0,
	chromaticIntensity: 0.002,
	vignetteRadius: 0.9,
	breatheAmplitude: 1.0,
};

const DREAM_CONFIG: DreamConfig = {
	bloomStrength: 1.8,
	rotateSpeed: 0.08,
	fogColor: 0x0a0520,
	fogDensity: 0.006,
	nebulaIntensity: 1.0,
	chromaticIntensity: 0.005,
	vignetteRadius: 0.7,
	breatheAmplitude: 2.0,
};

export class DreamMode {
	active = false;
	private transition = 0; // 0 = normal, 1 = dream
	private transitionSpeed = 0.008; // ~2 seconds at 60fps
	current: DreamConfig;
	private auroraHue = 0;

	constructor() {
		this.current = { ...NORMAL_CONFIG };
	}

	setActive(active: boolean) {
		this.active = active;
	}

	update(
		scene: THREE.Scene,
		bloomPass: UnrealBloomPass,
		controls: OrbitControls,
		lights: { point1: THREE.PointLight; point2: THREE.PointLight },
		_time: number
	) {
		// Smooth transition
		const target = this.active ? 1 : 0;
		this.transition += (target - this.transition) * this.transitionSpeed * 60 * (1 / 60);
		this.transition = Math.max(0, Math.min(1, this.transition));

		const t = this.transition;

		// Lerp all config values
		this.current.bloomStrength = this.lerp(NORMAL_CONFIG.bloomStrength, DREAM_CONFIG.bloomStrength, t);
		this.current.rotateSpeed = this.lerp(NORMAL_CONFIG.rotateSpeed, DREAM_CONFIG.rotateSpeed, t);
		this.current.fogDensity = this.lerp(NORMAL_CONFIG.fogDensity, DREAM_CONFIG.fogDensity, t);
		this.current.nebulaIntensity = this.lerp(NORMAL_CONFIG.nebulaIntensity, DREAM_CONFIG.nebulaIntensity, t);
		this.current.chromaticIntensity = this.lerp(NORMAL_CONFIG.chromaticIntensity, DREAM_CONFIG.chromaticIntensity, t);
		this.current.vignetteRadius = this.lerp(NORMAL_CONFIG.vignetteRadius, DREAM_CONFIG.vignetteRadius, t);
		this.current.breatheAmplitude = this.lerp(NORMAL_CONFIG.breatheAmplitude, DREAM_CONFIG.breatheAmplitude, t);

		// Apply
		bloomPass.strength = this.current.bloomStrength;
		controls.autoRotateSpeed = this.current.rotateSpeed;

		// Fog color lerp
		const normalFog = new THREE.Color(NORMAL_CONFIG.fogColor);
		const dreamFog = new THREE.Color(DREAM_CONFIG.fogColor);
		const fogColor = normalFog.clone().lerp(dreamFog, t);
		scene.fog = new THREE.FogExp2(fogColor, this.current.fogDensity);

		// Aurora color cycling during dream
		if (t > 0.01) {
			this.auroraHue = (_time * 0.1) % 1;
			const auroraColor1 = new THREE.Color().setHSL(0.75 + this.auroraHue * 0.15, 0.8, 0.5);
			const auroraColor2 = new THREE.Color().setHSL(0.55 + this.auroraHue * 0.2, 0.7, 0.4);
			lights.point1.color.lerp(auroraColor1, t * 0.3);
			lights.point2.color.lerp(auroraColor2, t * 0.3);
		} else {
			lights.point1.color.set(0x6366f1);
			lights.point2.color.set(0xa855f7);
		}
	}

	private lerp(a: number, b: number, t: number): number {
		return a + (b - a) * t;
	}
}
