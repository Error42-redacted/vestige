# Postdict — the 60-second funding demo

**Audience:** investors. **Goal:** they see a category, a moat, and a market — not a feature.
**The thesis, in four words:** *relevance does not equal resemblance.*

Format: live terminal, one take, you talking over it. ~60s. Punchy. No slides.
**Rule:** the monologue earns ~20 seconds. The rest is the terminal *doing the impossible* on screen.

---

## THE SCRIPT (what's on screen · what you say)

### [0:00–0:15] — THE FLAWED AXIOM (your cold open, verbatim)

**On screen:** a clean dark terminal, one line:
`relevance ≠ resemblance`

**You say:**
> "Every major AI memory framework on Earth — every VC-backed startup, every
> native platform layer — is built on one flawed assumption: that **relevance
> equals resemblance.** They turn your text into vector embeddings, and when
> something breaks, they search for memories that *look similar* to the problem.
>
> Here's what blows that foundation to pieces: **a root cause never looks like
> the bug it creates.** So the entire industry is searching in the one place the
> answer can never be."

*(Stop. Let it sit one beat. "Relevance equals resemblance" is the line they repeat to their partners.)*

---

### [0:15–0:28] — MAKE IT CONCRETE (type it live)

**On screen:**
```
$ vestige ingest "Set API_TIMEOUT=2 in the deploy env" --ago-days 3     # the quiet cause
$ vestige ingest "500 error in the billing service" --ago-days 20       # an old lookalike
$ vestige ingest "Service crashed: 500 on the auth endpoint"            # today's crash
```

**You say:**
> "Watch. A one-line config change three days ago — forgotten. An old, unrelated
> 500 error weeks back. And today, the auth service crashes. Now ask: which past
> memory *caused* today's crash? A vector database ranks by resemblance — so to
> it, today's crash looks most like that old billing 500. **The thing that looks
> similar is never the thing that caused it.**"

---

### [0:28–0:45] — THE PROOF (the money shot · slow down here)

**On screen:** `$ vestige backfill --contrast` →
```
── 1. SIMILARITY SEARCH · keyword (BM25) ──
   1. 500 error in the billing service          ← top match   (WRONG)
   → ranked by RESEMBLANCE. its top hit is a lookalike, not the cause.

── 2. POSTDICT (reach backward for the CAUSE) ──
   #1 Set API_TIMEOUT=2 in the deploy env
      ↩ reached back 3.0 days before the failure
      🔗 causal join: api_timeout                            (RIGHT)
```

**You say (let the `↩ reached back 3.0 days` line hold in silence for a full beat):**
> "Same database. Same query. Similarity search returns the lookalike — confident,
> and wrong. Postdict reaches **backward three days** and finds the actual cause.
> Not because it's similar — because it's **causally upstream.** This is memory
> with hindsight: the 'ohhh, *that's* why' moment, automatic."

---

### [0:45–0:55] — THE MOAT (kill the "can't they just add this?" objection)

**You say:**
> "Two reasons this is defensible. One: it's a faithful port of a 2024 *Nature*
> result — the brain reaches *backward* in time to find causes, and it's
> backward-*only*, which is exactly right, because a root cause is always in the
> past. We didn't invent this; we ported the algorithm evolution already
> perfected. Two: the incumbents can't bolt this on. Their entire architecture
> **is** the flawed axiom. To do this you rebuild memory from the cognitive
> science up — which we already did, and it's running locally, today."

---

### [0:55–1:00] — THE MARKET + THE ASK

**On screen:** `the first memory that finds the cause, not the lookalike.`

**You say:**
> "Every AI agent that writes code, runs infrastructure, or touches production
> hits root causes it can't explain. That's the entire agentic market, and it's
> on fire. We're not a better memory — we're the first memory that **reasons
> backward.** Local-first, reproducible, running now. We're raising [X] to make
> every agent debug like a senior engineer. The seed's in the repo — run it
> yourself."

---

## WHY THIS OPENING IS STRONGER (and how to deliver it)

Your framing beats "category error" because it names the **mechanism** of the
error, not just that one exists:

- **"Relevance equals resemblance"** is a *diagnosis* — it tells the investor
  precisely what's broken (the axiom) in four words. "Category error" only says
  *that* something's broken.
- **The one-two punch:** state the flawed axiom → detonate it with the fact
  ("a root cause never looks like the bug it creates"). The investor *feels* the
  foundation crack. That's the moment they lean in.

**Delivery rules:**
1. **The monologue is 20 seconds, max.** Investors fund what they *see* work, not
   what they hear claimed. Get to the terminal fast; let the contrast carry the
   weight.
2. **Memorize two sentences.** The axiom: *"They believe relevance equals
   resemblance."* The detonation: *"A root cause never looks like the bug it
   creates."* Everything else can be loose.
3. **Silence is the tool at 0:28–0:45.** When `↩ reached back 3.0 days` hits the
   screen, say nothing for a full second. The image does the selling.
4. **The moat answer is non-optional.** Every investor thinks "can't Mem0 add
   this?" Answer it *before* they ask — their architecture is the axiom. That's
   what converts "neat" into "fundable."
5. **End on the market, not the demo.** "Every agent that touches production" is
   the TAM. The demo earns the right to say it; don't bury it under the feature.

## THE THREE LINES THAT DO THE WORK
- **The axiom (the hook):** "Every AI memory framework believes relevance equals resemblance."
- **The detonation (the thesis):** "A root cause never looks like the bug it creates."
- **The category (the close):** "We're not a better memory. We're the first memory that reasons backward."
