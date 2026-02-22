<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$stores/api';
	import type { IntentionItem } from '$types';

	let intentions: IntentionItem[] = $state([]);
	let predictions: Record<string, unknown>[] = $state([]);
	let loading = $state(true);
	let statusFilter = $state('active');

	const STATUS_COLORS: Record<string, string> = {
		active: 'text-synapse-glow bg-synapse/10 border-synapse/30',
		fulfilled: 'text-recall bg-recall/10 border-recall/30',
		cancelled: 'text-dim bg-surface border-subtle/30',
		snoozed: 'text-dream-glow bg-dream/10 border-dream/30',
	};

	const PRIORITY_COLORS: Record<string, string> = {
		critical: 'text-decay',
		high: 'text-amber-400',
		normal: 'text-dim',
		low: 'text-muted',
	};

	const TRIGGER_ICONS: Record<string, string> = {
		time: '⏰',
		context: '◎',
		event: '⚡',
	};

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const [intRes, predRes] = await Promise.all([
				api.intentions(statusFilter),
				api.predict()
			]);
			intentions = intRes.intentions || [];
			predictions = (predRes.predictions || []) as Record<string, unknown>[];
		} catch { /* ignore */ }
		finally { loading = false; }
	}

	async function changeFilter(status: string) {
		statusFilter = status;
		await loadData();
	}

	function formatDate(d: string | undefined): string {
		if (!d) return '';
		try {
			return new Date(d).toLocaleDateString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
		} catch { return d; }
	}
</script>

<div class="p-6 max-w-5xl mx-auto space-y-8">
	<div class="flex items-center justify-between">
		<h1 class="text-xl text-bright font-semibold">Intentions & Predictions</h1>
		<span class="text-xs text-muted">{intentions.length} intentions</span>
	</div>

	<!-- Intentions Section -->
	<div class="space-y-4">
		<div class="flex items-center gap-2">
			<h2 class="text-sm text-bright font-semibold">Prospective Memory</h2>
			<span class="text-xs text-muted">"Remember to do X when Y happens"</span>
		</div>

		<!-- Status filter tabs -->
		<div class="flex gap-1.5">
			{#each ['active', 'fulfilled', 'snoozed', 'cancelled', 'all'] as status}
				<button
					onclick={() => changeFilter(status)}
					class="px-3 py-1.5 rounded-lg text-xs transition {statusFilter === status
						? 'bg-synapse/20 text-synapse-glow border border-synapse/40'
						: 'bg-surface/40 text-dim border border-subtle/20 hover:border-subtle/40'}"
				>
					{status.charAt(0).toUpperCase() + status.slice(1)}
				</button>
			{/each}
		</div>

		{#if loading}
			<div class="space-y-2">
				{#each Array(4) as _}
					<div class="h-16 bg-surface/50 rounded-lg animate-pulse"></div>
				{/each}
			</div>
		{:else if intentions.length === 0}
			<div class="text-center py-12 text-dim">
				<div class="text-4xl mb-3 opacity-20">◇</div>
				<p>No {statusFilter === 'all' ? '' : statusFilter + ' '}intentions.</p>
				<p class="text-xs text-muted mt-1">Use "Remind me..." in conversation to create intentions.</p>
			</div>
		{:else}
			<div class="space-y-2">
				{#each intentions as intention}
					<div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg">
						<div class="flex items-start gap-3">
							<!-- Trigger icon -->
							<div class="w-8 h-8 rounded-lg bg-deep flex items-center justify-center text-lg flex-shrink-0">
								{TRIGGER_ICONS[intention.trigger_type] || '◇'}
							</div>

							<div class="flex-1 min-w-0">
								<p class="text-sm text-text">{intention.content}</p>
								<div class="flex flex-wrap gap-2 mt-2">
									<!-- Status badge -->
									<span class="px-2 py-0.5 text-[10px] rounded border {STATUS_COLORS[intention.status] || 'text-dim bg-surface border-subtle/30'}">
										{intention.status}
									</span>
									<!-- Priority -->
									<span class="text-[10px] {PRIORITY_COLORS[intention.priority] || 'text-muted'}">
										{intention.priority} priority
									</span>
									<!-- Trigger -->
									<span class="text-[10px] text-muted">
										{intention.trigger_type}: {intention.trigger_value.length > 40
											? intention.trigger_value.slice(0, 37) + '...'
											: intention.trigger_value}
									</span>
									{#if intention.deadline}
										<span class="text-[10px] text-dream-glow">
											deadline: {formatDate(intention.deadline)}
										</span>
									{/if}
									{#if intention.snoozed_until}
										<span class="text-[10px] text-muted">
											snoozed until {formatDate(intention.snoozed_until)}
										</span>
									{/if}
								</div>
							</div>

							<span class="text-[10px] text-muted flex-shrink-0">{formatDate(intention.created_at)}</span>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>

	<!-- Predictions Section -->
	<div class="pt-6 border-t border-subtle/20 space-y-4">
		<div class="flex items-center gap-2">
			<h2 class="text-sm text-bright font-semibold">Predicted Needs</h2>
			<span class="text-xs text-muted">What you might need next</span>
		</div>

		{#if predictions.length === 0}
			<div class="text-center py-8 text-dim">
				<div class="text-3xl mb-3 opacity-20">◬</div>
				<p class="text-sm">No predictions yet. Use Vestige more to train the predictive model.</p>
			</div>
		{:else}
			<div class="space-y-2">
				{#each predictions as pred, i}
					<div class="p-3 bg-surface/40 border border-subtle/20 rounded-lg flex items-start gap-3">
						<div class="w-6 h-6 rounded-full bg-dream/20 text-dream-glow text-xs flex items-center justify-center flex-shrink-0 mt-0.5">
							{i + 1}
						</div>
						<div class="flex-1 min-w-0">
							<p class="text-sm text-text line-clamp-2">{pred.content}</p>
							<div class="flex gap-3 mt-1 text-xs text-muted">
								<span>{pred.nodeType}</span>
								{#if pred.retention}
									<span>{(Number(pred.retention) * 100).toFixed(0)}% retention</span>
								{/if}
								{#if pred.predictedNeed}
									<span class="text-dream-glow">{pred.predictedNeed} need</span>
								{/if}
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
