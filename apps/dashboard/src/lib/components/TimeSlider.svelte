<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { GraphNode } from '$types';
	import { getDateRange } from '$lib/graph/temporal';

	interface Props {
		nodes: GraphNode[];
		onDateChange: (date: Date) => void;
		onToggle: (enabled: boolean) => void;
	}

	let { nodes, onDateChange, onToggle }: Props = $props();

	let enabled = $state(false);
	let playing = $state(false);
	let speed = $state(1); // days per second
	let sliderValue = $state(100); // 0-100 percentage
	let animFrameId: number;
	let lastTime = 0;

	let dateRange = $derived(getDateRange(nodes));
	let currentDate = $derived.by(() => {
		const oldest = dateRange.oldest.getTime();
		const newest = dateRange.newest.getTime();
		const range = newest - oldest || 1;
		return new Date(oldest + (sliderValue / 100) * range);
	});

	function formatDate(d: Date): string {
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
	}

	function toggle() {
		enabled = !enabled;
		onToggle(enabled);
		if (enabled) {
			sliderValue = 100;
			onDateChange(currentDate);
		}
	}

	function togglePlay() {
		playing = !playing;
		if (playing) {
			sliderValue = 0;
			lastTime = performance.now();
			playLoop();
		} else {
			cancelAnimationFrame(animFrameId);
		}
	}

	function playLoop() {
		if (!playing) return;
		animFrameId = requestAnimationFrame((now) => {
			const deltaSeconds = (now - lastTime) / 1000;
			lastTime = now;

			const oldest = dateRange.oldest.getTime();
			const newest = dateRange.newest.getTime();
			const totalDays = (newest - oldest) / (24 * 60 * 60 * 1000) || 1;

			// Advance by speed days per second
			const percentPerSecond = (speed / totalDays) * 100;
			sliderValue = Math.min(100, sliderValue + percentPerSecond * deltaSeconds);

			onDateChange(currentDate);

			if (sliderValue >= 100) {
				playing = false;
				return;
			}
			playLoop();
		});
	}

	function onSliderInput() {
		onDateChange(currentDate);
	}

	onDestroy(() => {
		playing = false;
		cancelAnimationFrame(animFrameId);
	});
</script>

{#if enabled}
	<div class="absolute bottom-4 left-1/2 -translate-x-1/2 z-10 w-[90%] max-w-xl">
		<div class="glass-panel rounded-xl p-3 space-y-2">
			<div class="flex items-center justify-between">
				<div class="flex items-center gap-2">
					<button
						onclick={togglePlay}
						class="w-7 h-7 rounded-lg bg-synapse/20 border border-synapse/30 text-synapse-glow text-xs flex items-center justify-center hover:bg-synapse/30 transition"
					>
						{playing ? '⏸' : '▶'}
					</button>

					<select
						bind:value={speed}
						class="px-2 py-1 bg-white/[0.03] border border-synapse/10 rounded-lg text-[10px] text-dim focus:outline-none"
					>
						<option value={1}>1x</option>
						<option value={7}>7x</option>
						<option value={30}>30x</option>
					</select>
				</div>

				<span class="text-xs text-bright font-medium">{formatDate(currentDate)}</span>

				<button
					onclick={toggle}
					class="text-[10px] text-muted hover:text-text transition"
				>
					Close
				</button>
			</div>

			<input
				type="range"
				min="0"
				max="100"
				step="0.1"
				bind:value={sliderValue}
				oninput={onSliderInput}
				class="w-full h-1.5 appearance-none bg-white/[0.06] rounded-full cursor-pointer
					[&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3
					[&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-synapse-glow
					[&::-webkit-slider-thumb]:shadow-[0_0_8px_rgba(129,140,248,0.4)]"
			/>

			<div class="flex justify-between text-[9px] text-muted">
				<span>{formatDate(dateRange.oldest)}</span>
				<span>{formatDate(dateRange.newest)}</span>
			</div>
		</div>
	</div>
{:else}
	<button
		onclick={toggle}
		class="absolute bottom-4 right-4 z-10 px-3 py-2 glass rounded-xl text-dim text-xs hover:text-text transition flex items-center gap-1.5"
	>
		<span>◷</span>
		<span>Timeline</span>
	</button>
{/if}
