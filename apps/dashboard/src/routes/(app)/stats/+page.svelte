<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$stores/api';
	import type { SystemStats, HealthCheck, RetentionDistribution } from '$types';

	let stats: SystemStats | null = $state(null);
	let health: HealthCheck | null = $state(null);
	let retention: RetentionDistribution | null = $state(null);
	let loading = $state(true);

	onMount(async () => {
		try {
			[stats, health, retention] = await Promise.all([
				api.stats(),
				api.health(),
				api.retentionDistribution()
			]);
		} catch {
			// API not available
		} finally {
			loading = false;
		}
	});

	function statusColor(status: string): string {
		return { healthy: '#10b981', degraded: '#f59e0b', critical: '#ef4444', empty: '#6b7280' }[status] || '#6b7280';
	}

	async function runConsolidation() {
		try {
			await api.consolidate();
			[stats, health, retention] = await Promise.all([api.stats(), api.health(), api.retentionDistribution()]);
		} catch {
			// API not available
		}
	}
</script>

<div class="p-6 max-w-5xl mx-auto space-y-6">
	<h1 class="text-xl text-bright font-semibold">System Stats</h1>

	{#if loading}
		<div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
			{#each Array(8) as _}
				<div class="h-24 bg-surface/50 rounded-lg animate-pulse"></div>
			{/each}
		</div>
	{:else if stats && health}
		<!-- Status banner -->
		<div class="flex items-center gap-3 p-4 rounded-lg border" style="border-color: {statusColor(health.status)}40; background: {statusColor(health.status)}10">
			<div class="w-3 h-3 rounded-full animate-pulse-glow" style="background: {statusColor(health.status)}"></div>
			<span class="text-sm font-medium" style="color: {statusColor(health.status)}">{health.status.toUpperCase()}</span>
			<span class="text-xs text-dim">v{health.version}</span>
		</div>

		<!-- Key metrics -->
		<div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
			<div class="p-4 bg-surface/50 border border-subtle/20 rounded-lg">
				<div class="text-2xl text-bright font-bold">{stats.totalMemories}</div>
				<div class="text-xs text-dim mt-1">Total Memories</div>
			</div>
			<div class="p-4 bg-surface/50 border border-subtle/20 rounded-lg">
				<div class="text-2xl font-bold" style="color: {stats.averageRetention > 0.7 ? '#10b981' : stats.averageRetention > 0.4 ? '#f59e0b' : '#ef4444'}">{(stats.averageRetention * 100).toFixed(1)}%</div>
				<div class="text-xs text-dim mt-1">Avg Retention</div>
			</div>
			<div class="p-4 bg-surface/50 border border-subtle/20 rounded-lg">
				<div class="text-2xl text-bright font-bold">{stats.dueForReview}</div>
				<div class="text-xs text-dim mt-1">Due for Review</div>
			</div>
			<div class="p-4 bg-surface/50 border border-subtle/20 rounded-lg">
				<div class="text-2xl text-bright font-bold">{stats.embeddingCoverage.toFixed(0)}%</div>
				<div class="text-xs text-dim mt-1">Embedding Coverage</div>
			</div>
		</div>

		<!-- Retention Distribution -->
		{#if retention}
			<div class="p-6 bg-surface/30 border border-subtle/20 rounded-lg">
				<h2 class="text-sm text-bright font-semibold mb-4">Retention Distribution</h2>
				<div class="flex items-end gap-1 h-40">
					{#each retention.distribution as bucket, i}
						{@const maxCount = Math.max(...retention.distribution.map(b => b.count), 1)}
						{@const height = (bucket.count / maxCount) * 100}
						{@const color = i < 3 ? '#ef4444' : i < 5 ? '#f59e0b' : i < 7 ? '#10b981' : '#6366f1'}
						<div class="flex-1 flex flex-col items-center gap-1">
							<span class="text-xs text-dim">{bucket.count}</span>
							<div class="w-full rounded-t transition-all duration-500" style="height: {height}%; background: {color}; opacity: 0.7; min-height: 2px"></div>
							<span class="text-xs text-muted">{bucket.range}</span>
						</div>
					{/each}
				</div>
			</div>

			<!-- Type breakdown -->
			<div class="p-6 bg-surface/30 border border-subtle/20 rounded-lg">
				<h2 class="text-sm text-bright font-semibold mb-4">Memory Types</h2>
				<div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
					{#each Object.entries(retention.byType) as [type, count]}
						<div class="flex items-center gap-2 text-sm">
							<div class="w-3 h-3 rounded-full" style="background: {({'fact':'#3b82f6','concept':'#8b5cf6','event':'#f59e0b','person':'#10b981','note':'#6b7280','pattern':'#ec4899','decision':'#ef4444'})[type] || '#6b7280'}"></div>
							<span class="text-dim">{type}</span>
							<span class="text-muted ml-auto">{count}</span>
						</div>
					{/each}
				</div>
			</div>

			<!-- Endangered memories -->
			{#if retention.endangered.length > 0}
				<div class="p-6 bg-decay/5 border border-decay/20 rounded-lg">
					<h2 class="text-sm text-decay font-semibold mb-3">Endangered Memories ({retention.endangered.length})</h2>
					<div class="space-y-2 max-h-48 overflow-y-auto">
						{#each retention.endangered.slice(0, 20) as m}
							<div class="flex items-center gap-3 text-sm">
								<span class="text-xs text-decay">{(m.retentionStrength * 100).toFixed(0)}%</span>
								<span class="text-dim truncate">{m.content}</span>
							</div>
						{/each}
					</div>
				</div>
			{/if}
		{/if}

		<!-- Actions -->
		<div class="flex gap-3">
			<button onclick={runConsolidation}
				class="px-4 py-2 bg-warning/20 border border-warning/40 text-warning text-sm rounded-lg hover:bg-warning/30 transition">
				Run Consolidation
			</button>
		</div>
	{/if}
</div>
