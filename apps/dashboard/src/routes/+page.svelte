<script lang="ts">
	import { onMount } from 'svelte';
	import Graph3D from '$components/Graph3D.svelte';
	import { api } from '$stores/api';
	import { eventFeed } from '$stores/websocket';
	import type { GraphResponse, Memory } from '$types';

	let graphData: GraphResponse | null = $state(null);
	let selectedMemory: Memory | null = $state(null);
	let loading = $state(true);
	let error = $state('');
	let isDreaming = $state(false);

	onMount(async () => {
		try {
			graphData = await api.graph({ max_nodes: 150, depth: 3 });
		} catch (e) {
			error = 'No memories yet. Start using Vestige to see your memory graph.';
		} finally {
			loading = false;
		}
	});

	async function triggerDream() {
		isDreaming = true;
		try {
			const result = await api.dream();
			// Reload graph with new connections
			graphData = await api.graph({ max_nodes: 150, depth: 3 });
		} catch {
			// Dream failed silently
		} finally {
			isDreaming = false;
		}
	}

	async function onNodeSelect(nodeId: string) {
		try {
			selectedMemory = await api.memories.get(nodeId);
		} catch {
			selectedMemory = null;
		}
	}
</script>

<div class="h-full relative">
	<!-- 3D Graph fills the viewport -->
	{#if loading}
		<div class="h-full flex items-center justify-center">
			<div class="text-center space-y-4">
				<div class="w-16 h-16 mx-auto rounded-full border-2 border-synapse/30 border-t-synapse animate-spin"></div>
				<p class="text-dim text-sm">Loading memory graph...</p>
			</div>
		</div>
	{:else if error}
		<div class="h-full flex items-center justify-center">
			<div class="text-center space-y-4 max-w-md px-8">
				<div class="text-4xl">◎</div>
				<h2 class="text-xl text-bright">Your Mind Awaits</h2>
				<p class="text-dim text-sm">{error}</p>
			</div>
		</div>
	{:else if graphData}
		<Graph3D
			nodes={graphData.nodes}
			edges={graphData.edges}
			centerId={graphData.center_id}
			events={$eventFeed}
			{isDreaming}
			onSelect={onNodeSelect}
		/>
	{/if}

	<!-- Floating controls -->
	<div class="absolute top-4 left-4 flex gap-2 z-10">
		<button
			onclick={triggerDream}
			disabled={isDreaming}
			class="px-4 py-2 rounded-lg bg-dream/20 border border-dream/40 text-dream-glow text-sm
				hover:bg-dream/30 transition-all disabled:opacity-50 backdrop-blur-sm
				{isDreaming ? 'glow-dream animate-pulse-glow' : ''}"
		>
			{isDreaming ? '◎ Dreaming...' : '◎ Dream'}
		</button>
	</div>

	<!-- Floating stats -->
	<div class="absolute top-4 right-4 z-10 text-xs text-dim backdrop-blur-sm bg-abyss/60 rounded-lg px-3 py-2 border border-subtle/20">
		{#if graphData}
			<div>{graphData.nodeCount} nodes / {graphData.edgeCount} edges</div>
		{/if}
	</div>

	<!-- Selected memory detail panel -->
	{#if selectedMemory}
		<div class="absolute right-0 top-0 h-full w-96 bg-abyss/95 backdrop-blur-xl border-l border-subtle/30 p-6 overflow-y-auto z-20">
			<div class="flex justify-between items-start mb-4">
				<h3 class="text-bright text-sm font-semibold">Memory Detail</h3>
				<button onclick={() => selectedMemory = null} class="text-dim hover:text-text text-lg">×</button>
			</div>

			<div class="space-y-4">
				<!-- Type badge -->
				<div class="flex gap-2">
					<span class="px-2 py-0.5 rounded text-xs bg-synapse/20 text-synapse-glow">{selectedMemory.nodeType}</span>
					{#each selectedMemory.tags as tag}
						<span class="px-2 py-0.5 rounded text-xs bg-surface text-dim">{tag}</span>
					{/each}
				</div>

				<!-- Content -->
				<div class="text-sm text-text leading-relaxed whitespace-pre-wrap">{selectedMemory.content}</div>

				<!-- Retention bar -->
				<div>
					<div class="flex justify-between text-xs text-dim mb-1">
						<span>Retention</span>
						<span>{(selectedMemory.retentionStrength * 100).toFixed(1)}%</span>
					</div>
					<div class="h-2 bg-surface rounded-full overflow-hidden">
						<div
							class="h-full rounded-full transition-all duration-500"
							style="width: {selectedMemory.retentionStrength * 100}%; background: {
								selectedMemory.retentionStrength > 0.7 ? '#10b981' :
								selectedMemory.retentionStrength > 0.4 ? '#f59e0b' : '#ef4444'
							}"
						></div>
					</div>
				</div>

				<!-- Metadata -->
				<div class="text-xs text-dim space-y-1">
					<div>Created: {new Date(selectedMemory.createdAt).toLocaleDateString()}</div>
					<div>Reviews: {selectedMemory.reviewCount ?? 0}</div>
					{#if selectedMemory.source}
						<div>Source: {selectedMemory.source}</div>
					{/if}
				</div>

				<!-- Actions -->
				<div class="flex gap-2">
					<button
						onclick={() => selectedMemory && api.memories.promote(selectedMemory.id)}
						class="flex-1 px-3 py-2 rounded bg-recall/20 text-recall text-xs hover:bg-recall/30 transition"
					>
						Promote
					</button>
					<button
						onclick={() => selectedMemory && api.memories.demote(selectedMemory.id)}
						class="flex-1 px-3 py-2 rounded bg-decay/20 text-decay text-xs hover:bg-decay/30 transition"
					>
						Demote
					</button>
				</div>
			</div>
		</div>
	{/if}
</div>
