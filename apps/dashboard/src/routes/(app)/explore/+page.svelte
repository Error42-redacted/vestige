<script lang="ts">
	import { api } from '$stores/api';
	import type { Memory } from '$types';

	let searchQuery = $state('');
	let targetQuery = $state('');
	let sourceMemory: Memory | null = $state(null);
	let targetMemory: Memory | null = $state(null);
	let associations: Record<string, unknown>[] = $state([]);
	let mode = $state<'associations' | 'chains' | 'bridges'>('associations');
	let loading = $state(false);
	let importanceText = $state('');
	let importanceResult: Record<string, unknown> | null = $state(null);

	const MODE_INFO: Record<string, { icon: string; desc: string }> = {
		associations: { icon: '◎', desc: 'Spreading activation — find related memories via graph traversal' },
		chains: { icon: '⟿', desc: 'Build reasoning path from source to target memory' },
		bridges: { icon: '⬡', desc: 'Find connecting memories between two concepts' },
	};

	async function findSource() {
		if (!searchQuery.trim()) return;
		loading = true;
		try {
			const res = await api.search(searchQuery, 1);
			if (res.results.length > 0) {
				sourceMemory = res.results[0];
				await explore();
			}
		} catch { /* ignore */ }
		finally { loading = false; }
	}

	async function findTarget() {
		if (!targetQuery.trim()) return;
		loading = true;
		try {
			const res = await api.search(targetQuery, 1);
			if (res.results.length > 0) {
				targetMemory = res.results[0];
				if (sourceMemory) await explore();
			}
		} catch { /* ignore */ }
		finally { loading = false; }
	}

	async function explore() {
		if (!sourceMemory) return;
		loading = true;
		try {
			const toId = (mode === 'chains' || mode === 'bridges') && targetMemory
				? targetMemory.id : undefined;
			const res = await api.explore(sourceMemory.id, mode, toId);
			associations = (res.results || res.nodes || res.chain || res.bridges || []) as Record<string, unknown>[];
		} catch { associations = []; }
		finally { loading = false; }
	}

	async function scoreImportance() {
		if (!importanceText.trim()) return;
		importanceResult = await api.importance(importanceText) as unknown as Record<string, unknown>;
	}

	function switchMode(m: typeof mode) {
		mode = m;
		if (sourceMemory) explore();
	}
</script>

