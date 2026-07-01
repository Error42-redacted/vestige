import * as THREE from 'three';
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js';
import type { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';

// Chromatic Aberration
const ChromaticAberrationShader = {
	uniforms: {
		tDiffuse: { value: null },
		uIntensity: { value: 0.002 },
	},
	vertexShader: /* glsl */ `
		varying vec2 vUv;
		void main() {
			vUv = uv;
			gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
		}
	`,
	fragmentShader: /* glsl */ `
		uniform sampler2D tDiffuse;
		uniform float uIntensity;
		varying vec2 vUv;

		void main() {
			vec2 center = vec2(0.5);
			vec2 dir = vUv - center;
			float dist = length(dir);

			float rOffset = uIntensity * dist;
			float gOffset = 0.0;
			float bOffset = -uIntensity * dist;

			vec2 rUv = vUv + dir * rOffset;
			vec2 gUv = vUv + dir * gOffset;
			vec2 bUv = vUv + dir * bOffset;

			float r = texture2D(tDiffuse, rUv).r;
			float g = texture2D(tDiffuse, gUv).g;
			float b = texture2D(tDiffuse, bUv).b;

			gl_FragColor = vec4(r, g, b, 1.0);
		}
	`,
};

// Film Grain
const FilmGrainShader = {
	uniforms: {
		tDiffuse: { value: null },
		uTime: { value: 0 },
		uIntensity: { value: 0.04 },
	},
	vertexShader: /* glsl */ `
		varying vec2 vUv;
		void main() {
			vUv = uv;
			gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
		}
	`,
	fragmentShader: /* glsl */ `
		uniform sampler2D tDiffuse;
		uniform float uTime;
		uniform float uIntensity;
		varying vec2 vUv;

		float rand(vec2 co) {
			return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453);
		}

		void main() {
			vec4 color = texture2D(tDiffuse, vUv);
			float grain = rand(vUv + vec2(uTime)) * 2.0 - 1.0;
			color.rgb += grain * uIntensity;
			gl_FragColor = color;
		}
	`,
};

// Vignette
const VignetteShader = {
	uniforms: {
		tDiffuse: { value: null },
		uRadius: { value: 0.9 },
		uSoftness: { value: 0.5 },
	},
	vertexShader: /* glsl */ `
		varying vec2 vUv;
		void main() {
			vUv = uv;
			gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
		}
	`,
	fragmentShader: /* glsl */ `
		uniform sampler2D tDiffuse;
		uniform float uRadius;
		uniform float uSoftness;
		varying vec2 vUv;

		void main() {
			vec4 color = texture2D(tDiffuse, vUv);
			vec2 center = vec2(0.5);
			float dist = distance(vUv, center) * 1.414;
			float vignette = smoothstep(uRadius, uRadius - uSoftness, dist);
			color.rgb *= vignette;
			gl_FragColor = color;
		}
	`,
};

export interface PostProcessingStack {
	chromatic: ShaderPass;
	grain: ShaderPass;
	vignette: ShaderPass;
}

export function createPostProcessing(composer: EffectComposer): PostProcessingStack {
	const chromatic = new ShaderPass(ChromaticAberrationShader);
	const grain = new ShaderPass(FilmGrainShader);
	const vignette = new ShaderPass(VignetteShader);

	composer.addPass(chromatic);
	composer.addPass(grain);
	composer.addPass(vignette);

	return { chromatic, grain, vignette };
}

export function updatePostProcessing(
	stack: PostProcessingStack,
	time: number,
	dreamIntensity: number
) {
	// Chromatic aberration: doubles during dream
	const chromaticBase = 0.002;
	const chromaticDream = 0.005;
	stack.chromatic.uniforms.uIntensity.value =
		chromaticBase + (chromaticDream - chromaticBase) * dreamIntensity;

	// Film grain: animated
	stack.grain.uniforms.uTime.value = time;
	stack.grain.uniforms.uIntensity.value = 0.04 + dreamIntensity * 0.02;

	// Vignette: tighter during dream
	const vignetteBase = 0.9;
	const vignetteDream = 0.7;
	stack.vignette.uniforms.uRadius.value =
		vignetteBase + (vignetteDream - vignetteBase) * dreamIntensity;
}
