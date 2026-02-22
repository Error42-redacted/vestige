<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$stores/api';
	import type { TimelineDay } from '$types';
	import { NODE_TYPE_COLORS } from '$types';

	let timeline: TimelineDay[] = $state([]);
	let loading = $state(true);
	let days = $state(14);
	let expandedDay: string | null = $state(null);

	onMount(() => loadTimeline());

	async function loadTimeline() {
		loading = true;
		try {
			const res = await api.timeline(days, 500);
			timeline = res.timeline;
		} catch {
			timeline = [];
		} finally {
			loading = false;
		}
	}
</script>

<div class="p-6 max-w-4xl mx-auto space-y-6">
	<div class="flex items-center justify-between">
		<h1 class="text-xl text-bright font-semibold">Timeline</h1>
		<select bind:value={days} onchange={loadTimeline}
			class="px-3 py-2 bg-surface border border-subtle/40 rounded-lg text-dim text-sm">
			<option value={7}>7 days</option>
			<option value={14}>14 days</option>
			<option value={30}>30 days</option>
			<option value={90}>90 days</option>
		</select>
	</div>

	{#if loading}
		<div class="space-y-4">
			{#each Array(7) as _}
				<div class="h-16 bg-surface/50 rounded-lg animate-pulse"></div>
			{/each}
		</div>
	{:else if timeline.length === 0}
		<div class="text-center py-20 text-dim">
			<p>No memories in the selected time range.</p>
		</div>
	{:else}
		<div class="relative">
			<!-- Timeline line -->
			<div class="absolute left-6 top-0 bottom-0 w-px bg-subtle/30"></div>

			<div class="space-y-4">
				{#each timeline as day (day.date)}
					<div class="relative pl-14">
						<!-- Dot -->
						<div class="absolute left-4 top-3 w-5 h-5 rounded-full border-2 border-synapse bg-abyss flex items-center justify-center">
							<div class="w-2 h-2 rounded-full bg-synapse"></div>
						</div>

						<button onclick={() => expandedDay = expandedDay === day.date ? null : day.date}
							class="w-full text-left p-4 bg-surface/40 border border-subtle/20 rounded-lg hover:border-synapse/30 transition-all">
							<div class="flex items-center justify-between">
								<div>
									<span class="text-sm text-bright font-medium">{day.date}</span>
									<span class="text-xs text-dim ml-2">{day.count} memories</span>
								</div>
								<!-- Dots for memory types -->
								<div class="flex gap-1">
									{#each day.memories.slice(0, 10) as m}
										<div class="w-2 h-2 rounded-full" style="background: {NODE_TYPE_COLORS[m.nodeType] || '#6b7280'}; opacity: {0.3 + m.retentionStrength * 0.7}"></div>
									{/each}
									{#if day.memories.length > 10}
										<span class="text-xs text-muted">+{day.memories.length - 10}</span>
									{/if}
								</div>
							</div>

							{#if expandedDay === day.date}
								<div class="mt-3 pt-3 border-t border-subtle/20 space-y-2">
									{#each day.memories as m}
										<div class="flex items-start gap-2 text-sm">
											<div class="w-2 h-2 mt-1.5 rounded-full flex-shrink-0" style="background: {NODE_TYPE_COLORS[m.nodeType] || '#6b7280'}"></div>
											<div class="flex-1 min-w-0">
												<span class="text-dim line-clamp-1">{m.content}</span>
											</div>
											<span class="text-xs text-muted flex-shrink-0">{(m.retentionStrength * 100).toFixed(0)}%</span>
										</div>
									{/each}
								</div>
							{/if}
						</button>
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>
