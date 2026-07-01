<script lang="ts">
	import { onMount } from 'svelte';
	import { base } from '$app/paths';

	type SubmitState = 'idle' | 'submitting' | 'success' | 'error';
	type SupportMessage = {
		role: 'bot' | 'user';
		content: string;
	};

	let canvas: HTMLCanvasElement;
	let name = $state('');
	let email = $state('');
	let role = $state('solo');
	let priority = $state('sync');
	let notes = $state('');
	let companySite = $state('');
	let submitState = $state<SubmitState>('idle');
	let submitMessage = $state('');
	let botQuestion = $state('');
	let botBusy = $state(false);
	let botMessages = $state<SupportMessage[]>([
		{
			role: 'bot',
			content: 'Ask me about installing Vestige, whether heavy models are required, Solo vs Team Pro, sync, pricing, or what happens after you join the June list.'
		}
	]);

	const waitlistEndpoint = import.meta.env.VITE_WAITLIST_ENDPOINT as string | undefined;
	const supportBotEndpoint = import.meta.env.VITE_SUPPORT_BOT_ENDPOINT as string | undefined;

	const proofPoints = [
		{ value: 'Local', label: 'SQLite memory, no hosted memory service' },
		{ value: 'MCP', label: 'Claude Code, Cursor, Cline, Codex, Goose' },
		{ value: 'June', label: 'Pro sync, backup, team memory early access' }
	];

	const proTracks = [
		{
			name: 'Solo Pro',
			accent: '#22c55e',
			copy: 'Multi-device sync, encrypted backups, managed updates, and a cleaner memory dashboard for developers living inside AI coding agents.'
		},
		{
			name: 'Team Pro',
			accent: '#06b6d4',
			copy: 'Shared project memory, admin review, audit trails, PostgreSQL-backed deployments, and async support for engineering teams.'
		}
	];

	const launchPillars = [
		'Private by default',
		'Sync without lock-in',
		'Team memory controls',
		'Bot-assisted support'
	];

	const supportPrompts = [
		{ label: 'Install', prompt: 'How do I install Vestige and connect it to Claude Code?' },
		{ label: 'No 20GB?', prompt: 'Do I need the Sanhedrin model or 20GB of RAM?' },
		{ label: 'Solo vs Team', prompt: 'Should I choose Solo Pro or Team Pro?' },
		{ label: 'Sync', prompt: 'How will Pro sync and backups work?' },
		{ label: 'Pricing', prompt: 'How much will Vestige Pro cost?' },
		{ label: 'Human help', prompt: 'When does a human get involved?' }
	];

	onMount(() => {
		const rawContext = canvas.getContext('2d');
		if (!rawContext) return;
		const context: CanvasRenderingContext2D = rawContext;

		let frame = 0;
		let width = 0;
		let height = 0;
		const nodeCount = 62;
		const nodes = Array.from({ length: nodeCount }, (_, index) => ({
			x: Math.random(),
			y: Math.random(),
			vx: (Math.random() - 0.5) * 0.00016,
			vy: (Math.random() - 0.5) * 0.00016,
			phase: Math.random() * Math.PI * 2,
			kind: index % 5
		}));

		function resize() {
			const dpr = Math.min(window.devicePixelRatio || 1, 2);
			width = window.innerWidth;
			height = window.innerHeight;
			canvas.width = Math.floor(width * dpr);
			canvas.height = Math.floor(height * dpr);
			canvas.style.width = `${width}px`;
			canvas.style.height = `${height}px`;
			context.setTransform(dpr, 0, 0, dpr, 0, 0);
		}

		function draw(time: number) {
			context.clearRect(0, 0, width, height);
			const gradient = context.createLinearGradient(0, 0, width, height);
			gradient.addColorStop(0, '#07100f');
			gradient.addColorStop(0.45, '#0b1221');
			gradient.addColorStop(1, '#15100a');
			context.fillStyle = gradient;
			context.fillRect(0, 0, width, height);

			context.strokeStyle = 'rgba(148, 163, 184, 0.08)';
			context.lineWidth = 1;
			for (let x = 0; x < width; x += 72) {
				context.beginPath();
				context.moveTo(x, 0);
				context.lineTo(x + Math.sin(time / 3000 + x) * 12, height);
				context.stroke();
			}
			for (let y = 0; y < height; y += 72) {
				context.beginPath();
				context.moveTo(0, y);
				context.lineTo(width, y + Math.cos(time / 3300 + y) * 12);
				context.stroke();
			}

			for (const node of nodes) {
				node.x += node.vx;
				node.y += node.vy;
				if (node.x < 0.04 || node.x > 0.96) node.vx *= -1;
				if (node.y < 0.06 || node.y > 0.94) node.vy *= -1;
			}

			for (let i = 0; i < nodes.length; i++) {
				const a = nodes[i];
				const ax = a.x * width;
				const ay = a.y * height;
				for (let j = i + 1; j < nodes.length; j++) {
					const b = nodes[j];
					const bx = b.x * width;
					const by = b.y * height;
					const dx = ax - bx;
					const dy = ay - by;
					const distance = Math.sqrt(dx * dx + dy * dy);
					if (distance < 168) {
						const alpha = (1 - distance / 168) * 0.18;
						context.strokeStyle = `rgba(34, 197, 94, ${alpha})`;
						context.beginPath();
						context.moveTo(ax, ay);
						context.lineTo(bx, by);
						context.stroke();
					}
				}
			}

			const colors = ['#22c55e', '#06b6d4', '#f59e0b', '#ef4444', '#a3e635'];
			for (const node of nodes) {
				const pulse = 0.5 + Math.sin(time / 900 + node.phase) * 0.5;
				const x = node.x * width;
				const y = node.y * height;
				context.fillStyle = colors[node.kind];
				context.globalAlpha = 0.45 + pulse * 0.35;
				context.beginPath();
				context.arc(x, y, 1.6 + pulse * 1.8, 0, Math.PI * 2);
				context.fill();
				context.globalAlpha = 1;
			}

			frame = requestAnimationFrame(draw);
		}

		resize();
		window.addEventListener('resize', resize);
		frame = requestAnimationFrame(draw);

		return () => {
			cancelAnimationFrame(frame);
			window.removeEventListener('resize', resize);
		};
	});

	function githubWaitlistUrl() {
		const body = [
			'## Vestige Pro waitlist',
			'',
			`Plan: ${role}`,
			`Priority: ${priority}`,
			notes.trim() ? `Use case: ${notes.trim()}` : 'Use case:',
			'',
			'Please do not include private email addresses in this public issue.'
		].join('\n');

		return `https://github.com/samvallad33/vestige/issues/new?title=${encodeURIComponent('Vestige Pro waitlist')}&body=${encodeURIComponent(body)}`;
	}

	async function joinWaitlist(event: SubmitEvent) {
		event.preventDefault();
		submitState = 'submitting';
		submitMessage = '';

		if (companySite.trim()) {
			submitState = 'success';
			submitMessage = 'You are on the list.';
			return;
		}

		if (!email.includes('@')) {
			submitState = 'error';
			submitMessage = 'Enter an email so the early-access invite can reach you.';
			return;
		}

		const payload = {
			name: name.trim(),
			email: email.trim(),
			plan: role,
			priority,
			notes: notes.trim(),
			source: 'vestige-pro-waitlist',
			createdAt: new Date().toISOString()
		};

		if (!waitlistEndpoint) {
			submitState = 'success';
			submitMessage = 'Email capture is ready for an endpoint. Opening the GitHub waitlist fallback with your email omitted.';
			window.open(githubWaitlistUrl(), '_blank', 'noopener,noreferrer');
			return;
		}

		try {
			const response = await fetch(waitlistEndpoint, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(payload)
			});
			if (!response.ok) throw new Error(`Waitlist endpoint returned ${response.status}`);
			submitState = 'success';
			submitMessage = 'You are on the June early-access list.';
			name = '';
			email = '';
			notes = '';
		} catch (error) {
			submitState = 'error';
			submitMessage = error instanceof Error
				? error.message
				: 'The waitlist endpoint did not accept the request.';
		}
	}

	function localSupportAnswer(question: string) {
		const query = question.toLowerCase();

		if (/(install|setup|onboard|claude|cursor|cline|codex|connect)/.test(query)) {
			return [
				'Start with the open-source install:',
				'1. `npm install -g vestige-mcp-server@latest`',
				'2. Claude Code: `claude mcp add vestige vestige-mcp -s user`',
				'3. Codex: `codex mcp add vestige -- vestige-mcp`',
				'Then test it by asking your agent to remember a preference, opening a fresh session, and asking for that preference back.'
			].join('\n');
		}

		if (/(sanhedrin|20gb|20 gb|ram|heavy|model|mlx|preflight|hook)/.test(query)) {
			return 'No. The default Vestige path is the local MCP memory server. Sanhedrin, preflight hooks, and large local verifier models are optional. Pro should keep that promise: nobody should need a 20GB machine just to use memory.';
		}

		if (/(solo|team|plan|seat|buying)/.test(query)) {
			return 'Choose Solo Pro if you want your own multi-device memory, backups, smoother updates, and personal support. Choose Team Pro if multiple people need shared project memory, admin controls, PostgreSQL-backed storage, audit trails, or team onboarding.';
		}

		if (/(sync|backup|device|dropbox|icloud|syncthing|postgres|postgresql|pg|central)/.test(query)) {
			return 'Open-source Vestige should stay local-first. Pro is where guided sync, encrypted backups, conflict handling, and Team Pro PostgreSQL-backed storage belong. The important design rule: users own memory and can export it.';
		}

		if (/(price|pricing|cost|pay|billing|stripe|lemon|subscription|monthly|yearly)/.test(query)) {
			return 'Pricing is not final yet. The current plan is simple: Solo Pro for individual developers, Team Pro for engineering teams. Join the waitlist so early users can shape pricing before the June launch.';
		}

		if (/(update|upgrade|curl|reinstall|version)/.test(query)) {
			return 'Use `vestige update` for existing installs. The goal is that users should not need to keep copying curl commands just to stay current.';
		}

		if (/(privacy|local|cloud|telemetry|data|where.*stored|sqlite)/.test(query)) {
			return 'Vestige core stores memory locally in SQLite and does not need a hosted memory service. Pro should add convenience around sync, backup, and teams without turning private local memory into a black box.';
		}

		if (/(support|bot|human|email|question|help|available|awake|discord)/.test(query)) {
			return 'The support bot should answer common install, sync, plan, and onboarding questions instantly. Hard cases should escalate with context so a human teammate only handles the issues that actually need human judgment.';
		}

		if (/(waitlist|june|early|launch|invite|after)/.test(query)) {
			return 'After you join the waitlist, the June early-access flow should invite you into the right lane: Solo Pro for personal memory, Team Pro for shared memory and admin controls. The bot will keep onboarding answers available while the launch scales.';
		}

		return 'I can help with install, updates, optional heavy models, Solo vs Team Pro, sync, backups, privacy, pricing, and support escalation. For now, the fastest next step is to join the waitlist and include your use case so the June onboarding can prioritize the right workflows.';
	}

	async function askSupportBot(event?: SubmitEvent, prompt?: string) {
		event?.preventDefault();
		const question = (prompt ?? botQuestion).trim();
		if (!question || botBusy) return;

		botQuestion = '';
		botBusy = true;
		botMessages = [...botMessages, { role: 'user', content: question }];

		if (!supportBotEndpoint) {
			botMessages = [...botMessages, { role: 'bot', content: localSupportAnswer(question) }];
			botBusy = false;
			return;
		}

		try {
			const response = await fetch(supportBotEndpoint, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					question,
					plan: role,
					priority,
					source: 'vestige-pro-waitlist',
					history: botMessages.slice(-6)
				})
			});
			if (!response.ok) throw new Error(`Support bot endpoint returned ${response.status}`);
			const data = await response.json();
			botMessages = [
				...botMessages,
				{ role: 'bot', content: String(data.answer ?? data.message ?? localSupportAnswer(question)) }
			];
		} catch {
			botMessages = [
				...botMessages,
				{ role: 'bot', content: localSupportAnswer(question) }
			];
		} finally {
			botBusy = false;
		}
	}
