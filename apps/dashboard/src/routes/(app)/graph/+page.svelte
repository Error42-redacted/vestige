<script lang="ts">
	import { onMount } from 'svelte';
	import Graph3D from '$components/Graph3D.svelte';
	import RetentionCurve from '$components/RetentionCurve.svelte';
	import { api } from '$stores/api';
	import { eventFeed } from '$stores/websocket';
	import type { GraphResponse, Memory } from '$types';

	let graphData: GraphResponse | null = $state(null);
	let selectedMemory: Memory | null = $state(null);
	let loading = $state(true);
	let error = $state('');
	let isDreaming = $state(false);
	let searchQuery = $state('');
	let maxNodes = $state(150);

	onMount(() => loadGraph());

	async function loadGraph(query?: string, centerId?: string) {
		loading = true;
		error = '';
		try {
			graphData = await api.graph({
				max_nodes: maxNodes,
				depth: 3,
				query: query || undefined,
				center_id: centerId || undefined
			});
		} catch {
			error = 'No memories yet. Start using Vestige to populate your graph.';
		} finally {
			loading = false;
		}
	}

	async function triggerDream() {
		isDreaming = true;
		try {
			await api.dream();
			await loadGraph();
		} catch { /* dream failed */ }
		finally { isDreaming = false; }
	}

	async function onNodeSelect(nodeId: string) {
		try {
			selectedMemory = await api.memories.get(nodeId);
		} catch {
			selectedMemory = null;
		}
	}

	function searchGraph() {
		if (searchQuery.trim()) loadGraph(searchQuery);
	}
</script>

