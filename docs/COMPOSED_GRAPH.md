# ComposedGraph

ComposedGraph records memory combinations as durable reasoning events.

Most memory systems store facts, entities, or relationships. ComposedGraph stores a
different object: which memories were used together, why they were used, and what
happened afterward.

## Model

`composition_events` stores the reasoning envelope:

- tool and mode, such as `deep_reference` or `bounty`
- query and query hash
- confidence, status, and output preview
- metadata for intent, analyzed memory count, activation expansion, and reasoning preview

`composition_members` stores the participating memories:

- memory id
- role, such as `primary`, `supporting`, `contradicting`, or `superseded`
- rank, trust, relevance score, preview, and metadata

`composition_outcomes` stores later labels:

- `helpful`
- `dead_end`
- `submitted`
- `accepted`
- `rejected`
- `duplicate_risk`
- `needs_poc`
- `bad_severity`
- `user_promoted`
- `user_demoted`
- `closed_by_scope`
- `closed_by_duplicate`
- `closed_by_false_assumption`
- `closed_by_user`
- `expired_lane`

Member memory ids are intentionally historical references, not foreign keys into
`knowledge_nodes`. Purging or superseding a memory should not erase the fact that
it once participated in a reasoning path.

## MCP Tool

Use `composed_graph` for read/write access to the composition ledger.

```json
{ "action": "recent", "limit": 10 }
```

```json
{ "action": "get", "event_id": "<composition-event-id>" }
```

```json
{ "action": "memory", "memory_id": "<memory-id>", "limit": 10 }
```

```json
{ "action": "neighbors", "memory_id": "<memory-id>", "limit": 10 }
```

```json
{ "action": "never_composed", "tags": ["project:vestige"], "limit": 10 }
```

```json
{
  "action": "label",
  "event_id": "<composition-event-id>",
  "outcome_type": "helpful",
  "notes": "This combination led to the accepted fix."
}
```

## Never-Composed Frontier

`never_composed` returns pairs that have not yet appeared together in a
composition event.

The ranking is intentionally not just shared-tag matching. It combines:

- exact shared tags
- shared meaningful content terms
- boundary tags such as `boundary-*`, `oracle`, `queue`, `settlement`, `upgrade`,
  `pause`, `accounting`, or `scope`
- node-type diversity
- FSRS retention strength
- composition novelty, so memories that have not already been heavily composed
  still get surfaced
- prior composition outcomes from either member, so previously accepted,
  duplicate-risk, or dead-end lanes shape the frontier without hiding it

Each candidate includes:

- `score`
- `noveltyScore`
- `bridgeScore`
- `trustScore`
- `outcomeScoreAdjustment`
- `sharedTags`
- `boundaryTags`
- `sharedTerms`
- `priorOutcomes`
- `outcomeSignal`, such as `clean`, `prior_success`, `prior_duplicate_risk`,
  `prior_closed_door`, or `mixed_prior_outcomes`
- node types
- previews
- a short reason
- a `compositionQuestion` that an agent can answer before taking action

The output is a frontier queue, not a finding. A never-composed pair means
"worth investigating," not "true," "novel," or "reportable."
Prior outcomes are also guardrails, not verdicts: a duplicate-risk signal should
make the agent check duplicate families first, while a success signal should make
it inspect why the older composition worked.

Closed-door labels should be specific when possible. Prefer `closed_by_scope`,
`closed_by_duplicate`, `closed_by_false_assumption`, `closed_by_user`, or
`expired_lane` over a generic `dead_end` when the reason is known.

## Bounty / Research Mode

`bounty_mode` is a higher-level read shape for investigative workflows. It returns:

- recent already-composed lanes
- never-composed lanes
- closed doors
- duplicate-risk lanes
- lanes that need proof-of-concept work
- top weird combinations

This is useful for security research, bug triage, architecture work, and product
strategy because failed or duplicate compositions are preserved instead of being
rediscovered repeatedly.

## Deep Reference Integration

`deep_reference` persists composition events automatically when it has evidence
members. Empty evidence does not create a ledger event.

The response includes:

- `composition_event_id` when persisted
- `compositionWriteStatus`, usually `persisted` or `skipped_empty`

## Design Direction

The next useful upgrades are:

- triple or n-ary candidate mining, not only pairs
- structural-fit scoring for analogies, separate from surface similarity
- trust-zone scoring so a composition is limited by its weakest provenance
- temporal replay: "what combinations were available when this decision was made?"
- evaluation tasks where success requires combining memories that were never
  previously co-composed
