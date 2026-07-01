import * as THREE from 'three';

// Domain-warped FBM noise nebula background shader
const vertexShader = /* glsl */ `
varying vec2 vUv;
void main() {
	vUv = uv;
	gl_Position = vec4(position, 1.0);
}
`;

const fragmentShader = /* glsl */ `
precision highp float;

uniform float uTime;
uniform vec2 uResolution;
uniform float uDreamIntensity;

varying vec2 vUv;

// Simplex-style hash
vec3 hash33(vec3 p3) {
	p3 = fract(p3 * vec3(0.1031, 0.1030, 0.0973));
	p3 += dot(p3, p3.yxz + 33.33);
	return fract((p3.xxy + p3.yxx) * p3.zyx);
}

// 3D value noise
float noise(vec3 p) {
	vec3 i = floor(p);
	vec3 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);

	float n = i.x + i.y * 157.0 + 113.0 * i.z;

	vec4 v1 = fract(sin(vec4(n + 0.0, n + 1.0, n + 157.0, n + 158.0)) * 43758.5453);
	vec4 v2 = fract(sin(vec4(n + 113.0, n + 114.0, n + 270.0, n + 271.0)) * 43758.5453);

	vec4 a = mix(v1, v2, f.z);
	vec2 b = mix(a.xy, a.zw, f.y);
	return mix(b.x, b.y, f.x);
}

// FBM with 5 octaves
float fbm(vec3 p) {
	float value = 0.0;
	float amplitude = 0.5;
	float frequency = 1.0;
	for (int i = 0; i < 5; i++) {
		value += amplitude * noise(p * frequency);
		frequency *= 2.0;
		amplitude *= 0.5;
	}
	return value;
}

// IQ cosine palette
vec3 palette(float t, vec3 a, vec3 b, vec3 c, vec3 d) {
	return a + b * cos(6.28318 * (c * t + d));
}

void main() {
	vec2 uv = (gl_FragCoord.xy - 0.5 * uResolution.xy) / min(uResolution.x, uResolution.y);
	float t = uTime * 0.05;

	// Domain warping: fbm(p + fbm(p + fbm(p)))
	vec3 p = vec3(uv * 2.0, t);

	float warp1 = fbm(p);
	float warp2 = fbm(p + warp1 * 3.0 + vec3(1.7, 9.2, t * 0.3));
	float warp3 = fbm(p + warp2 * 2.5 + vec3(8.3, 2.8, t * 0.2));

	// Final noise value
	float f = fbm(p + warp3 * 2.0);

	// Color: cosmic palette that shifts during dream mode
	vec3 normalA = vec3(0.02, 0.01, 0.05);
	vec3 normalB = vec3(0.03, 0.02, 0.08);
	vec3 normalC = vec3(1.0, 1.0, 1.0);
	vec3 normalD = vec3(0.70, 0.55, 0.80);

	vec3 dreamA = vec3(0.05, 0.01, 0.08);
	vec3 dreamB = vec3(0.06, 0.03, 0.12);
	vec3 dreamC = vec3(1.0, 0.8, 1.0);
	vec3 dreamD = vec3(0.80, 0.40, 0.90);

	vec3 a = mix(normalA, dreamA, uDreamIntensity);
	vec3 b = mix(normalB, dreamB, uDreamIntensity);
	vec3 c = mix(normalC, dreamC, uDreamIntensity);
	vec3 d = mix(normalD, dreamD, uDreamIntensity);

	vec3 color = palette(f + warp2 * 0.5, a, b, c, d);

	// Add subtle star-like highlights
	float stars = smoothstep(0.97, 1.0, noise(vec3(uv * 50.0, t * 0.1)));
	color += stars * 0.15;

	// Intensity modulation
	float intensity = 0.15 + 0.1 * uDreamIntensity;
	color *= intensity;

	// Vignette
	float dist = length(uv);
	color *= smoothstep(1.5, 0.3, dist);

	gl_FragColor = vec4(color, 1.0);
}
`;

export function createNebulaBackground(scene: THREE.Scene): {
	mesh: THREE.Mesh;
	material: THREE.ShaderMaterial;
} {
	const geometry = new THREE.PlaneGeometry(2, 2);
	const material = new THREE.ShaderMaterial({
		vertexShader,
		fragmentShader,
		uniforms: {
			uTime: { value: 0 },
			uResolution: { value: new THREE.Vector2(window.innerWidth, window.innerHeight) },
			uDreamIntensity: { value: 0 },
		},
		depthWrite: false,
		depthTest: false,
		transparent: false,
	});

	const mesh = new THREE.Mesh(geometry, material);
	mesh.frustumCulled = false;
	mesh.renderOrder = -1000;
	scene.add(mesh);

	return { mesh, material };
}

export function updateNebula(
	material: THREE.ShaderMaterial,
	time: number,
	dreamIntensity: number,
	width: number,
	height: number
) {
	material.uniforms.uTime.value = time;
	material.uniforms.uDreamIntensity.value = dreamIntensity;
	material.uniforms.uResolution.value.set(width, height);
}
