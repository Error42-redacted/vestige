<script lang="ts">
	interface Props {
		retention: number;
		stability: number;
		width?: number;
		height?: number;
	}

	let { retention, stability, width = 240, height = 80 }: Props = $props();

	// FSRS-6 retention formula: R(t) = e^(-t/S)
	// where S = stability (in days), t = time since last review
	function retentionAt(days: number): number {
		if (stability <= 0) return 0;
		return Math.exp(-days / stability);
	}

	// Generate SVG path for the decay curve
	let curvePath = $derived.by(() => {
		const points: string[] = [];
		const maxDays = Math.max(stability * 3, 30);
		const padding = 4;
		const w = width - padding * 2;
		const h = height - padding * 2;

		for (let i = 0; i <= 50; i++) {
			const t = (i / 50) * maxDays;
			const r = retentionAt(t);
			const x = padding + (i / 50) * w;
			const y = padding + (1 - r) * h;
			points.push(`${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`);
		}
		return points.join(' ');
	});

	// Key prediction points
	let predictions = $derived([
		{ label: 'Now', days: 0, value: retention },
		{ label: '1d', days: 1, value: retentionAt(1) },
		{ label: '7d', days: 7, value: retentionAt(7) },
		{ label: '30d', days: 30, value: retentionAt(30) },
	]);

	function retColor(r: number): string {
		if (r > 0.7) return '#10b981';
		if (r > 0.4) return '#f59e0b';
		return '#ef4444';
	}
</script>

<div class="space-y-2">
	<!-- SVG Curve -->
	<svg {width} {height} class="w-full" viewBox="0 0 {width} {height}">
		<!-- Grid lines -->
		<line x1="4" y1="{4 + (height - 8) * 0.5}" x2="{width - 4}" y2="{4 + (height - 8) * 0.5}" stroke="#2a2a5e" stroke-width="0.5" stroke-dasharray="2,4" />
		<line x1="4" y1="{4 + (height - 8) * 0.8}" x2="{width - 4}" y2="{4 + (height - 8) * 0.8}" stroke="#ef444430" stroke-width="0.5" stroke-dasharray="2,4" />

		<!-- Decay curve -->
		<path d={curvePath} fill="none" stroke="#6366f1" stroke-width="2" stroke-linecap="round" />

		<!-- Fill under curve -->
		<path d="{curvePath} L{width - 4},{height - 4} L4,{height - 4} Z" fill="url(#curveGrad)" opacity="0.15" />

		<!-- Current retention dot -->
		<circle cx="4" cy="{4 + (1 - retention) * (height - 8)}" r="3" fill={retColor(retention)} />

		<defs>
			<linearGradient id="curveGrad" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0%" stop-color="#6366f1" />
				<stop offset="100%" stop-color="#6366f100" />
			</linearGradient>
		</defs>
	</svg>

	<!-- Prediction pills -->
	<div class="flex gap-2 flex-wrap">
		{#each predictions as pred}
			<div class="flex items-center gap-1 text-[10px]">
				<span class="text-muted">{pred.label}:</span>
				<span style="color: {retColor(pred.value)}">{(pred.value * 100).toFixed(0)}%</span>
			</div>
		{/each}
	</div>
</div>
