# Postdict — memory with hindsight

> Every other AI memory finds what your bug **looks like**.
> This one finds what **caused** it — even when the cause was days ago
> and looks nothing like the crash.

When your agent hits a failure **today**, Postdict reaches **backward in time**
and surfaces the quiet earlier change that caused it — the root cause a vector /
semantic search **structurally cannot** find, because it isn't *similar* to the
error, only *causally upstream* of it.

It's a faithful port of a 2024 *Nature* neuroscience result: the brain links a
later salient event to an earlier quiet memory **backward in time** ("fear links
retrospectively, not prospectively"). Root cause is always in the past — so the
backward-only direction isn't a metaphor, it's the correct behavior.

---

## ▶️ Run the demo (30 seconds, one command)

```sh
# from the repo root
cargo build -p vestige-mcp --bin vestige --release   # first time only (~1 min)
./demo/postdict-demo.sh
```

That's it. It uses a **fresh throwaway database** (a temp dir, deleted on exit) —
it touches nothing else on your machine. No API keys, no network, fully local.

**Pacing:** the script pauses ~1.4s between beats for a clean screen-recording.
- `PAUSE=0 ./demo/postdict-demo.sh` — instant (no pauses)
- `PAUSE=2 ./demo/postdict-demo.sh` — slower, more dramatic

---

## What you'll see

The demo plants three memories into an empty store, then asks one question:

| When | Memory | Note |
|---|---|---|
| **3 days ago** | `Set API_TIMEOUT=2 in the deploy env to speed up cold starts` | the quiet cause. boring. forgotten. |
| **20 days ago** | `A 500 Internal Server Error happened in the billing service` | a lookalike — *resembles* today's crash |
| **today** 💥 | `Service crashed: 500 Internal Server Error on the auth endpoint` | the failure |

Then it runs the same question through both engines, side by side:

```
── 1. SIMILARITY SEARCH · keyword (BM25) ──
   1. A 500 Internal Server Error happened in the billing service   ← top match
   → ranked by RESEMBLANCE. its top hit is a lookalike, not the cause.

── 2. POSTDICT (reach backward for the CAUSE) ──
   #1 Set API_TIMEOUT=2 in the deploy env to speed up cold starts
      ↩ reached back 3.0 days before the failure
      🔗 causal join: api_timeout
      ✅ promoted — it will resurface next time
```

**Similarity search confidently returns the lookalike.** It's wrong.
**Postdict reaches back 3 days and finds the real cause** — by the shared
`API_TIMEOUT` entity, backward in time. Then it promotes that memory so it
stops decaying and surfaces next time.

> The label says exactly which engine ran (`keyword (BM25)` here; it becomes
> `semantic (vector + BM25 hybrid)` once embeddings are generated). No
> sleight of hand — it's the real search every other memory tool does.

---

## Try your own scenario (the "it's not staged" proof)

Nothing here is hardcoded. Build any history and ask:

```sh
DB=$(mktemp -d)/db
BIN=./target/release/vestige

# plant a quiet cause N days ago (--ago-days backdates it)
$BIN --data-dir "$DB" ingest "Disabled the checkout cache while debugging" \
    --tags checkout --node-type decision --ago-days 4

# record a failure today that shares an entity (here: 'checkout')
$BIN --data-dir "$DB" ingest "Checkout latency spiked to 9s after deploy" \
    --tags checkout,latency,regression --node-type event

# reach backward for the cause, with the similarity contrast
$BIN --data-dir "$DB" backfill --contrast
```

The cause and the failure share the `checkout` entity but are **not textually
similar** — so semantic search misses it and Postdict finds it.

`vestige backfill --help` for all flags (`--manual`, `--lookback-days`,
`--failure-id`, `--no-promote`).

---

## The honest boundary (read this — it's the point)

- **If the upstream change was never recorded, nothing can reach it.** Postdict
  reaches back through *memory*, not magic. No memory of the cause → no backfill.
- **It links by shared entities** (same file / env var / service / symbol),
  backward in time — *not* by semantic similarity. That's deliberate: similarity
  is exactly the blind spot every other memory already has.
- **It won't invent a cause.** No shared entity between the failure and an
  earlier memory → no link. It would rather say nothing than fabricate.
- The "promote" step boosts the cause's retention so it resurfaces; it never
  deletes or rewrites anything (bi-temporal — old memories stay queryable).

---

## How it works (for the skeptics)

1. **Trigger.** A memory lands that reads like a failure (high surprise +
   failure markers like `error`/`crash`/`500`), or you mark one manually.
2. **Backward reach.** Postdict scans memories *older* than the failure that
   share an entity with it (the causal join), within a lookback window.
3. **Rank by cause, not resemblance.** Candidates are scored by shared-entity
   strength and *dissimilarity* — the less a candidate resembles the failure,
   the more valuable it is, because that's precisely what a vector search can't
   surface.
4. **Promote.** The surfaced cause's FSRS retention is boosted so it stops
   decaying and is there next time.

The mechanism, tests, and the *Nature* citation live in
[`crates/vestige-core/src/advanced/retroactive_backfill.rs`](../crates/vestige-core/src/advanced/retroactive_backfill.rs).
The field itself admits this is unsolved: causal + temporal retrieval is
"largely unexplored" (mem0, *State of AI Agent Memory 2026*), and frontier
models fail at cloud root-cause analysis ([arXiv:2602.09937](https://arxiv.org/abs/2602.09937)).
This is the first memory that does it.

---

## Recording the demo (for a clean clip)

1. Make your terminal large, dark theme, ~16pt mono font. Clear scrollback.
2. macOS: `Cmd-Shift-5` → record a tight region around the terminal (or the
   whole window). QuickTime works too.
3. Run `./demo/postdict-demo.sh` (default pacing) — or `PAUSE=2` for a slower,
   more cinematic take.
4. The single "hold here" frame: when **POSTDICT** resolves the `#1` cause with
   `↩ reached back 3.0 days` — that's the money shot. Let it sit.
5. Trim to ~30s. Muted + captions plays best in feeds.

**Caption / hook (ready to paste):**

> Your crash is today. The cause was an env-var edit 3 days ago — it looks
> *nothing* like the error, so vector search will never surface it. Same query,
> split screen: similarity search returns the lookalike, Postdict reaches back
> and finds the cause. Seed's in the repo — run it yourself.

**Pinned honest-boundary reply:** *"Where it doesn't work: if the upstream change
was never recorded, nothing can reach it. Everything in the clip is in the
seeded repo."*

---

*Local-first. No API keys. No data leaves your machine.*