<div class="h-full relative">
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
				<div class="text-5xl opacity-30">◎</div>
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

	<!-- Top controls bar -->
	<div class="absolute top-4 left-4 right-4 z-10 flex items-center gap-3">
		<!-- Search -->
		<div class="flex gap-2 flex-1 max-w-md">
			<input
				type="text"
				placeholder="Center graph on..."
				bind:value={searchQuery}
				onkeydown={(e) => e.key === 'Enter' && searchGraph()}
				class="flex-1 px-3 py-2 bg-abyss/80 backdrop-blur-sm border border-subtle/30 rounded-lg text-text text-sm
					placeholder:text-muted focus:outline-none focus:border-synapse/50 transition"
			/>
			<button onclick={searchGraph}
				class="px-3 py-2 bg-synapse/20 border border-synapse/40 text-synapse-glow text-sm rounded-lg hover:bg-synapse/30 transition backdrop-blur-sm">
				Focus
			</button>
		</div>

		<div class="flex gap-2 ml-auto">
			<!-- Node count -->
			<select bind:value={maxNodes} onchange={() => loadGraph()}
				class="px-2 py-2 bg-abyss/80 backdrop-blur-sm border border-subtle/30 rounded-lg text-dim text-xs">
				<option value={50}>50 nodes</option>
				<option value={100}>100 nodes</option>
				<option value={150}>150 nodes</option>
				<option value={200}>200 nodes</option>
			</select>

			<!-- Dream button -->
			<button
				onclick={triggerDream}
				disabled={isDreaming}
				class="px-4 py-2 rounded-lg bg-dream/20 border border-dream/40 text-dream-glow text-sm
					hover:bg-dream/30 transition-all backdrop-blur-sm disabled:opacity-50
					{isDreaming ? 'glow-dream animate-pulse-glow' : ''}"
			>
				{isDreaming ? '◈ Dreaming...' : '◈ Dream'}
			</button>

			<!-- Reload -->
			<button onclick={() => loadGraph()}
				class="px-3 py-2 bg-abyss/80 backdrop-blur-sm border border-subtle/30 rounded-lg text-dim text-sm hover:text-text transition">
				↻
			</button>
		</div>
	</div>

	<!-- Bottom stats -->
	<div class="absolute bottom-4 left-4 z-10 text-xs text-dim backdrop-blur-sm bg-abyss/60 rounded-lg px-3 py-2 border border-subtle/20">
		{#if graphData}
			<span>{graphData.nodeCount} nodes</span>
			<span class="mx-2 text-subtle">·</span>
			<span>{graphData.edgeCount} edges</span>
			<span class="mx-2 text-subtle">·</span>
			<span>depth {graphData.depth}</span>
		{/if}
	</div>

	<!-- Selected memory panel -->
	{#if selectedMemory}
		<div class="absolute right-0 top-0 h-full w-96 bg-abyss/95 backdrop-blur-xl border-l border-subtle/30 p-6 overflow-y-auto z-20
			transition-transform duration-300">
			<div class="flex justify-between items-start mb-4">
				<h3 class="text-bright text-sm font-semibold">Memory Detail</h3>
				<button onclick={() => selectedMemory = null} class="text-dim hover:text-text text-lg leading-none">×</button>
			</div>

			<div class="space-y-4">
				<div class="flex gap-2 flex-wrap">
					<span class="px-2 py-0.5 rounded text-xs bg-synapse/20 text-synapse-glow">{selectedMemory.nodeType}</span>
					{#each selectedMemory.tags as tag}
						<span class="px-2 py-0.5 rounded text-xs bg-surface text-dim">{tag}</span>
					{/each}
				</div>

				<div class="text-sm text-text leading-relaxed whitespace-pre-wrap max-h-64 overflow-y-auto">{selectedMemory.content}</div>

				<!-- FSRS bars -->
				<div class="space-y-2">
					{#each [
						{ label: 'Retention', value: selectedMemory.retentionStrength },
						{ label: 'Storage', value: selectedMemory.storageStrength },
						{ label: 'Retrieval', value: selectedMemory.retrievalStrength }
					] as bar}
						<div>
							<div class="flex justify-between text-xs text-dim mb-0.5">
								<span>{bar.label}</span>
								<span>{(bar.value * 100).toFixed(1)}%</span>
							</div>
							<div class="h-1.5 bg-surface rounded-full overflow-hidden">
								<div
									class="h-full rounded-full transition-all duration-500"
									style="width: {bar.value * 100}%; background: {
										bar.value > 0.7 ? '#10b981' :
										bar.value > 0.4 ? '#f59e0b' : '#ef4444'
									}"
								></div>
							</div>
						</div>
					{/each}
				</div>

				<!-- FSRS Decay Curve -->
				<div>
					<div class="text-xs text-dim mb-1 font-medium">Retention Forecast</div>
					<RetentionCurve
						retention={selectedMemory.retentionStrength}
						stability={selectedMemory.storageStrength * 30}
					/>
				</div>

				<div class="text-xs text-muted space-y-1">
					<div>Created: {new Date(selectedMemory.createdAt).toLocaleString()}</div>
					<div>Updated: {new Date(selectedMemory.updatedAt).toLocaleString()}</div>
					{#if selectedMemory.lastAccessedAt}
						<div>Accessed: {new Date(selectedMemory.lastAccessedAt).toLocaleString()}</div>
					{/if}
					<div>Reviews: {selectedMemory.reviewCount ?? 0}</div>
				</div>

				<div class="flex gap-2 pt-2">
					<button
						onclick={() => { if (selectedMemory) { api.memories.promote(selectedMemory.id); } }}
						class="flex-1 px-3 py-2 rounded bg-recall/20 text-recall text-xs hover:bg-recall/30 transition"
					>
						↑ Promote
					</button>
					<button
						onclick={() => { if (selectedMemory) { api.memories.demote(selectedMemory.id); } }}
						class="flex-1 px-3 py-2 rounded bg-decay/20 text-decay text-xs hover:bg-decay/30 transition"
					>
						↓ Demote
					</button>
				</div>

				<!-- Explore from this node -->
				<a
					href="/explore"
					class="block text-center px-3 py-2 rounded bg-dream/10 text-dream-glow text-xs hover:bg-dream/20 transition border border-dream/20"
				>
					◬ Explore Connections
				</a>
			</div>
		</div>
	{/if}
</div>