</script>

<svelte:head>
	<title>Vestige Pro Waitlist</title>
	<meta
		name="description"
		content="Join the Vestige Pro waitlist for local-first AI agent memory sync, backups, team memory, PostgreSQL-backed storage, and bot-assisted support."
	/>
</svelte:head>

<div class="waitlist-shell">
	<canvas bind:this={canvas} class="memory-field" aria-hidden="true"></canvas>
	<div class="field-vignette" aria-hidden="true"></div>

	<header class="topbar">
		<a class="brand" href={`${base}/waitlist`} aria-label="Vestige Pro waitlist home">
			<span class="brand-mark">V</span>
			<span>Vestige Pro</span>
		</a>
		<nav aria-label="Waitlist navigation">
			<a href="https://github.com/samvallad33/vestige" target="_blank" rel="noreferrer">GitHub</a>
			<a href={`${base}/graph`}>Dashboard</a>
			<a class="nav-cta" href="#join">Join</a>
		</nav>
	</header>

	<main>
		<section class="hero" aria-labelledby="hero-title">
			<div class="hero-copy">
				<p class="eyebrow">June early access</p>
				<h1 id="hero-title">Vestige Pro</h1>
				<p class="hero-subtitle">
					The paid layer for developers and teams who already trust Vestige as local memory for AI agents.
					Sync, backups, team memory, and bot-assisted support come next.
				</p>

				<div class="hero-actions" aria-label="Primary actions">
					<a class="primary-link" href="#join">Join the waitlist <span aria-hidden="true">-&gt;</span></a>
					<a class="secondary-link" href="https://github.com/samvallad33/vestige" target="_blank" rel="noreferrer">
						View open source
					</a>
				</div>

				<div class="proof-row" aria-label="Vestige proof points">
					{#each proofPoints as point}
						<div class="proof-item">
							<strong>{point.value}</strong>
							<span>{point.label}</span>
						</div>
					{/each}
				</div>
			</div>

			<form id="join" class="waitlist-form" onsubmit={joinWaitlist}>
				<div class="form-heading">
					<p>Early access</p>
					<h2>Reserve a Pro seat</h2>
				</div>

				<label>
					<span>Name</span>
					<input bind:value={name} autocomplete="name" name="name" placeholder="Alex" />
				</label>

				<label>
					<span>Email</span>
					<input bind:value={email} autocomplete="email" name="email" placeholder="you@example.com" type="email" required />
				</label>

				<label>
					<span>Who are you buying for?</span>
					<select bind:value={role} name="role">
						<option value="solo">Solo Pro</option>
						<option value="team">Team Pro</option>
					</select>
				</label>

				<label>
					<span>What matters most?</span>
					<select bind:value={priority} name="priority">
						<option value="sync">Multi-device sync</option>
						<option value="team-memory">Shared team memory</option>
						<option value="postgres">PostgreSQL / central backend</option>
						<option value="support-bot">Bot-assisted support</option>
					</select>
				</label>

				<label>
					<span>Use case</span>
					<textarea
						bind:value={notes}
						name="notes"
						placeholder="Tell us where your agent keeps forgetting context."
						rows="4"
					></textarea>
				</label>

				<label class="hidden-field" aria-hidden="true">
					<span>Company site</span>
					<input bind:value={companySite} name="company_site" tabindex="-1" autocomplete="off" />
				</label>

				<button class="submit-button" type="submit" disabled={submitState === 'submitting'}>
					{submitState === 'submitting' ? 'Saving...' : 'Join June early access'}
				</button>

				{#if submitMessage}
					<p class="submit-message" class:success={submitState === 'success'} class:error={submitState === 'error'}>
						{submitMessage}
					</p>
				{/if}
			</form>
		</section>

		<section class="signal-band" aria-label="Launch focus">
			{#each launchPillars as pillar}
				<div>{pillar}</div>
			{/each}
		</section>

		<section class="pro-grid" aria-labelledby="pro-title">
			<div class="section-heading">
				<p>Why Pro exists</p>
				<h2 id="pro-title">Two plans. One promise: agent memory you can depend on.</h2>
			</div>
			<div class="track-grid">
				{#each proTracks as track}
					<article class="track" style={`--track-color: ${track.accent}`}>
						<div class="track-line"></div>
						<h3>{track.name}</h3>
						<p>{track.copy}</p>
					</article>
				{/each}
			</div>
		</section>

		<section class="support-bot" aria-labelledby="bot-title">
			<div>
				<p class="eyebrow">Always-on answers</p>
				<h2 id="bot-title">The support bot handles the first wave.</h2>
				<p class="bot-intro">
					This is the first support layer: instant onboarding answers before anyone has to write an email.
					It can run locally from the FAQ now and call a hosted support endpoint later.
				</p>
			</div>
			<div class="bot-panel">
				<div class="bot-status">
					<span class="bot-light" aria-hidden="true"></span>
					<span>Onboarding bot</span>
					<small>{supportBotEndpoint ? 'Connected' : 'FAQ mode'}</small>
				</div>

				<div class="bot-messages" aria-live="polite">
					{#each botMessages as message}
						<div class:bot-bubble={message.role === 'bot'} class:user-bubble={message.role === 'user'}>
							{#each message.content.split('\n') as line}
								<p>{line}</p>
							{/each}
						</div>
					{/each}
					{#if botBusy}
						<div class="bot-bubble">
							<p>Checking the onboarding notes...</p>
						</div>
					{/if}
				</div>

				<div class="prompt-row" aria-label="Common onboarding questions">
					{#each supportPrompts as prompt}
						<button type="button" onclick={() => askSupportBot(undefined, prompt.prompt)}>
							{prompt.label}
						</button>
					{/each}
				</div>

				<form class="bot-input" onsubmit={askSupportBot}>
					<input
						bind:value={botQuestion}
						name="support_question"
						placeholder="Ask about install, sync, pricing, or Team Pro"
						aria-label="Ask the Vestige support bot"
					/>
					<button type="submit" disabled={botBusy || !botQuestion.trim()}>
						Ask
					</button>
				</form>
			</div>
		</section>

		<section class="roadmap" aria-labelledby="roadmap-title">
			<div>
				<p class="eyebrow">May to June</p>
				<h2 id="roadmap-title">The plan is simple.</h2>
			</div>
			<ol>
				<li>
					<strong>May</strong>
					<span>Get Vestige into every MCP, Claude Code, Cursor, local AI, Rust, and self-hosted channel that cares about agent memory.</span>
				</li>
				<li>
					<strong>June</strong>
					<span>Invite the first Solo Pro and Team Pro users into sync, backups, shared memory, PostgreSQL-backed deployments, and bot-assisted support.</span>
				</li>
				<li>
					<strong>After</strong>
					<span>Use paid feedback to turn Vestige from a beloved local tool into durable agent-memory infrastructure.</span>
				</li>
			</ol>
		</section>
	</main>
</div>

<style>
	:global(body) {
		overflow: hidden;
	}

	.waitlist-shell {
		position: fixed;
		inset: 0;
		overflow-y: auto;
		background: #07100f;
		color: #edf7f2;
		font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
	}

	.memory-field,
	.field-vignette {
		position: fixed;
		inset: 0;
		pointer-events: none;
	}

	.memory-field {
		z-index: 0;
	}

	.field-vignette {
		z-index: 1;
		background:
			linear-gradient(90deg, rgba(7, 16, 15, 0.92) 0%, rgba(7, 16, 15, 0.62) 48%, rgba(7, 16, 15, 0.88) 100%),
			linear-gradient(180deg, rgba(7, 16, 15, 0.2) 0%, rgba(7, 16, 15, 0.82) 100%);
	}

	.topbar,
	main {
		position: relative;
		z-index: 2;
	}

	.topbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		width: min(1180px, calc(100% - 2rem));
		margin: 0 auto;
		padding: 1rem 0;
	}

	.brand,
	.topbar nav,
	.hero-actions,
	.proof-row,
	.signal-band,
	.track-grid,
	.support-bot,
	.roadmap,
	.roadmap li {
		display: flex;
	}

	.brand {
		align-items: center;
		gap: 0.7rem;
		color: #ffffff;
		text-decoration: none;
		font-weight: 800;
	}

	.brand-mark {
		display: grid;
		place-items: center;
		width: 2.15rem;
		height: 2.15rem;
		border: 1px solid rgba(34, 197, 94, 0.48);
		border-radius: 8px;
		background: linear-gradient(135deg, rgba(34, 197, 94, 0.24), rgba(6, 182, 212, 0.16));
		color: #bbf7d0;
	}

	.topbar nav {
		align-items: center;
		gap: 0.4rem;
	}

	.topbar a {
		color: #b8c7c0;
		text-decoration: none;
	}

	.topbar nav a {
		border-radius: 8px;
		padding: 0.65rem 0.85rem;
		font-size: 0.88rem;
	}

	.topbar nav a:hover,
	.nav-cta {
		background: rgba(255, 255, 255, 0.07);
		color: #ffffff;
	}

	main {
		width: min(1180px, calc(100% - 2rem));
		margin: 0 auto;
	}

	.hero {
		display: grid;
		grid-template-columns: minmax(0, 1.05fr) minmax(320px, 0.7fr);
		gap: clamp(2rem, 6vw, 5rem);
		align-items: center;
		min-height: 86vh;
		padding: clamp(2rem, 5vw, 4.8rem) 0 2rem;
	}

	.hero-copy {
		max-width: 720px;
	}

	.eyebrow,
	.form-heading p,
	.section-heading p {
		margin: 0 0 0.8rem;
		color: #67e8f9;
		font-size: 0.78rem;
		font-weight: 800;
		letter-spacing: 0;
		text-transform: uppercase;
	}

	h1,
	h2,
	h3,
	p {
		margin-top: 0;
	}

	h1 {
		margin-bottom: 1.1rem;
		color: #ffffff;
		font-size: clamp(3.8rem, 13vw, 8rem);
		line-height: 0.92;
		letter-spacing: 0;
	}

	.hero-subtitle {
		max-width: 680px;
		margin-bottom: 1.6rem;
		color: #d7e6df;
		font-size: clamp(1.05rem, 2.4vw, 1.45rem);
		line-height: 1.5;
	}

	.hero-actions {
		flex-wrap: wrap;
		gap: 0.8rem;
		margin-bottom: 2rem;
	}

	.primary-link,
	.secondary-link,
	.submit-button {
		border-radius: 8px;
		font-weight: 800;
		text-decoration: none;
	}

	.primary-link,
	.submit-button {
		border: 1px solid rgba(34, 197, 94, 0.86);
		background: #22c55e;
		color: #04130b;
		box-shadow: 0 20px 42px rgba(34, 197, 94, 0.19);
	}

	.primary-link,
	.secondary-link {
		padding: 0.88rem 1rem;
	}

	.secondary-link {
		border: 1px solid rgba(226, 232, 240, 0.2);
		background: rgba(255, 255, 255, 0.06);
		color: #edf7f2;
	}

	.proof-row {
		flex-wrap: wrap;
		gap: 0.7rem;
	}

	.proof-item {
		width: min(100%, 13rem);
		border: 1px solid rgba(226, 232, 240, 0.12);
		border-radius: 8px;
		background: rgba(5, 12, 11, 0.54);
		padding: 0.9rem;
	}

	.proof-item strong,
	.proof-item span {
		display: block;
	}

	.proof-item strong {
		margin-bottom: 0.4rem;
		color: #bbf7d0;
		font-size: 1.05rem;
	}

	.proof-item span {
		color: #a8bbb2;
		font-size: 0.82rem;
		line-height: 1.45;
	}

	.waitlist-form {
		border: 1px solid rgba(226, 232, 240, 0.16);
		border-radius: 8px;
		background: rgba(5, 12, 11, 0.82);
		box-shadow: 0 28px 90px rgba(0, 0, 0, 0.36);
		padding: clamp(1rem, 3vw, 1.35rem);
	}

	.form-heading h2,
	.section-heading h2,
	.roadmap h2 {
		margin-bottom: 1rem;
		color: #ffffff;
		font-size: clamp(1.6rem, 4vw, 2.7rem);
		line-height: 1.05;
		letter-spacing: 0;
	}

	.form-heading h2 {
		font-size: clamp(1.4rem, 3vw, 2rem);
	}

	label {
		display: block;
		margin-top: 0.85rem;
	}

	label span {
		display: block;
		margin-bottom: 0.38rem;
		color: #a8bbb2;
		font-size: 0.78rem;
		font-weight: 750;
	}

	input,
	select,
	textarea {
		width: 100%;
		border: 1px solid rgba(226, 232, 240, 0.16);
		border-radius: 8px;
		background: rgba(255, 255, 255, 0.065);
		color: #ffffff;
		font: inherit;
		padding: 0.78rem 0.82rem;
		outline: none;
	}

	select {
		color-scheme: dark;
	}

	textarea {
		resize: vertical;
		min-height: 6rem;
	}

	input:focus,
	select:focus,
	textarea:focus {
		border-color: rgba(34, 197, 94, 0.9);
		box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.14);
	}

	.hidden-field {
		position: absolute;
		left: -10000px;
		height: 1px;
		overflow: hidden;
	}

	.submit-button {
		width: 100%;
		margin-top: 1rem;
		padding: 0.95rem 1rem;
		cursor: pointer;
		font: inherit;
	}

	.submit-button:disabled {
		cursor: wait;
		opacity: 0.72;
	}

	.submit-message {
		margin: 0.8rem 0 0;
		font-size: 0.82rem;
		line-height: 1.45;
	}

	.submit-message.success {
		color: #86efac;
	}

	.submit-message.error {
		color: #fca5a5;
	}

	.signal-band {
		flex-wrap: wrap;
		gap: 0.65rem;
		border-top: 1px solid rgba(226, 232, 240, 0.12);
		border-bottom: 1px solid rgba(226, 232, 240, 0.12);
		padding: 1rem 0;
	}

	.signal-band div {
		border-radius: 999px;
		background: rgba(255, 255, 255, 0.07);
		color: #d7e6df;
		padding: 0.55rem 0.75rem;
		font-size: 0.84rem;
	}

	.pro-grid,
	.roadmap {
		padding: clamp(3rem, 7vw, 5rem) 0;
	}

	.section-heading {
		max-width: 760px;
		margin-bottom: 1.7rem;
	}

	.track-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0.9rem;
	}

	.track {
		border: 1px solid rgba(226, 232, 240, 0.13);
		border-radius: 8px;
		background: rgba(5, 12, 11, 0.66);
		padding: 1.1rem;
	}

	.track-line {
		width: 3.5rem;
		height: 0.22rem;
		margin-bottom: 1rem;
		border-radius: 999px;
		background: var(--track-color);
	}

	.track h3 {
		margin-bottom: 0.65rem;
		color: #ffffff;
		font-size: 1.08rem;
	}

	.track p,
	.roadmap span {
		color: #a8bbb2;
		line-height: 1.55;
	}

	.support-bot,
	.roadmap {
		align-items: flex-start;
		justify-content: space-between;
		gap: clamp(2rem, 6vw, 4rem);
		border-top: 1px solid rgba(226, 232, 240, 0.12);
	}

	.support-bot {
		padding: clamp(2.6rem, 6vw, 4.5rem) 0;
	}

	.support-bot > div:first-child,
	.roadmap > div {
		flex: 0 0 min(28rem, 100%);
	}

	.bot-intro {
		max-width: 34rem;
		color: #a8bbb2;
		line-height: 1.55;
	}

	.bot-panel {
		flex: 1;
		border: 1px solid rgba(226, 232, 240, 0.13);
		border-radius: 8px;
		background: rgba(5, 12, 11, 0.62);
		padding: clamp(1rem, 3vw, 1.25rem);
	}

	.bot-status,
	.prompt-row,
	.bot-input {
		display: flex;
		align-items: center;
	}

	.bot-status {
		gap: 0.55rem;
		margin-bottom: 0.9rem;
		color: #d7e6df;
		font-size: 0.86rem;
		font-weight: 800;
	}

	.bot-status small {
		margin-left: auto;
		border: 1px solid rgba(34, 197, 94, 0.22);
		border-radius: 999px;
		padding: 0.3rem 0.5rem;
		color: #86efac;
		font-size: 0.72rem;
	}

	.bot-light {
		width: 0.42rem;
		height: 0.42rem;
		border-radius: 999px;
		background: #22c55e;
		box-shadow: 0 0 18px rgba(34, 197, 94, 0.7);
	}

	.bot-messages {
		display: grid;
		gap: 0.75rem;
		max-height: 22rem;
		overflow-y: auto;
		border: 1px solid rgba(226, 232, 240, 0.1);
		border-radius: 8px;
		background: rgba(255, 255, 255, 0.035);
		padding: 0.8rem;
	}

	.bot-bubble,
	.user-bubble {
		max-width: 88%;
		border-radius: 8px;
		padding: 0.75rem 0.82rem;
	}

	.bot-bubble {
		justify-self: start;
		border: 1px solid rgba(34, 197, 94, 0.18);
		background: rgba(34, 197, 94, 0.08);
		color: #d7e6df;
	}

	.user-bubble {
		justify-self: end;
		border: 1px solid rgba(6, 182, 212, 0.24);
		background: rgba(6, 182, 212, 0.12);
		color: #ffffff;
	}

	.bot-bubble p,
	.user-bubble p {
		margin: 0;
		font-size: 0.86rem;
		line-height: 1.5;
		white-space: pre-wrap;
	}

	.bot-bubble p + p,
	.user-bubble p + p {
		margin-top: 0.35rem;
	}

	.prompt-row {
		flex-wrap: wrap;
		gap: 0.45rem;
		margin: 0.85rem 0;
	}

	.prompt-row button {
		border: 1px solid rgba(226, 232, 240, 0.14);
		border-radius: 999px;
		background: rgba(255, 255, 255, 0.055);
		color: #d7e6df;
		cursor: pointer;
		font: inherit;
		font-size: 0.78rem;
		padding: 0.46rem 0.62rem;
	}

	.prompt-row button:hover {
		border-color: rgba(34, 197, 94, 0.42);
		color: #ffffff;
	}

	.bot-input {
		gap: 0.55rem;
	}

	.bot-input input {
		margin: 0;
	}

	.bot-input button {
		flex: 0 0 auto;
		border: 1px solid rgba(34, 197, 94, 0.86);
		border-radius: 8px;
		background: #22c55e;
		color: #04130b;
		cursor: pointer;
		font: inherit;
		font-weight: 800;
		padding: 0.78rem 0.95rem;
	}

	.bot-input button:disabled {
		cursor: not-allowed;
		opacity: 0.45;
	}

	.roadmap ol {
		display: grid;
		gap: 0.8rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.roadmap li {
		gap: 1rem;
		align-items: flex-start;
		border: 1px solid rgba(226, 232, 240, 0.13);
		border-radius: 8px;
		background: rgba(5, 12, 11, 0.58);
		padding: 1rem;
	}

	.roadmap strong {
		flex: 0 0 4.5rem;
		color: #fbbf24;
	}

	@media (max-width: 900px) {
		.topbar {
			align-items: flex-start;
			flex-direction: column;
		}

		.hero,
		.track-grid,
		.support-bot,
		.roadmap {
			grid-template-columns: 1fr;
		}

		.hero {
			display: block;
			min-height: auto;
			padding-top: 2rem;
		}

		.waitlist-form {
			margin-top: 2rem;
		}

		.support-bot,
		.roadmap {
			display: block;
		}

		.bot-panel {
			margin-top: 1rem;
		}
	}

	@media (max-width: 560px) {
		main,
		.topbar {
			width: min(100% - 1rem, 1180px);
		}

		.topbar nav {
			width: 100%;
			justify-content: space-between;
		}

		.topbar nav a {
			padding: 0.58rem 0.62rem;
			font-size: 0.8rem;
		}

		h1 {
			font-size: clamp(3.35rem, 18vw, 4.8rem);
		}

		.hero-subtitle {
			font-size: 1rem;
		}

		.proof-item {
			width: 100%;
		}

		.roadmap li {
			display: block;
		}

		.roadmap strong {
			display: block;
			margin-bottom: 0.5rem;
		}

		.bot-input {
			align-items: stretch;
			flex-direction: column;
		}

		.bot-input button {
			width: 100%;
		}
	}
</style>
