<!--
  ForgettingIndicator — v2.0.5 Active Forgetting status pill
  Shows the number of memories actively being suppressed via the `suppress`
  MCP tool (Anderson 2025 SIF + Davis Rac1). Pulses when count > 0.
-->
<script lang="ts">
	import { suppressedCount } from '$stores/websocket';
</script>

{#if $suppressedCount > 0}
	<div
		class="flex items-center gap-2 rounded-full border border-[#A33FFF]/40 bg-[#A33FFF]/10 px-3 py-1.5 text-xs shadow-[0_0_12px_rgba(163,63,255,0.15)]"
		title="Memories currently under top-down suppression (Anderson 2025 SIF)"
	>
		<div class="relative flex h-2 w-2 items-center justify-center">
			<span
				class="absolute inline-flex h-full w-full animate-ping rounded-full bg-[#A33FFF] opacity-75"
			></span>
			<span class="relative inline-flex h-2 w-2 rounded-full bg-[#A33FFF]"></span>
		</div>
		<span class="font-medium text-[#E4C8FF]">
			Actively forgetting {$suppressedCount} {$suppressedCount === 1 ? 'memory' : 'memories'}
		</span>
	</div>
{/if}
