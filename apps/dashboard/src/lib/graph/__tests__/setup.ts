/**
 * Test setup: minimal DOM stubs for canvas-based text rendering.
 */
import { vi } from 'vitest';

// Minimal canvas gradient mock — collects colour stops so tests can inspect
// them if they want to, but is mostly a no-op for runtime.
function createMockGradient() {
	return {
		colorStops: [] as Array<{ offset: number; color: string }>,
		addColorStop(offset: number, color: string) {
			this.colorStops.push({ offset, color });
		},
	};
}

// Minimal canvas 2D context mock
const mockContext2D = {
	clearRect: vi.fn(),
	fillRect: vi.fn(),
	fillText: vi.fn(),
	strokeText: vi.fn(),
	measureText: vi.fn(() => ({ width: 100 })),
	createRadialGradient: vi.fn(() => createMockGradient()),
	createLinearGradient: vi.fn(() => createMockGradient()),
	beginPath: vi.fn(),
	closePath: vi.fn(),
	moveTo: vi.fn(),
	lineTo: vi.fn(),
	quadraticCurveTo: vi.fn(),
	arc: vi.fn(),
	fill: vi.fn(),
	stroke: vi.fn(),
	font: '',
	textAlign: '',
	textBaseline: '',
	fillStyle: '' as string | object,
	strokeStyle: '' as string | object,
	lineWidth: 1,
	shadowColor: '',
	shadowBlur: 0,
	shadowOffsetX: 0,
	shadowOffsetY: 0,
};

// Minimal canvas element mock
const mockCanvas = {
	width: 512,
	height: 64,
	getContext: vi.fn(() => mockContext2D),
	toDataURL: vi.fn(() => 'data:image/png;base64,'),
};

// Stub document.createElement for canvas
if (typeof globalThis.document === 'undefined') {
	(globalThis as any).document = {
		createElement: vi.fn((tag: string) => {
			if (tag === 'canvas') return { ...mockCanvas };
			return {};
		}),
	};
}
