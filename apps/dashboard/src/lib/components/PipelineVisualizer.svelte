<script lang="ts">
	import { onMount } from 'svelte';

	interface Props {
		resultCount?: number;
		durationMs?: number;
		active?: boolean;
	}

	let { resultCount = 0, durationMs = 0, active = false }: Props = $props();

	const stages = [
		{ name: 'Overfetch', icon: '◎', color: '#818CF8', desc: 'Pull 3x results from hybrid search' },
		{ name: 'Rerank', icon: '⟿', color: '#00A8FF', desc: 'Re-score by relevance quality' },
		{ name: 'Temporal', icon: '◷', color: '#00D4FF', desc: 'Recent memories get recency bonus' },
		{ name: 'Access', icon: '◇', color: '#00FFD1', desc: 'FSRS-6 retention threshold filter' },
		{ name: 'Context', icon: '◬', color: '#FFB800', desc: 'Encoding specificity matching' },
		{ name: 'Compete', icon: '⬡', color: '#FF3CAC', desc: 'Retrieval-induced forgetting' },
		{ name: 'Activate', icon: '◈', color: '#9D00FF', desc: 'Spreading activation cascade' },
	];

	let activeStage = $state(-1);
	let animating = $state(false);
	let showResult = $state(false);

	$effect(() => {
		if (active && !animating) {
			startAnimation();
		}
	});

	function startAnimation() {
		animating = true;
		activeStage = -1;
		showResult = false;

		// Stretch animation to 2x actual duration for visibility (min 1.5s)
		const totalDuration = Math.max(1500, (durationMs || 50) * 2);
		const stageDelay = totalDuration / (stages.length + 1);

		stages.forEach((_, i) => {
			setTimeout(() => {
				activeStage = i;
			}, stageDelay * (i + 1));
		});

		setTimeout(() => {
			showResult = true;
			animating = false;
		}, totalDuration);
	}
</script>

<div class="glass-subtle rounded-xl p-4 space-y-3">
	<div class="flex items-center justify-between">
		<span class="text-[10px] text-synapse-glow uppercase tracking-wider font-medium">Cognitive Search Pipeline</span>
		{#if showResult}
			<span class="text-[10px] text-recall">{resultCount} results in {durationMs}ms</span>
		{/if}
	</div>

	<!-- 7-stage pipeline visualization -->
	<div class="flex items-center gap-0.5">
		{#each stages as stage, i}
			{@const isActive = i <= activeStage}
			{@const isCurrent = i === activeStage && animating}

			<!-- Stage node -->
			<div class="flex flex-col items-center gap-1 flex-1 min-w-0">
				<div
					class="w-8 h-8 rounded-full flex items-center justify-center text-xs transition-all duration-300
						{isCurrent ? 'scale-125' : ''}"
					style="background: {isActive ? stage.color + '25' : 'rgba(255,255,255,0.03)'};
						border: 1.5px solid {isActive ? stage.color : 'rgba(255,255,255,0.06)'};
						color: {isActive ? stage.color : '#4a4a7a'};
						box-shadow: {isCurrent ? '0 0 12px ' + stage.color + '40' : 'none'}"
					title={stage.desc}
				>
					{stage.icon}
				</div>
				<span class="text-[8px] truncate w-full text-center transition-colors duration-300"
					style="color: {isActive ? stage.color : '#4a4a7a'}">
					{stage.name}
				</span>
			</div>

			<!-- Connecting line -->
			{#if i < stages.length - 1}
				<div class="h-px flex-shrink-0 w-2 mt-[-12px] transition-all duration-300"
					style="background: {i < activeStage ? stages[i + 1].color + '60' : 'rgba(255,255,255,0.06)'}">
				</div>
			{/if}
		{/each}
	</div>

	<!-- Energy pulse bar -->
	<div class="h-1 bg-white/[0.03] rounded-full overflow-hidden">
		{#if animating || showResult}
			<div
				class="h-full rounded-full transition-all ease-out"
				style="width: {showResult ? '100' : ((activeStage + 1) / stages.length * 100).toFixed(0)}%;
					background: linear-gradient(90deg, #818CF8, #00FFD1, #9D00FF);
					transition-duration: {animating ? '300ms' : '500ms'}"
			></div>
		{/if}
	</div>

	<!-- Result burst -->
	{#if showResult}
		<div class="flex items-center gap-2 pt-1 animate-fade-in">
			<div class="w-1.5 h-1.5 rounded-full bg-recall animate-pulse-glow"></div>
			<span class="text-[10px] text-dim">
				Pipeline complete: {resultCount} memories surfaced from {stages.length}-stage cognitive cascade
			</span>
		</div>
	{/if}
</div>

<style>
	@keyframes fade-in {
		from { opacity: 0; transform: translateY(4px); }
		to { opacity: 1; transform: translateY(0); }
	}
	.animate-fade-in {
		animation: fade-in 0.3s ease-out;
	}
</style>