<div class="p-6 max-w-5xl mx-auto space-y-8">
	<h1 class="text-xl text-bright font-semibold">Explore Connections</h1>

	<!-- Mode selector -->
	<div class="grid grid-cols-3 gap-2">
		{#each (['associations', 'chains', 'bridges'] as const) as m}
			<button onclick={() => switchMode(m)}
				class="flex flex-col items-center gap-1 p-3 rounded-lg text-sm transition
					{mode === m
						? 'bg-synapse/15 text-synapse-glow border border-synapse/40'
						: 'bg-surface/30 text-dim border border-subtle/20 hover:border-subtle/40'}">
				<span class="text-xl">{MODE_INFO[m].icon}</span>
				<span class="font-medium">{m.charAt(0).toUpperCase() + m.slice(1)}</span>
				<span class="text-[10px] text-muted text-center">{MODE_INFO[m].desc}</span>
			</button>
		{/each}
	</div>

	<!-- Search for source memory -->
	<div class="space-y-3">
		<label class="text-xs text-dim font-medium">Source Memory</label>
		<div class="flex gap-2">
			<input type="text" placeholder="Search for a memory to explore from..."
				bind:value={searchQuery}
				onkeydown={(e) => e.key === 'Enter' && findSource()}
				class="flex-1 px-4 py-2.5 bg-surface border border-subtle/40 rounded-lg text-text text-sm
					placeholder:text-muted focus:outline-none focus:border-synapse/60 transition" />
			<button onclick={findSource}
				class="px-4 py-2.5 bg-synapse/20 border border-synapse/40 text-synapse-glow text-sm rounded-lg hover:bg-synapse/30 transition">
				Find
			</button>
		</div>
	</div>

	{#if sourceMemory}
		<div class="p-3 bg-synapse/10 border border-synapse/30 rounded-lg">
			<div class="text-[10px] text-synapse-glow mb-1 uppercase tracking-wider">Source</div>
			<p class="text-sm text-text">{sourceMemory.content.slice(0, 200)}</p>
			<div class="flex gap-2 mt-1.5 text-[10px] text-muted">
				<span>{sourceMemory.nodeType}</span>
				<span>{(sourceMemory.retentionStrength * 100).toFixed(0)}% retention</span>
			</div>
		</div>
	{/if}

	<!-- Target memory (for chains/bridges) -->
	{#if mode === 'chains' || mode === 'bridges'}
		<div class="space-y-3">
			<label class="text-xs text-dim font-medium">Target Memory <span class="text-muted">(for {mode})</span></label>
			<div class="flex gap-2">
				<input type="text" placeholder="Search for the target memory..."
					bind:value={targetQuery}
					onkeydown={(e) => e.key === 'Enter' && findTarget()}
					class="flex-1 px-4 py-2.5 bg-surface border border-subtle/40 rounded-lg text-text text-sm
						placeholder:text-muted focus:outline-none focus:border-dream/60 transition" />
				<button onclick={findTarget}
					class="px-4 py-2.5 bg-dream/20 border border-dream/40 text-dream-glow text-sm rounded-lg hover:bg-dream/30 transition">
					Find
				</button>
			</div>
		</div>

		{#if targetMemory}
			<div class="p-3 bg-dream/10 border border-dream/30 rounded-lg">
				<div class="text-[10px] text-dream-glow mb-1 uppercase tracking-wider">Target</div>
				<p class="text-sm text-text">{targetMemory.content.slice(0, 200)}</p>
				<div class="flex gap-2 mt-1.5 text-[10px] text-muted">
					<span>{targetMemory.nodeType}</span>
					<span>{(targetMemory.retentionStrength * 100).toFixed(0)}% retention</span>
				</div>
			</div>
		{/if}
	{/if}

	<!-- Results -->
	{#if sourceMemory}
		{#if loading}
			<div class="text-center py-8 text-dim">
				<div class="text-lg animate-pulse mb-2">◎</div>
				<p>Exploring {mode}...</p>
			</div>
		{:else if associations.length > 0}
			<div class="space-y-4">
				<div class="flex items-center justify-between">
					<h2 class="text-sm text-bright font-semibold">{associations.length} Connections Found</h2>
				</div>
				<div class="space-y-2">
					{#each associations as assoc, i}
						<div class="p-3 bg-surface/40 border border-subtle/20 rounded-lg flex items-start gap-3 hover:border-subtle/40 transition">
							<div class="w-6 h-6 rounded-full bg-synapse/15 text-synapse-glow text-xs flex items-center justify-center flex-shrink-0 mt-0.5">
								{i + 1}
							</div>
							<div class="flex-1 min-w-0">
								<p class="text-sm text-text line-clamp-2">{assoc.content}</p>
								<div class="flex flex-wrap gap-3 mt-1.5 text-xs text-muted">
									{#if assoc.nodeType}<span class="px-1.5 py-0.5 bg-deep rounded">{assoc.nodeType}</span>{/if}
									{#if assoc.score}<span>Score: {Number(assoc.score).toFixed(3)}</span>{/if}
									{#if assoc.similarity}<span>Similarity: {Number(assoc.similarity).toFixed(3)}</span>{/if}
									{#if assoc.retention}<span>{(Number(assoc.retention) * 100).toFixed(0)}% retention</span>{/if}
									{#if assoc.connectionType}<span class="text-synapse-glow">{assoc.connectionType}</span>{/if}
								</div>
							</div>
						</div>
					{/each}
				</div>
			</div>
		{:else}
			<div class="text-center py-8 text-dim">
				<div class="text-3xl mb-3 opacity-20">◬</div>
				<p>No connections found for this query.</p>
			</div>
		{/if}
	{/if}

	<!-- Importance Scorer -->
	<div class="pt-8 border-t border-subtle/20">
		<h2 class="text-lg text-bright font-semibold mb-4">Importance Scorer</h2>
		<p class="text-xs text-muted mb-3">4-channel neuroscience scoring: novelty, arousal, reward, attention</p>
		<textarea
			bind:value={importanceText}
			placeholder="Paste any text to score its importance..."
			class="w-full h-24 px-4 py-3 bg-surface border border-subtle/40 rounded-lg text-text text-sm
				placeholder:text-muted resize-none focus:outline-none focus:border-synapse/60 transition"
		></textarea>
		<button onclick={scoreImportance}
			class="mt-2 px-4 py-2 bg-dream/20 border border-dream/40 text-dream-glow text-sm rounded-lg hover:bg-dream/30 transition">
			Score
		</button>

		{#if importanceResult}
			{@const channels = importanceResult.channels as Record<string, number> | undefined}
			{@const composite = Number(importanceResult.composite || importanceResult.compositeScore || 0)}
			<div class="mt-4 p-4 bg-surface/30 border border-subtle/20 rounded-lg">
				<div class="flex items-center gap-3 mb-4">
					<span class="text-3xl text-bright font-bold">{composite.toFixed(2)}</span>
					<span class="px-2 py-1 rounded text-xs {composite > 0.6
						? 'bg-recall/20 text-recall border border-recall/30'
						: 'bg-surface text-dim border border-subtle/30'}">
						{composite > 0.6 ? 'SAVE' : 'SKIP'}
					</span>
				</div>
				{#if channels}
					<div class="grid grid-cols-4 gap-3">
						{#each Object.entries(channels) as [channel, score]}
							<div>
								<div class="text-xs text-dim mb-1.5 capitalize">{channel}</div>
								<div class="h-2 bg-deep rounded-full overflow-hidden">
									<div class="h-full rounded-full transition-all duration-500
										{channel === 'novelty' ? 'bg-synapse' :
										 channel === 'arousal' ? 'bg-dream' :
										 channel === 'reward' ? 'bg-recall' : 'bg-amber-400'}"
										style="width: {score * 100}%"></div>
								</div>
								<div class="text-xs text-muted mt-1">{score.toFixed(2)}</div>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>
