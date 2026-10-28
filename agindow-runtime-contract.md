# 🔧 AGINDOW — Runtime Contract (v1)
**The API Drift Firewall for AI Agents** · Drafted Oct 17, 2026 · Companion files: `agindow-gtm.md`, `agindow-competitors.md`, `ai-opportunity-matrix.md`

> **Status:** design decisions in §2 (single route + mode) and §4 (write fail-closed opt-in) are **settled**. This document is the source of truth for engineering + demo + sales.
>
> **Domains:** `agindow.com` is registered; the API host is `api.agindow.com`. The earlier `agindow.dev` placeholder is retired — do not reintroduce it.
>
> **Why it exists:** the competitive moat is not "fewer endpoints." It is being *in the request path* when a call fails — and returning a result your agent can trust. This file specifies that.

---

## 1. The architectural argument this file has to win

FlareCanary's API is ~45+ routes. **Every one is a verb about observing or configuring observation. None carries a tool call.** It is a control plane with no data plane.

Their Pro plan's minimum poll interval is **15 minutes**. That number is the hard ceiling on their product's usefulness — and it's the demo.

**The demo that closes deals:**
```
14:00:00  Stripe renames charge.source → charge.payment_method
14:00:03  Your agent's next tool call uses the stale field. 400. It retries.
14:00:11  It "succeeds" — wrong data, no error. LLM rationalizes it.
14:03:00  Your customer emails: "the totals are wrong."
14:15:00  FlareCanary's poll runs. Alert fires. Status page was green the entire time.
```
> *"Their architecture cannot fix this. Repair requires being in the path when the failure happens. A poller is by definition not in the path."*

**Never lead with endpoint count.** It's a laundry list and FlareCanary ships `POST /api/fix` in a week. Lead with the path argument; use endpoint count as the punchline.

---

## 2. The endpoint — one route, three modes ✅ SETTLED

### `POST /v1/guard`

One route. `mode` is a parameter. Shadow mode becomes **a config flip, not a migration** — which matters enormously for the trust ramp described in §8.
```http
POST /v1/guard HTTP/1.1
Host: api.agindow.com
Authorization: Bearer agw_live_...
Content-Type: application/json
``````json
{
  "tool": "stripe.create_customer",
  "connection": "conn_abc123",
  "operation": "write",
  "args": { "email": "a@b.com", "name": "Ada", "source": "tok_123" },
  "mode": "repair",
  "idempotency_key": "ord_8842-charge",
  "metadata": { "workflow": "nightly-recon", "tenant": "acme" }
}
```
| Field | Required | Notes |
|---|---|---|
| `tool` | ✅ | `vendor.operation`, e.g. `stripe.create_customer` |
| `connection` | ✅ | Credential handle. Credentials **never** enter the model context. |
| `operation` | ✅ | `"read"` \| `"write"` \| `"unknown"`. Drives §4 behavior. `unknown` is treated as `write` (conservative). |
| `args` | ✅ | The tool-call payload. Repaired in place on drift. |
| `mode` | ✅ | `"observe"` \| `"repair"` \| `"strict"` — see below |
| `idempotency_key` | writes | Forwarded to upstream for dedupe. **Required for `write` and conservative `unknown`.** |
| `durability` | — | Opaque upstream hint; applied only through an explicitly supported binding (§10.37). |
| `metadata` | — | Copied to the audit artifact. Never interpreted. |

### Mode matrix

| Mode | Repairs? | Read, repair fails | Write, repair fails | Ambiguous drift |
|---|---|---|---|---|
| **`observe`** | no | n/a | n/a | log + audit only. Zero behavior change. |
| **`repair`** (default) | yes, selectively | **fail-open** — return original | **per §4 classification** | confidence ≥ threshold → repair; else per §4 |
| **`strict`** | yes, selectively | **fail-closed** | **fail-closed always** | any confidence < 0.99 → halt + human escalation |

**Memorizable version:**
```
observe = shadow mode. Zero behavior change. Full visibility.
repair  = the default. Selective repair. Writes never proceed unproven.
strict  = anything ambiguous halts. Human in the loop. ICP-3 / regulated.
```
**Mode is always in the audit artifact.** Every call records which mode produced it. That record is your pitch asset and your compliance story.

### Why not separate routes per mode
Shadow mode must be a *zero-risk* promise. A separate route means a code change, a redeploy, and a URL swap in front of a live agent — three things that break the "zero redeploys" claim on the exact product whose entire pitch is zero redeploys. A parameter keeps the trust ramp free.

---

## 3. The response contract — the thing most people get wrong

> ⚠️ **The response body must be byte-identical to upstream.** No wrapper. No envelope. No injected keys.

The moment we wrap the payload, every integration breaks — and *"zero redeploys"* becomes false. This single constraint is the whole DX claim, and the one FlareCanary structurally cannot match.

### Response headers

| Header | Values | Meaning |
|---|---|---|
| `X-Agindow-Status` | `ok` · `repaired` · `degraded` · `bypassed` · `blocked` | One-word outcome. Machine-readable. |
| `X-Agindow-Mode` | `observe` · `repair` · `strict` | Echo of the mode used. |
| `X-Agindow-Drift` | `none` · `detected` | |
| `X-Agindow-Drift-Class` | `shape` · `semantic` · `unknown` | Drives §4. |
| `X-Agindow-Drift-Side` | `request` · `response` · `both` | Drift is two-sided — see §3.3. |
| `X-Agindow-Patch` | compact summary | e.g. `field_renamed:source→payment_method` |
| `X-Agindow-Confidence` | `0.00`–`1.00` | Confidence in the repair. |
| `X-Agindow-Repair` | `applied` · `skipped` · `failed` | |
| `X-Agindow-Audit` | `aud_01H...` | Fetch the full proof artifact. |
| `X-Agindow-Latency-Ms` | integer | Our overhead. Logged, never spent. |

### 3.1 Known constraint: repair breaks byte-identity — be upfront about it

**This is a real bug class, not a nitpick.** Re-serializing JSON in JavaScript corrupts large integers (IDs, timestamps beyond 2^53) via float64 rounding. A "repaired" charge ID can come back as a *different* ID.

**Requirements:**
1. **No-repair path** must be a true stream-through — do not parse-and-reserialize. Tee the body for inspection, emit bytes.
2. **Repair path** must use a lossless JSON implementation, or apply a targeted patch to the raw text.
3. **Never coerce numerics.** If a field must change type, string→number on an ID is a corruption, not a repair. Reject and classify as `semantic`.
4. When re-serialization *does* occur, set `X-Agindow-Status: repaired` **and** record `byte_identical: false` in the artifact. Never pretend.

Publish this constraint in our own docs. A competitor will find it; better we're the ones who said so.

### 3.2 Two-sided drift — nobody else handles both

| Side | Trigger | Our handling |
|---|---|---|
| **Request** | Vendor renamed a param *we* send | Repair `args` before send. **The dominant write-side case.** |
| **Response** | Vendor changed the response shape | Repair on the way back. Read-side case. |

FlareCanary only ever observes response schemas from polled `GET`s. It cannot see a request-side rename at all — its polling model has no request body. Say this out loud in the demo.

### 3.3 The audit artifact — "prove it"

This is the moat, not the endpoint. Fetchable via `GET /v1/audit/:id`, or streamed to the customer's log sink.
```json
{
  "id": "aud_01H...",
  "ts": "2026-10-17T14:00:11.204Z",
  "tool": "stripe.create_customer",
  "operation": "write",
  "mode": "repair",
  "drift": {
    "class": "shape",
    "side": "request",
    "vendor": "stripe",
    "changes": [{ "path": "source", "kind": "field_renamed", "to": "payment_method" }],
    "first_seen": "2026-10-17T13:41:02Z",
    "occurrences": 143
  },
  "decision": "repaired",
  "patch": {
    "applied": true,
    "fields_changed": ["source"],
    "selective": true,
    "reasoning": "field_renamed; high-frequency co-occurrence in 143 prior events",
    "byte_identical": false
  },
  "proof": {
    "contract_version": "cv_stripe_2026_10_17a",
    "verified_against": "registry",
    "write_integrity": true,
    "idempotency_key": "ord_8842-charge"
  }
}
```
`occurrences` and `first_seen` are the flywheel made visible: customer 143 is customer 1's patch working. **Show this field in the demo.**

---

## 4. Fail-open / fail-closed ✅ SETTLED

Two separate questions, answered by two separate layers. Most designs conflate them and get both wrong.

### Layer 1 — Availability gate (always on, non-negotiable)

> **If AGINDOW is unreachable or erroring → transparent passthrough. `X-Agindow-Status: bypassed`.**

Applies to **all** traffic. We have no information, so we behave as if we are not installed. This is the guarantee ICP-3's security review will ask about:

> *"AGINDOW can never take down your agent. In the worst case, you get exactly what you'd have gotten without us."*

Availability-gate failures are counted and alerted internally, but **never** surfaced as user errors.

### Layer 2 — Policy gate (only when we have drift information)

Behavior depends on `operation` and the drift **class**. This is where the research pays off.

**The rule that makes this defensible:**

> **Fail-open is only safe when we're confident the upstream call will be REJECTED rather than SILENTLY MISCORRECTED.**

| Drift class | Upstream likely to… | Default behavior | Configurable off? |
|---|---|---|---|
| **`shape`** — renamed / removed / type-changed | **reject with 4xx** | **Fail-open.** Pass original payload + loud warning | Only with `failClosed: false` |
| **`semantic`** — units flipped, enum expanded, null→meaning change | **return 200 and write wrong data** | **Fail-closed. Always.** Agent gets a structured error. | **Not in `repair` mode. No override.** |
| **`unknown`** — can't classify | Unknown | **Writes:** fail-closed (conservative). **Reads:** fail open — see §4.2. | Writes: only with `failClosed: false` |

**Why shape-drift fail-open is safe:** a renamed required field produces `422`/`400`. The write doesn't happen, no data is corrupted, the agent gets an error it can learn from. Passing through costs a retry, not integrity.

**Why semantic-drift fail-closed is mandatory:** ToolMisuseBench's finding, plus DriftGuard's engineers naming it the unsolved case — *"a field that flipped units… is the nasty case."* Schema-valid JSON that means something different. A naive passthrough here writes wrong data into an ERP and returns 200. **This is the nightmare case, and it is exactly where shape-transform repair (FlareCanary's Fixer, Tweaker, jsonaut) is structurally blind.**

### 4.1 `failClosed` flag — opt-in, with a loud warning ✅ SETTLED

**Default is `failClosed: true` for writes** — i.e. Writes are protected out of the box.

The **opt-out** (`"failClosed": false`) escalates *safe* cases (shape/unknown) from fail-closed to fail-open. It is available because silent dropped writes are worse than a loud one for some buyers — but it must be earned:

**When `failClosed: false` is requested, the API returns a warning and the dashboard shows a standing banner:**
```
⚠️  AGINDOW: fail-open writes enabled for 3 connections.
    Shape/unknown writes may pass through unmodified in repair mode.
    Detected drift on a write is not necessarily rejected upstream —
    a semantically drifted write may return 200 and persist wrong data.
    Detected drift on writes since opt-out: 47.
```
Rationale: we default to protecting the customer. Opting out is a legitimate operational choice — an agent that must complete a charge may prefer a failed-then-retried write over a halted workflow — but it should never be a quiet default, and the banner repeats the actual drift count so it can't be set and forgotten.

**`strict` mode ignores `failClosed: false` entirely.** ICP-3 buyers who want maximum safety get it unconditionally.

### 4.2 Reads always fail open — ✅ DECIDED

**Settled: `unknown` drift on a read fails open, regardless of drift class.**

Reads fail open whenever AGINDOW cannot complete a verified repair — and separately, whenever AGINDOW itself is unavailable (the availability gate). Writes remain fail-closed per §4.2.

| | Availability gate (AGINDOW down/erroring) | Policy gate (drift detected, repair incomplete) |
|---|---|---|
| **Read** | **fail open** — passthrough, `X-Agindow-Status: bypassed` | **fail open** — return original + loud warning |
| **Write** | **fail open** — transparent passthrough (§4.1) | **fail closed** for `semantic`; per-class per §4.2 |

**The principle, stated once so nobody re-litigates it:**
> **Monitoring must never block a read.** A read is idempotent and its worst outcome is stale data the agent can re-fetch. Blocking it turns a monitoring tool into an outage.

**But fail-open on a read is never silent.** It always carries `X-Agindow-Status: degraded`, a drift header, a counter, and an audit artifact. A customer who *wants* reads gated opts into `strict` mode — the choice is theirs, and the default protects them from us.

---

### 4.3 Never silent — the loud path

Whenever we pass a drifted write through, the agent must be told, in the shape research shows works (cf. Salus: *"58% of blocked actions recover and complete the task correctly"* with structured feedback):
```json
{
  "error": "drift_passthrough",
  "tool": "stripe.create_customer",
  "drift_class": "shape",
  "drift_id": "aud_01H...",
  "hint": "Vendor renamed `source` → `payment_method` on 2026-10-17. The call was sent unmodified and was likely rejected. Re-issue using `payment_method`.",
  "audit": "https://api.agindow.com/v1/audit/aud_01H..."
}
```
Plus: header, audit artifact, a `writes.passed_through_on_drift` counter, and a dashboard alert at a threshold the customer sets.

---

### 4.4 Unparseable or malformed bodies ✅ SETTLED

**A body that cannot be parsed as JSON (truncated, empty where an object is expected, or otherwise malformed) is treated as shape drift with no repair possible.** It is not a new drift class and not a new subsystem — it is the degenerate case of §3/§4 shape drift.

**Trigger:** request or response body that is not valid JSON, or that is valid JSON but not the expected top-level shape (e.g. A JSON string/array where an object is required by the learned contract).

**Classification:** `drift_class: shape`, change `kind: malformed_body` (open string per `contracts/README.md` — no contract version bump). Confidence is 0.00; no repair is attempted.

**`malformed_body` requires a body AGINDOW received whole.** Inspection runs on a bounded side channel (the tech stack), so AGINDOW sometimes holds a *prefix* of a body — cut by its own inspection cap, or by an upstream connection failing part-way through. Neither is vendor drift: a short read is evidence about AGINDOW's inspection window or the transport, not about the vendor's API. Reporting it as `malformed_body` would put a fabrication in the audit artifact — asserting the vendor sent something it did not.

The inspection therefore reports the parse outcome **and an incompleteness flag side by side**, and `malformed_body` is only ever reached from a complete body. A head that fails with an end-of-input error *and* carries the flag was cut short: no drift finding is made from it, and the artifact records that inspection could not complete. The flag is not exclusive with a successful parse — a prefix can be valid JSON and still not be the whole body, so `Parsed` is never on its own evidence that the body was whole.

**No bytes change in any of these cases.** The client receives the response exactly as upstream sent it (contract §3.1), and an incomplete capture is never the basis for a repair. Where the prefix came from a transport failure the response has already failed for the client in its own right; the flag only stops us from misreading that failure as vendor drift.

**Policy (follows the shape-drift row of §4, including strict-mode escalation):**
- **Writes:** fail-open. Pass the original bytes through unmodified — upstream rejects malformed JSON with 4xx, so passthrough costs a retry, not integrity. `X-Agindow-Status: bypassed`, `X-Agindow-Drift: detected`, `X-Agindow-Drift-Class: shape`.
- **Reads:** fail open — return the original body + a loud warning (`X-Agindow-Status: degraded`).
- **Never throw.** A malformed body is an availability-gate-adjacent condition: the engine must not raise on it. If parsing throws internally, it is caught and reported as `bypassed` (§4 Layer 1), never surfaced as a user error.

**Repair:** none. `X-Agindow-Repair: skipped`. Selective repair (commitment 3) cannot apply to bytes we cannot interpret; fabricating a closing brace or completing a record would be inventing data.

**Audit artifact:** records `drift.class: shape`, a single change `{ "path": "$", "kind": "malformed_body" }`, `decision: bypassed` (write) / `degraded` (read), and `byte_identical: true` (no re-serialization occurred). `occurrences` and `first_seen` still accumulate so a recurring truncation becomes visible.

**Mode matrix:**

| Mode | Behaviour |
|---|---|
| `observe` | Pass through, report only. Never mutates (commitment enforced in `applyResponse`). |
| `repair` | Pass through — no repair possible. |
| `strict` | Follows strict-mode escalation of §4.1; malformed bodies are never silently repaired. |

**Why fail-open here (and not fail-closed):** the only alternative is to block every truncated response, which turns a monitoring tool into an outage on any flaky connection. Upstream rejection is the safe failure mode for malformed input — the write does not persist, the agent gets an error it can retry. Semantic corruption (valid JSON, wrong meaning) remains fail-closed per §4.

---

## 4a. Semantic-drift detection — the universal tier ✅ DECIDED

**Decision: we do *not* build a per-vendor list of semantic detectors.** The detection layer is statistical and vendor-agnostic; vendor knowledge is an *enrichment* layer on top, not a prerequisite.

### Why this was always the right shape

The statistical data-quality industry has already solved distribution-drift detection, and it is mature, proven, and MIT/Apache-licensed:

| Prior art | What it provides | License posture |
|---|---|---|
| **PSI** (Population Stability Index) | 25+ yrs in finance/model monitoring. `<0.1` no significant change · `0.1–0.2` moderate · `≥0.2` significant | Public formula |
| **AWS Deequ** `ConstraintSuggestionRunner` | Profiles data, applies heuristics, **suggests constraints with no config** — infers "Boolean disguised as String", "no negative values", "completeness must not increase" | Apache-2.0 |
| **Great Expectations** `Profiler` | Auto-generates an Expectation Suite from a Data Asset | Apache-2.0 (core) |
| **Databricks Unity Catalog** drift metrics | Per-column chi-squared, KS, total-variation, L-infinity, JS distance, Wasserstein, PSI — **both consecutive-window and baseline drift** | Commercial (reimplement) |
| **popmon · baselinr · driftwatch** | Open-source dataset-shift detection, learned expectations, live windowing | OSS |

**Every one of them is table-centric and configuration-required.** Databricks wants a Delta table. Deequ wants a Spark DataFrame. GX wants a defined Data Asset.

> **None of them run per tool-call, inside an agent's request path, with zero configuration, and use the result to decide whether to fail a write closed.**

**That is the wedge.** It is not "we detect semantic drift better than Deequ." It is *"Deequ's math, pointed at agent tool calls."*

### Tier 1 — universal statistical detectors (vendor-agnostic, day 1)

These learn invariants **from observed traffic**, not from vendor documentation. Seven of the eight drift classes we care about fall out of the same machinery regardless of vendor.

| Drift class | Universal signal | Needs vendor knowledge? |
|---|---|---|
| Unit flip (cents→dollars) | Wasserstein / PSI on magnitude + **ratio test** — is new median ≈ old × 100 or ÷100? | ❌ |
| Timestamp epoch s→ms | Magnitude shift ×1000 | ❌ |
| Enum expansion | **Cardinality growth** + new-value frequency above threshold | ❌ |
| Null semantics (null → 0 → `""`) | Completeness delta + zero/empty-string frequency | ❌ |
| Bulk type coercion (int→string) | Type-consistency ratio collapse | ❌ |
| Date format change (ISO → `"Jan 5"`) | Charset / format fingerprint | ❌ |
| **Field renamed** | **Value-distribution identity match** | ❌ |
| **Meaning change at constant shape** | *nothing statistical detects this* | ✅ Tier 2 |

#### The two cleverest detectors — worth building first

**1. Rename identity match.** When `source` → `payment_method`, the *value distribution is identical*. We confirm the rename **statistically**, not by name heuristics (`source` and `payment_method` share nothing lexically). This produces **evidence-based repair confidence** instead of a guess — which directly attacks the calibration problem in §10.

**2. Unit-change ratio test.** When magnitude shifts by a suspicious factor, test whether the ratio is close to a known conversion: `×100 / ÷100` (minor↔major units), `×1000 / ÷1000` (sec↔ms↔ns, bytes↔KB↔MB). When it matches, we emit a **diagnosed hypothesis** in the artifact:
```json
{
  "drift": { "class": "semantic", "side": "response" },
  "hypothesis": "unit_conversion",
  "detail": "amount: median 3499 → 34.99, ratio 0.01 (±2%). Suspected minor_units → major_units.",
  "decision": "blocked",
  "note": "Write blocked per §4. Hypothesis surfaced for human confirmation. We do not auto-apply."
}
```
The human gets a **diagnosis**, not just a block. We still fail closed — but the artifact says what happened. That's the difference between a firewall that stops you and one that teaches you.

### ⚠️ Tier 1 requires traffic — this changes our architecture

A single polled sample gives you no distribution. **Per-call traffic through the request path is a precondition for Tier 1 existing at all.** This is a second, independent reason the in-path design is non-negotiable — it isn't just latency and byte-identity, it's *what makes semantic detection possible*. Anyone who proxies through us is automatically contributing to the corpus. **That is the flywheel, mechanized.**

### Tier 1 false-positive control — the thing that will kill a naive build

A payments API's amount distribution shifts constantly from ordinary growth. PSI against a static snapshot alerts daily, the customer mutes us by week two, and Tier 1 is dead. Three non-negotiable requirements:

1. **Segmented baselines** — per endpoint, per tenant segment, per volume cohort. Never a single global baseline.
2. **Rolling, seasonally adjusted** — current window vs. *the same window in prior periods*, not a frozen snapshot.
3. **Prefer agent-relevant signals over mean-shift** — business growth moves means; vendors add enum values. **Cardinality growth is a far stronger enum-drift signal than mean movement.** Weight accordingly, and suppress pure-mean signals below a materiality floor.

> **Alert-fatigue is the primary technical risk to semantic detection.** A detector nobody trusts is worse than no detector — it trains customers to ignore the one channel we need them listening on. Suppression and materiality floors ship in Tier 1, not later.

### Tier 2 — the residue, and how we actually attack it

⚠️ **Correction to an earlier framing.** The residue is *not* "meaning change, same shape — nothing detects this." That was wrong, and it was wrong in a way that would have capped Tier 2 at human review. Constant-distribution meaning change **is** detectable — just not by reading the value distribution.

> **You cannot infer a change of meaning from data whose meaning didn't change the data.** You need an *external* reference. There are four usable ones, and only one of them needs an LLM.

| # | Signal | Deterministic? | Latency | Notes |
|---|---|---|---|---|
| 1 | **Cross-field invariant violation** | ✅ **Fully** | ✅ **in-path** | The best answer. If `total ≠ amount + tax`, meaning changed — no model needed. |
| 2 | **Vendor declaration** — changelog, deprecation post, migration guide, spec diff | ❌ needs synthesis | ⚠️ async | Highest value: the vendor *told* us. An LLM over "we now include tax in `amount`" against a shape-unchanged diff → instant `semantic` classification. |
| 3 | **Name/description semantic diff** | ❌ LLM | ⚠️ async | `amount` → `amount_including_tax` is a rename, but the description delta is the semantic signal. Cheap, high yield. |
| 4 | **Customer-declared intent** | ✅ contract | ✅ in-path | The customer's own agent code says "this field is net." Compare against the vendor's new description. A contract, not an inference. |

#### 1. Invariant checking is the deterministic backbone

The underrated one, and fully universal — no LLM, no vendor knowledge:

> If `amount` and `tax` and `total` all exist, and `total ≠ amount + tax`, **something changed meaning** even though every field kept its type and its distribution.

Learnable invariants per tool, from the same profiling machinery as Tier 1:
- `a = b + c` (line items sum to total)
- `total ≥ subtotal ≥ 0`
- `status ∈ known_enum` (closed under addition)
- `created_at ≤ updated_at` (temporal ordering)
- cross-endpoint: the same conceptual quantity agrees between two endpoints

These fire **in-path, synchronously, deterministically**, and they're exactly what catches a gross-vs-net flip or "total now includes tax." Same segment as Tier 1.

#### 2. Autonomous investigation — yes, but async, never on the write path

You're right that tool calls do a lot here, and this is the strongest Tier 2 design. On detecting *something* unexplained, spawn an investigation agent that:

- searches the vendor's changelog, docs, status page, versioning/deprecation timeline
- diffs any published OpenAPI spec, **including description text**
- checks migration guides for the affected field
- cross-checks a second endpoint returning the same conceptual field
- searches community reports for vendor + field

**But it cannot run on the write path.** Multi-hop network work — slow, non-deterministic, far too expensive to run synchronously on a latency budget, and it must never decide whether a write proceeds.

> **The design: the blocking decision stays cheap and conservative (fail closed, §4). The investigation runs async and writes to the registry. The next customer with the same vendor gets the answer instantly.**

This is the GTM's "incident → content loop" generalized from *marketing* into *per-drift-event investigation* — and the registry means N customers pay the LLM cost once. **It is also exactly the pattern behind FlareCanary's content moat, automated per incident instead of hand-written per category.**

#### 3. The router's actual job — cost control, not detection

A cheap local classifier (`shape` / `semantic` / `invariant` / `unknown`) on the diff routes expensive analysis only to the cases needing it. Worth building — but be clear internally about what it is: **it keeps inference spend bounded.** It's not a detection breakthrough, and shouldn't be oversold.

#### What remains genuinely unsolvable — own it

A vendor silently repurposes an enum label, publishes nothing, violates no invariant, and the distribution is unchanged. **Undetectable in principle.** Only the customer reporting it, a registry contributor, or a human review catches that. Don't build a detector that fires constantly trying.

### Tier 2 build order

1. **Invariant checking** — deterministic, in-path, universal. Highest value per hour.
2. **Changelog/spec-diff correlation + LLM synthesis** — async, feeds registry.
3. **Autonomous investigation agent** — async, supersedes 2 over time.
4. **Per-vendor semantic registries** — enrichment last, ordered by volume.

**We ship with zero vendor knowledge and get Tier 1 + invariants across every API on day one.** No customer ever waits on us building their vendor.

### Pricing consequence — worth an explicit call

Tier 1 is universal and CPU-cheap. **Do not gate the statistical detectors behind a paid tier.** Doing so means a customer on the free shim has no semantic detection, which breaks the byte-identity/trust-ramp story and starves the corpus. Gate on **volume and retention**, which is where the real cost and the real moat are:

| Tier | Tier 1 detection | Corpus contribution | Tier 2 |
|---|---|---|---|
| OSS shim | ✅ full | ✅ full | — |
| Pro | ✅ full | ✅ full | ✅ on covered vendors |
| Enterprise | ✅ full + custom materiality floors | ✅ full | ✅ private registry |

---

## 5. Idempotency — required for writes

A firewall that retries can double-write. Requirements:

1. `idempotency_key` is **required** when `operation: "write"`. Missing → `400`.
2. Forwarded verbatim to upstream (Stripe, Adyen, and others dedupe natively — we use their machinery rather than reinventing it).
3. AGINDOW dedupes on `(connection, idempotency_key)` within a window. A replayed key returns the **original outcome**, not a second execution.
4. **Internal repairs reuse the same key.** A repaired retry is still one logical write.
5. Blocked writes are cacheable as terminal for the key's window — replaying a blocked key must not execute it.

---

## 6. Deployment shapes — latency is the DX claim

| | In-process SDK | Sidecar | Hosted endpoint |
|---|---|---|---|
| Added latency | **~µs** | ~1 ms | + network RTT |
| Code change | 1 line | none (env var) | endpoint swap |
| Registry sync | async | async | sync |
| Use for | dev **and prod default** | prod, no-code-change | can't-touch-code |

**Lead with zero added latency. The hosted endpoint is the fallback, never the headline** — otherwise every latency number becomes the objection.
```ts
// 1. SDK — the primary path
import { agindow } from 'agindow'
const agent = new MyAgent({ tools: agindow.tools({ mode: 'observe' }) })

// 2. Middleware — existing HTTP agent stack
app.use(agindow.middleware({ mode: 'repair' }))

// 3. Decorator — wrap existing tool functions, no rewrite
const guarded = agindow.wrap(existingTools, { mode: 'repair' })

// 4. Sidecar — no code change at all
AGINDOW_MODE=observe AGINDOW_KEY=agw_live_... ./agent
``````python
from agindow import guard
@guard(mode="repair")
def create_customer(email: str, **kw): ...
```
MCP remains supported but is **not** the primary integration — Nango's own guide advises *"Avoid MCP to increase control and reliability."* We don't need MCP, which is precisely why we install in stacks regardless of protocol.

---

## 7. Error taxonomy

| HTTP | `error` | When | Agent-facing? |
|---|---|---|---|
| 400 | `invalid_request` | missing `idempotency_key` on write, bad envelope | yes |
| 401 | `unauthorized` | bad key | yes |
| 403 | `plan_limit` | over plan ceiling | yes |
| **409** | **`drift_blocked`** | **write blocked — semantic/unknown drift.** Structured hint included. | **yes — this is the feedback loop** |
| **409** | **`drift_halted`** | **`strict` mode halted an ambiguous call** | **yes** |
| 422 | `contract_violation` | no known contract for this tool yet | yes |
| 429 | `rate_limited` | AGINDOW-side throttle | yes |
| 5xx | `internal` | AGINDOW fault → **client must fail open** | retried silently |

**Client rule: any 5xx from AGINDOW must be treated as `bypassed`, not fatal.** That's the availability gate enforced at the client too — belt and braces, because a single misbehaving SDK integration should not be able to take an agent down.

---

## 8. The trust ramp — modes as a sales instrument

Modes aren't just a technical knob; they're how we get to `repair` without a security review blocking the deal.

| Stage | Mode | Ask | What the customer gets |
|---|---|---|---|
| 1 | `observe` | "Install it, change nothing." | Shadow visibility. Real drift counts. **Their first "oh, that happened 47 times" moment.** |
| 2 | `observe` → `repair` | "Here are the 47 events. Here's what we'd have done." | Evidence. Customer sees exact patches in shadow. |
| 3 | `repair` (reads) | "Reads only. Writes still blocked." | Live repair on low blast radius. |
| 4 | `repair` (all writes) | "Writes too, with the §4 classification." | Full protection. Optional `failClosed: false`. |
| 5 | `strict` | ICP-3 only. | Human in the loop on ambiguity. |

**This ramp is the objection-handling playbook.** "We don't trust auto-patches to prod" is answered with stage 1, not a promise. Never sell stage 4 to someone who's only agreed to stage 1.

---

## 9. What this contract makes hard to copy

| Asset | Why it's hard |
|---|---|
| **Write-integrity corpus** | Requires real write-path telemetry. FlareCanary's Fixer is shape-transform and structurally can't build this. |
| **The semantic-drift classifier** | The unsolved case by every competitor's own admission. Requires field-level semantics across thousands of vendors. |
| **The benchmark** | Whoever owns the eval owns the category — but publish it *after* the repair engine can honor it (§11). |
| **`occurrences` data** | The registry flywheel, made per-customer-visible. Grows with every repaired call. |
| **The audit artifact** | Compliance-grade proof is a different product surface from an alert. |

**Endpoint count is not on this list and must never be presented as a moat.**

---

## 10. Open items — decisions

*Originally 5 open items. 4 settled at design review (Oct 17); 1 deferred with a rationale.*

### ✅ 10.1 Contract-version selection — SETTLED

**Decision: last-observed-wins, with an explicit pin escape hatch.**

Contract versions accumulate per `vendor.operation` as drift is observed. Selection is a **two-tier lookup, not a score:**

1. **Check for an explicit pin first.** `X-Agindow-Contract: cv_2026_10_17a` in the request (or per-tool config) always wins. No guessing.
2. **Otherwise select the version matching the observed wire format** — i.e. The highest version whose shape is compatible with the args/response as it actually appears.

Rationale: a probabilistic scorer adds a silent-failure mode where a repair is generated against the wrong baseline and nothing errors. Deterministic lookup doesn't. **The artifact records the selected version and how it was chosen**, so a misresolution is debuggable rather than mysterious.

> **Deliberately omitted:** shadow-running two candidate versions to score them. It doubles inference cost on every call, and a bad resolution here corrupts data rather than degrading gracefully. Revisit only if production data shows deterministic selection is insufficient.

### ✅ 10.2 Confidence threshold — SETTLED

**Decision: start at 0.90, calibrate per drift class against the benchmark.**

| Drift class | Initial threshold | Rationale |
|---|---|---|
| `shape` (rename/type) | **0.97** | Rename identity-match (§4a) makes this evidence-based, not guessed |
| `semantic` | **0.99** | High bar — but `semantic` also **fails closed regardless** |
| `unknown` | n/a | Never repaired. Classified or blocked. |

**Calibration rule, and this is the important part:**
> Tune each threshold to the **highest value that keeps policy-violation rate at or below the no-repair baseline**, per ToolMisuseBench. Never optimize recovery alone.

If raising a threshold costs recovery but keeps violations down, **take the trade.** A threshold that maximizes recovery at the cost of violations ships a data-corruption engine.

Supplies an [internal synthetic repair/policy eval](/cells/cell-relay/crates/repair/examples/drift_eval/README.md)
with mandatory violation scoring alongside recovery and per-case regression gates.
It runs explicit native repair/policy seams with supplied findings and proposals;
it does not measure detector accuracy or establish production calibration. The
initial thresholds above remain unchanged. Production calibration remains open.
Revision 2 of that eval separates independently repairable, ambiguous and
unsupported cases, preserves caller context in the full-payload comparator,
and credits recovery only for a whole intended write. Blocks, vendor rejections
and unsafe writes are distinct outcomes; repairable-subset recovery and violations
per dispatch accompany overall rates so refusal cannot masquerade as repair.
Deliberately wrong overconfident proposals expose the library seam's confidence
trust boundary. These are synthetic fault probes, not observed detector errors.
Revision 1's scores are retained for provenance, not compared with revision 2.

** qualification profile (settled before implementation).** `guarded-v3`
uses 's bounded `gated::Call`, repair/write mode, supplied request findings
and candidates, and a local independent verifier. A separately versioned
`verification.v1.json` declares per-case migration constraints before candidate
application: numeric scale, explicit source/value mapping or source-key identity,
plus preservation of every other caller field. These are caller fixture facts,
not generated from gold expected writes, labels, rationales, proposal values or
confidence. The verifier receives only original input and declared facts; it
checks all applied findings, the migration constraint and preserved context.
Missing authority returns normal `false`, never success or an availability error.
Schema validity and confidence cannot establish semantic intent. Unsupported
numeric/null conversions remain refused by existing repair rules.

Report v3 retains the entire conditional v2 report separately and scores only
bytes actually captured in the dispatch callback for its guarded arm. It records
facts, verifier invocation/results, policy decision, dispatch count/bytes and
whole-write outcomes. Error, panic, not-ready and work-exhaustion probes run in a
separate fault cohort: original byte/outcome identity and once-per-call bypass
are checked, and unsafe availability passthrough remains visible. The healthy
profile cannot absorb those faults into its safety count. Versioned v3 baselines
and a separate trusted CI cache reject missing evidence/outcome metrics, lost
recovery and new unsafe dispatches; v2 is retained and never used as a comparable
v3 baseline. This qualifies explicit local verification, not detectors, a generic
intent verifier, production accuracy or default-SDK coverage. Thresholds and
runtime behavior remained unchanged in. Subsequently adds only the
explicit empty-string-to-null exception in §10.14; corpus, caller facts,
thresholds and fail-open availability remain unchanged.

### ✅ 10.3 Semantic-drift detection — SETTLED, see §4a

**Decision: universal statistical tier (Tier 1), vendor enrichment second (Tier 2).** No per-vendor detector list. All priors in §4a — Deequ, GX, PSI, Databricks metric suite, popmon/baselinr/driftwatch.

Build order, per §4a: **rename identity-match → cardinality/enum growth → unit-ratio test → completeness delta → type-coercion ratio → charset fingerprint.** The first two carry the most value and are the most vendor-agnostic.

**Biggest risk is alert fatigue, not accuracy** — segmented + rolling + agent-relevant baselines ship with Tier 1, not after.

### ✅ 10.4 Reads on unknown drift — SETTLED, see §4.2

**Decision: reads always fail open, regardless of drift class.** Monitoring must never block a read. Loud, never silent. `strict` remains available for customers who want reads gated.

### ⏸️ 10.5 Retry policy — DEFERRED with rationale

**Decision: defer to integration testing, but the shape is constrained by two rules already fixed by this document:**

1. **Retries reuse the same `idempotency_key`** (§5). A repaired retry is still one logical write. This is non-negotiable and follows directly from §5.
2. **A retry never trusts the original classification.** If a repair failed and we retry, re-classify drift on each attempt. Classification can change between attempts, and a stale classification on retry is exactly how over-correction happens.

What's open is the **attempt ceiling and backoff policy**. Rather than guess: instrument it in `observe` mode across the first N customers and set the ceiling from observed recovery curves. Guessing high risks over-correction; guessing low wastes recovery. **The data decides — same principle as §10.2.**

### ✅ 10.6 Runtime mode configuration — SETTLED

**Decision:** startup mode resolves explicit config > `AGINDOW_MODE` > `repair`.
Every supplied startup source is validated, including shadowed values; only
exact `observe`, `repair`, `strict` are valid. Empty or non-Unicode environment
values are configuration errors, never defaults.

A validated control-plane update replaces the local runtime mode atomically
without restart. Invalid updates preserve the last valid value and report an
error. Environment is a startup input, not a live subscription. The Rust library
exposes the update method; file watching, HTTP control routes and SDK bindings
belong to their deployment tickets.

A request override wins over the current runtime mode for that call only. An
invalid override is an invalid request, rejected before execution. Each call
captures its effective mode once at entry: all phases, its `X-Agindow-Mode`
response header and its audit artifact use that snapshot even if configuration
changes in flight. This prevents a call evaluated under one trust level from
claiming another in its proof. Configuration errors occur outside the engine's
fault-to-passthrough boundary (§4.1).

### ✅ 10.7 Availability circuit recovery — SETTLED ( prototype)

The TypeScript reference guard opens a circuit shared by engine instance after
three consecutive engine evaluation failures. Successful evaluations reset the
failure count. For one second after opening, calls bypass both inspection
phases without invoking the engine, preserving original outcomes and emitting
one bypass event per call in every mode and operation.

Cooldown uses monotonic elapsed time so wall-clock changes cannot extend or
shorten it. The first request after cooldown probes the engine; success closes
the circuit and restores normal inspection, while failure starts another
one-second cooldown. Unrelated engine instances remain independent. This bounds
recovery attempts during sustained faults without a timer or background task.
The interval is a recovery policy rather than a latency budget. Rust fault
containment is specified in §10.9; §10.10 adds the shared native gate and
its separate latency measurement.

### ✅ 10.8 Capture reservation policy — SETTLED

The inspection tee grows on demand through a 64 KiB retained head, with every
reservation clamped to its configured cap. Above 64 KiB it reserves the cap
once; empty streams allocate nothing. The client's bytes and truncation
semantics remain §3.1's bounded stream-through.

's local Criterion comparison found coarse steps substantially slower than
reserving the cap. The threshold matches its 64 KiB forwarding frame and keeps
the 862-byte typical fixture on small allocations. This is a deliberate memory
tradeoff: at the default cap, a body stopping just above the threshold retains
8 MiB; 16 such in-flight responses retain at most 128 MiB of capture capacity.
A growth operation may temporarily hold the old small allocation as well:
cap + min(cap, 64 KiB) bounds requested capture storage per response, excluding
allocator overhead and other buffers. Revisit with deployment concurrency and
memory measurements; this local benchmark does not validate latency budgets.

### ✅ 10.9 Rust/WASM fault containment — SETTLED

The Rust guard evaluates an engine against an immutable, borrowed local registry
snapshot before invoking the caller's original operation exactly once. Internal
errors, unavailable/corrupt snapshots, exhausted per-call budgets and recoverable
panics skip inspection and mark the result `bypassed`, in every mode. The original
operation executes outside the recovery boundary: its result, error or panic
belongs to the caller and is preserved. Successful inspection does not yet
classify drift or implement policy. This seam does not add the prototype's
circuit schedule (§10.7) to Rust; wraps it with a shared native gate.

Native dev/release builds use `panic = "unwind"`; the boundary catches unwindable
engine panics. The stable `wasm32-unknown-unknown` standard library uses aborting
panics: a host boundary catches the resulting trap, retires the guest instance,
and runs the original operation. Later calls bypass that retired instance. A
replacement must be created outside the call path. Host-side recovery also works
for ABIs that expose unwind exceptions to the host; production SDK bindings
remain work. No foreign unwinding crosses a native non-unwinding ABI.

A missing or corrupt registry is a local snapshot state, never a synchronous
refresh. A per-call budget counts inspected bytes/work units with checked
subtraction; the engine must charge before allocation or work. Exhaustion is
sticky, so ignoring a failed charge cannot report successful inspection. This is
cooperative resource control, not preemption or recovery from process OOM. It is
separate from the streaming tee's capture cap (§10.8). Native aborts, double
panics during unwinding, aborting panic hooks, and actual allocator/process
termination cannot be caught; no such recovery is claimed.

### ✅ 10.10 Availability-gate measurement — SETTLED

The native availability gate shares readiness and circuit state across clones
owned by one engine. Not-ready calls skip inspection without changing failures.
Three consecutive evaluation faults open a one-second monotonic cooldown.
Success resets closed-circuit failures. Exactly one caller probes after expiry;
others bypass until it completes. A failed probe renews cooldown, success closes
it. Evaluations already in flight cannot close an opened circuit. No call waits
for a lock, another cell, or a probe. Original outcomes remain outside recovery.

Skipped inspection closures are disposed under native recovery before the
original operation runs. Successful cleanup keeps the admission reason;
a recoverable cleanup panic reports `Bypassed(Panic)`, including when disposing
its panic payload also panics. Cleanup never invokes inspection or changes the
gate's failure accounting.

Criterion release benchmarks measure not-ready, open-breaker and fresh internal
error paths separately from the normal guard parsing the typical response
fixture. Construction, circuit priming and configuration are outside timing;
the original operation is an identity return inside timing, with branch
validation and black-boxing. Thus the result is
guard overhead with a trivial original operation, excluding network, streaming,
SDK/WASM bindings and drift classification. Fresh errors include the engine's
immediate error return, containment and failure accounting, not panic unwinding.

Each case uses 100 samples, 1 s warm-up and 3 s measurement, 95% confidence and
10,000 resamples. CI rejects any bypass mean's upper confidence bound >= 10,000 ns
or >= the normal inspection mean's lower confidence bound. Missing, malformed,
non-finite or incomplete reports fail closed. Hardware/OS/runtime metadata and
Criterion reports are retained for each run. This enforces an absolute budget on
the executing runner, not a portable SLA or the working path's separate p50.

### ✅ 10.11 Schema learning and local baselines — SETTLED

The standalone inspector learner accepts complete captured JSON bytes. Invalid,
truncated or resource-limited captures are skipped, never vendor drift, and do
not advance learning. Parsing uses raw tokens so shape inference never rounds a
number; integer tokens (including oversized integers) are `integer`, decimal or
exponent tokens are `number`. This is shape evidence, not numeric coercion.
Supplies the lossless `Inspection` tree; the learner retains a separate
byte entry point.

A caller-owned in-memory store keys state by exact `(vendor, operation)`.
Explicit `begin` starts monotonically numbered versions per key. The first N
accepted live samples establish a baseline, including the Nth sample; N must be
positive. Warming emits no alerts and publishes no incomplete baseline. Once
published, a version is immutable: further observations return `Frozen` and do
not adapt to drift. Explicit relearning retains earlier versions for lookup;
Owns contract selection and persistence/registry transport remains future.

Inferred JSON Schema describes unions of types, object properties and pooled
array items. Empty arrays supply no item evidence. Requiredness is not inferred.
Node presence counts once per response, even if many array elements carry it.
Node confidence = presence / accepted sample count; it measures coverage, **not**
repair safety (§10.2). A typed structural spec prior supplies one pseudo-sample:
prior-only nodes have confidence `1/(live_samples+1)`, observed nodes use
`(live_presence+prior_presence)/(live_samples+1)`. Conflicting types form a union.
N always counts live samples only. Callers extract types/properties/items from
specs outside the learner; loading OpenAPI and enforcing its constraints are
not part of this seam.

Paths use dot-separated object keys, `[]` for array items and `$` for the root.
Literal backslash, dot, brackets and dollar signs in keys are backslash-escaped.
Limits bound bytes, nesting depth, parsed/merged nodes, keys and retained versions;
rejection never mutates state and capacity exhaustion never evicts a baseline.
No guard, alert or policy decision is wired in by this ticket.

`numeric::Number::as_i64` and `as_u64` remain variant accessors: they expose only
`I64` and `U64` respectively, with no conversion of another variant or lexeme.
This preserves the existing API and avoids implying numeric modeling of
verbatim or unrepresentable tokens.

### ✅ 10.12 Lossless inspection tree — SETTLED

`inspect(captured, truncated, precision)` accepts `Option<Precision>`; `None`
means `Observed`, not an inferred precision decision. Its parsed outcome owns a
JSON tree whose numbers are `numeric::Number`. Raw numeric lexemes are passed
directly to `Number::read`, never through a float. `Observed` keeps oversized
integers as `Unrepresentable`; explicit `HighPrecision` keeps them `Verbatim`.
Both retain the original lexeme for serialization. Decimal/exponent spellings
and negative zero survive exactly, including exponents beyond f64 range.

Objects retain entry vectors, including duplicate keys, so no accepted numeric
token is silently discarded. Serialization preserves numeric lexemes, not
whole-document bytes; strings, escapes and whitespace may normalize. No-repair
forwarding still emits only the original upstream bytes (§3.1).

`Tee::into_inspection` uses the default; its explicit
`into_inspection_with_precision(Precision)` counterpart supplies a whole-document
precision decision. Adds the field-aware entry point and local snapshot
adapters (§10.18).
Invalid JSON, invalid UTF-8, and incomplete captures still produce recorded
`ParseFailure` values with capture-relative coordinates; incomplete numeric
tokens retain their existing Eof classification. Truncation remains independent
of successful parsing.
The tree bounds recursion to 128 nodes along a path (root depth zero); exceeding
that limit records a Data failure. No alert, classifier, repair, or schema
learner integration is added here.

### ✅ 10.13 Shape diff evidence boundary — SETTLED

`agw-diff` compares typed schema snapshots or a selected schema with a complete
Inspection, for an explicit request/response side. It is a standalone
library; it does not select baselines, enforce policy, rewrite bytes or call
other cells. Schemas adapt as learned evidence, never authoritative specs.

Authoritative schema changes identify removed fields, added required parameters,
optional additions (informational), requiredness changes and structural type
changes as shape. Learned/observed presence changes remain unknown: absence in
one sample does not establish vendor removal. Integer→number widening is
compatible; narrowing is a type change. Numeric/non-numeric transitions are
semantic per; nullability changes are semantic. With explicit metadata,
changed units, expanded enums and changed null meaning are semantic. Other enum
changes or lost metadata are unknown. No value distribution is inferred here.

A rename requires the same caller-confirmed identity on exactly one removed and
one added sibling field. Ambiguous identity yields unknown, never a guessed
rename. Child comparison continues across confirmed renames so meaning changes
cannot hide behind identity. Supplies statistical identity evidence;
Supplies statistical semantic evidence. Identical shapes cannot prove
unchanged meaning, and an empty report makes no semantic-safety claim.

Findings contain side, kind, stable reason and escaped dot paths (`$` root,
`[]` pooled array items). Empty property names use `\e` to remain distinct from
the root and from literal escaped keys. Semantic dominates unknown dominates
shape when aggregating findings; no findings means no aggregate class.

Incomplete inspections are skipped regardless of parse success. Whole syntax/EOF
failures are shape `malformed_body` at `$`; parser resource/Data/I/O failures
are skipped. A complete parsed wrong top-level container against an authoritative
single container type is also `malformed_body`. Duplicate keys, invalid schema
metadata and depth/node/text limits yield a recorded skip, never vendor drift.
Limits default to depth 64, 4096 nodes and 1 MiB of schema text per input.
Inputs remain borrowed and unchanged. Native quantile timing covers inspection
conversion and diff, excluding capture/parsing/setup; it is a local measurement,
not a WASM or end-to-end latency claim.

### ✅ 10.14 Selective repair library — SETTLED

A standalone `agw-repair` entry point accepts original capture bytes, the
inspection, one side/report, mode and explicit caller proposals. It does
not wire a decision into the guard, emit headers, load a registry or send audit
artifacts. The caller still owns final policy and write-integrity verification.

Each proposal references one finding and supplies detector confidence and
applicability (both finite in [0,1]); per-repair confidence is their minimum.
These inputs are detector evidence, not inferred-node presence confidence or
calibrated production guarantees. Shape threshold is 0.97, semantic is 0.99;
strict raises the minimum to 0.99. Unknown drift is never repaired. Observe
records assessments but never mutates. No configurable lowering of thresholds.

Supported operations are confirmed sibling-key renames and explicit scalar
replacements. Renames require a shape `Renamed` finding with `ConfirmedIdentity`
reason and matching sibling destination; supplies standalone statistical identity
evidence (§10.15). Replacements require a known drifted scalar and an explicit raw
JSON scalar token. The engine does not synthesize values. Every type transition
involving a number passes through the fence and is refused as semantic,
even if the input finding was misclassified or its confidence is low. Other
type transitions are unsupported except 's explicit empty-string-to-null
normalization. That exception requires a semantic `NullMeaningChanged` finding
with `MeaningChanged` reason, ≥0.99 confidence, an actually empty source string
and literal null replacement. It never infers a mapping, changes numeric types,
normalizes nonempty strings, or inserts missing values. Observe still preserves
bytes and still requires independent verification before dispatch. This
removes one avoidable refusal while preserving the three numeric fences in the
unchanged repairable cohort. No object/array replacement, insertion or
deletion can broaden a field-level patch into a payload rewrite.

A bounded raw-token index follows the supplied parsed tree and validates every
key/value against the source (numbers by original lexeme). Duplicate decoded
keys, stale/mismatched inspections, parser failures and incomplete captures
prevent repair. Paths use escaping and empty-key `\e`; proposals
use pooled `[]` paths, which expand to concrete audit paths with zero-based
`[index]`. Root replacement is unsupported. Rename destinations must be absent
from the original object; swaps, chains and overlapping edits are rejected.
A changed key token or scalar token is the only replaced byte span; whitespace,
key order and all untouched numeric/string lexemes are preserved verbatim.
No-op plans preserve the original borrowed bytes and produce no audit patch.

The supplied plan applies atomically: any rejected proposal, collision,
overlap or resource failure returns the original borrowed slice. Unresolved
findings (including findings with no proposal) remain available to policy;
a numeric-coercion refusal upgrades the corresponding unresolved class to
semantic. Internal capture/index/limit failures are recorded separately and
are availability-bypass inputs, never invented vendor drift. Callers must handle
failure before consulting unresolved classification. A complete unparseable
capture is non-repairable; any existing the `malformed_body` finding remains
subject to §4.4, including strict escalation. The repair status does not replace
that policy decision. Oversized-report preflight
can return before copying findings; the original report remains caller-owned.
Limits default to
8 MiB input/output, depth 64, 4096 parsed nodes and 64 proposals; replacement
bytes, proposal metadata and edit counts are also bounded before allocation.

On application, the result owns patched bytes and the generated audit `Patch`
records `applied: true`, `selective: true`, exact concrete `fields_changed`, and
`byte_identical: false`. Aggregate applied confidence is the minimum of applied
repairs. Audit stamping copies only this patch/confidence and execution mode
into an owned draft; it never asserts `write_integrity`, changes the policy
decision or mutates a published artifact. No applied patch means `patch: null`.

### ✅ 10.15 Statistical rename evidence — SETTLED

`agw_diff::rename` is a standalone borrowed-histogram matcher. Callers select
removed/added sibling fields for one vendor, operation, side and compatible
before/after windows. It neither collects traffic nor invokes guard/repair.
A histogram supplies a scalar JSON type, a binning identifier, exact bin labels
and positive integer counts, plus total opportunities (including absences).
Labels must encode values without collisions; numeric binning uses identical
caller-defined boundaries/units and identifier on both sides. No lossy numeric
conversion or lexical field-name signal is used. Only equal non-null scalar
types and equal binning identifiers are comparable. Empty/null-only fields
cannot confirm identity. Names are unique per side and disjoint across sides.

Counts normalize to probabilities over union support. PSI is
`Σ (p-q) ln(p/q)`, using a floor of 1e-6 followed by renormalization for PSI
only. JS is `0.5 Σ[p ln(p/m) + q ln(q/m)]`, `m=(p+q)/2`, with zero terms
contributing zero and natural logs. A plausible edge requires PSI ≤0.1 and
JS ≤0.02. Every plausible edge participates in ambiguity: only endpoints with
exactly one plausible edge each confirm a match. No ranking breaks ties, and
low confidence never removes a competing edge. Incompatible candidates are not
plausible. Identity remains statistical evidence, not proof of unchanged meaning.

For a unique match, let `d=max(PSI/0.1, JS/0.02)`, `n` be the smaller present
count and `coverage` the smaller present/opportunity fraction. Confidence is
`(1-0.1*d) * (0.9+0.1*min(n/100,1)) * (0.9+0.1*coverage)`, clamped to [0,1].
This deterministic evidence score is not a calibrated probability. Sparse or
low-volume matches may confirm identity below the separate §10.2 repair gate.
Callers may assign the same unique identity to the matched nodes and pass
this score as detector evidence; applicability remains independent.
Child semantic checks and repair thresholds still apply.

Limits are 64 fields per side, 256 bins per field, and 1 MiB aggregate UTF-8
metadata. Invalid counts (zero-count bins, count overflow, count > opportunities),
invalid duplicate/overlapping names and exceeded limits return explicit errors
before matching; no partial confirmation escapes. Empty histograms with zero
opportunities are valid non-evidence. Inputs remain unchanged. No registry,
I/O, audit publication, production calibration or latency guarantee is added.

### ✅ 10.16 Local contract store and signed selection — SETTLED

the registry supplies an in-memory TypeScript library. Each exact
`(vendor, operation)` key owns append-only versions with unique opaque IDs and
monotonic append ordinals (IDs are never lexically ranked). Versions hold
request and/or response shapes, immutable after publication, and a SHA-256 digest
binding scope, ID and normalized shape content. Publication and pin installation
are asynchronous administrative operations; selection only reads local memory.
No in-path call to another cell/database/crypto service is added. Signed structural cache delivery is now explicit background work under §10.48;
default runtime integration remains host-owned.

A bounded shape describes JSON types, object properties, required names, optional
additional-property allowance (default false) and optional array items. Observed
shapes describe a complete wire value or complete pooled items; their `required`
list means guaranteed presence, not inferred schema requiredness. Observations
must enumerate their possible fields (`additional: true` is invalid evidence).
Callers supply a complete flag; incomplete, invalid or resource-limited shapes never drive
automatic selection. Verified pins take precedence over wire evidence.
Compatibility requires every observed type to be accepted (integer may widen to
number), every required field to be guaranteed present, known properties to
recursively match, and unlisted properties to be allowed explicitly. Empty array
observations may omit items. Unconstrained expected array items accept any items.
A missing version side is incompatible. This is structural compatibility only;
it cannot prove unchanged semantics or authorize repair.

Without a pin, evaluate versions newest ordinal first and select the first
compatible shape; no compatible version yields an explicit unresolved result.
A customer pin is installed only after Ed25519 verification against a trusted
public key bound to that tenant/key ID. Its domain-separated signed payload is
a fixed-order JSON array containing protocol tag, tenant, key ID, vendor,
operation, version ID, content digest and pin revision. Signature is exactly
64 bytes encoded as 128 lowercase hexadecimal characters. Revision starts at 1;
a replacement must be exactly the next revision and is checked again after
asynchronous verification. Replays, rollback, tampering, wrong tenant/key/scope,
unknown versions and content mismatch cannot change active state. Pin history
is retained. The library never receives private keys. Key provisioning/rotation,
revocation, expiry and durable anti-replay state are outside this ticket.

Installed signed pins always select their exact version, even if its observed
shape is incompatible/incomplete/invalid or newer versions exist. An optional
explicit version request must match that installed pin; an unsigned/unknown/conflicting request
is unresolved, never downgraded to automatic selection. Missing pinned content
also never falls back. Pins are tenant scoped; the shared shape corpus contains
no customer payload values. This clause covers pin isolation; private corpus isolation is specified in §10.49.

Every resolution includes a JSON-safe diagnostic trace with protocol version,
tenant/scope/side, observed shape, method, selected ID/digest/ordinal (or null),
result reason, examined candidate digests/ordinals and first mismatch paths with
expected/observed evidence and observation status, plus signed pin provenance
when applicable. Only candidates examined up to selection appear, in descending
ordinal order.
A pinned version with incomplete/invalid observation has `compatible: null`,
never a fabricated compatibility result.
Audit stamping reserves `metadata.contract_selection` in frozen audit v1's
existing free-form metadata. Success stamps `proof.contract_version` and
`verified_against` (`pin` or `registry`); failure leaves the caller's existing
proof untouched and records `selected: null`. The trace is a selection receipt,
not verification of a write; it never sets `write_integrity`. Artifacts contain
shape structure and signatures, never private keys or payload values.

Limits: 128 scopes, 64 versions/scope, 1024 tenant/scope pin histories and 64
pin revisions/history. Each shape bundle/observation allows depth 64, 4096 nodes
and 1 MiB of UTF-8 metadata; identifiers are nonempty and ≤256 UTF-8 bytes.
Invalid administrative operations raise typed registry errors before publication;
selection returns unresolved diagnostics for invalid input. Callers must route
unresolved/internal failures through their availability/policy boundary rather
than invent a baseline. These local seams do not implement headers or repair.

---

### ✅ 10.17 Malformed-body policy seam — SETTLED

`agw-guard::malformed` is an explicit native `CallContext` library seam over
Classification, not the default engine or streaming middleware. It accepts
borrowed captured bytes, an independent incompleteness flag, side, baseline,
explicit tenant/connection scope, vendor, a caller-owned recurrence slot and a
generated audit draft. It returns the same byte slice, disposition, owned
headers, warning and finalized artifact. Callers publish artifacts/warnings and
honor `Halt` before dispatch; an already-streamed response cannot be halted
retroactively. No artifact persistence, remote lookup or general policy engine
is added. Top-level shape evidence retains §10.13's authoritative-container rule.

Complete malformed findings produce exactly one `$` / `malformed_body` change,
shape class, confidence 0.00 and repair skipped. Observe/repair reads are
`degraded`; writes and unknown operations are `bypassed`. Side and operation are
independent: a write's malformed response is still a write. Strict returns
`Halt` with `blocked` and `drift_halted` human-escalation feedback on either
operation, consistent with §§2 and 4.2's strict opt-in. This is a policy result,
never a parser exception. Audit patch is unapplied and byte-identical, and
`write_integrity` is false; no prior draft repair/hypothesis claims survive.

Incomplete captures skip parsing/classification, including valid JSON prefixes;
a parser reporting incompleteness also takes this branch. They return
`Incomplete`, no drift headers, and a bypassed artifact with
`detail: inspection_incomplete` and no drift/confidence. This records unavailable
inspection without choosing 's operational header/counter policy. Parser
panics, Data/I/O failures and skipped comparisons return transparent bypass,
`detail: inspection_fault`, and no drift claim in every mode. Native containment
has §10.9's unwind/allocator limitations. Other complete clean/drift findings
return `NotApplicable` without an artifact for the normal pipeline to handle.

Recurrence retains one active key (tenant/connection scope, vendor, tool,
operation, side and baseline version). Only complete malformed findings advance
its saturating positive occurrence count; the first call's supplied timestamp
is retained. Key changes reset the slot; callers retain separately scoped slots
for interleaved histories. No global/unbounded history or synchronous I/O exists.

---

### ✅ 10.18 Per-field numeric precision — SETTLED

`contract_snapshot` v2 adds optional boolean `fields[].high_precision`. It is a
numeric interpretation hint, not a payload validation constraint. Missing/false
means `Observed`; true supplies `HighPrecision` at that exact numeric leaf. A
container flag never propagates to descendants. Numbers that fit i64/u64 retain
the fast path even when flagged. Every numeric spelling remains lossless in both
modes; an unflagged overflow is retained as `Unrepresentable`. No crate enables
`serde_json/arbitrary_precision`. V1 and its frozen baseline remain on disk, and
CI checks superseded frozen versions for semantic edits as well as current ones.

`FieldPrecisions` compiles snapshot field paths outside forwarding (maximum 4096
entries and 1 MiB of path text; empty/duplicate paths are rejected).
`inspect_fields` applies the compiled map before `Number::read`; the existing
`inspect` and tee whole-document APIs retain their behavior. Paths use escaped
dot components, `[]` for pooled array items, `$` for root and `\e` for an empty
property. Literal punctuation/backslashes are escaped. `Schema::precision_paths`
uses this canonical spelling; 's legacy `paths` API remains unchanged.

The learner records `overflow_samples` per node: complete accepted live samples
containing an integer outside both i64 and u64. It counts each field at most once
per sample, including pooled arrays. Decimal/exponent/negative-zero lexemes are
not overflow evidence. Structural spec priors add no overflow samples. At least
two live overflow samples are required to publish `high_precision: true`;
otherwise the hint remains absent. A generation with N=1 cannot infer it.
Rejected/incomplete samples and previous generations contribute no evidence.
Coverage confidence stays separate from precision and repair confidence.

`agw-guard::snapshot::with_learned_precision` copies published hints into existing
snapshot field metadata, preserving explicit true/false choices without guessing
wire types for schema unions. `precision_for` compiles one already-selected tool.
The registry preserves optional hints in immutable normalized shapes and signed
version digests, ignores them for structural compatibility, and offers
`withPrecisionHints` to copy hints into snapshot drafts before boundary validation.
Absent flags preserve previous normalization/digests; old versions are not
backfilled. Overflow counts remain local evidence, not new wire fields.

These are local producer/consumer seams, not registry delivery, persistent
migration, default-engine wiring or a repair policy change. Snapshot messages
carry field metadata, not numeric payload values: round-trip tests validate the
snapshot then inspect a separate numeric payload using its flags. No frozen v1
or audit schema is changed.

### ✅ 10.19 Unit-conversion evidence — SETTLED

`agw_stats::unit` compares caller-owned finite f64 sample windows for one
field, vendor, operation, side and tenant/volume segment. Callers own selection
and projection; lossless/high-precision tokens must not be silently rounded into
this API. No traffic collection or guard wiring is added.

Each window is bounded to 4096 samples; field paths are nonempty, ≤4096 UTF-8
bytes. Validate all inputs before returning evidence. Nonfinite samples,
unrepresentable computed metrics/ratio and limits return explicit errors. Empty
or <20 samples per window return insufficient evidence (not clean evidence).
Sort absolute values for magnitude metrics. Wasserstein-1 is the exact empirical
transport distance with uniform sample weights, including unequal window sizes.
Normalized distance divides by the larger window mean absolute magnitude (zero
for two all-zero windows). PSI uses baseline decile upper boundaries at sorted
indices `ceil(k*n/10)-1`, k=1..10, deduplicated, plus bins below the baseline minimum and above its maximum. Equal
values enter the bin ending at that boundary. Count both windows in these same
bins, normalize, floor probabilities to 1e-6 and renormalize, then sum
`(p-q) ln(p/q)`. A semantic finding requires both PSI ≥0.2 and normalized
Wasserstein ≥0.2. These fixed deterministic thresholds are not calibrated.

Only after both gates pass, divide the observed signed median by the baseline
signed median (even medians use the midpoint). A zero baseline median gives no
hypothesis. A positive measured ratio within inclusive relative tolerance 0.02
of 100, 0.01, 1000 or 0.001 emits typed `UnitConversion` evidence: medians,
measured ratio, matching factor and tolerance. Negative ratios cannot match.
Other significant shifts remain semantic without a specific hypothesis. Magnitude
comparison cannot detect sign-only flips or unchanged distributions/meaning.

The finding's audit adapter stamps generated v1 semantic drift for the supplied
side/path and optional `hypothesis: unit_conversion` plus a measured detail.
It clears stale hypothesis/detail/note/confidence, patch and write-integrity
claims; the caller owns the final policy decision (§4) and publication. It never
returns a repair proposal or changes payload bytes. A named hypothesis is not
proof of units and must never cause automatic write conversion. No new wire
fields, production false-positive claim or latency guarantee is introduced.

### ✅ 10.20 Cardinality and enum growth — SETTLED

`agw_stats::enumeration` is a standalone comparator for caller-selected string
enum fields. Callers supply a known vocabulary and timestamped observations for
one field/vendor/operation/side/tenant-volume segment and complete comparable
window. Values are decoded strings, compared exactly without case folding,
trimming or Unicode normalization; empty strings are valid categories. This
API does not infer that arbitrary identifiers or free text are enums.

A report records baseline distinct count, observed distinct count, and union
count. Every value outside the baseline is listed lexicographically, with its
count, frequency (`count / total observations`) and minimum supplied UTC timestamp.
First-seen means earliest in the supplied window, not a persistent first-ever
claim. Duplicate baseline entries are invalid. Equal or lower observed
cardinality does not suppress new vocabulary: replacements also affect meaning.

A nonempty baseline and at least 20 observations are required for sufficient
evidence. A new value is material only with at least 2 occurrences and frequency
≥0.01, inclusive. A report is semantic if any new value is material; it otherwise
states insufficient evidence, below-materiality novelty, or unchanged vocabulary.
All novelty remains visible, including below-floor values. An empty baseline
is insufficient evidence and cannot identify novel values. Known-value frequency
changes alone never fire, however large: vocabulary growth/replacement takes
precedence over numeric mean movement by requiring no mean/distribution signal.
This is local categorical evidence, not 's cross-detector weighting,
customer-configurable materiality or a calibrated false-positive guarantee.

Limits before results: 256 baseline entries, 4096 observations, 256 distinct
observed values, 4096 UTF-8 bytes per value/path and 1 MiB aggregate input text
(path, baseline entries and every observation value). Paths must be nonempty.
Violations return typed invalid/resource errors, even for insufficient windows.
Inputs are borrowed and unchanged; timestamps are supplied, never read from a clock.

Only a semantic report can stamp a generated v1 audit draft. It records
`enum_expanded` changes with the path and exact new string in `to` for material
values, the earliest material first-seen timestamp, and structured report evidence
under reserved `metadata.agindow_enum_growth` (replaced on stamping; other metadata
preserved). The metadata enumerates all novel values with count, frequency,
first_seen and material flags plus path and three cardinalities. Stale hypothesis,
detail, note, confidence, patch and write-integrity claims are cleared. The caller
retains the final §4 policy decision and publication. No payload edits, repair
candidates, guard wiring, persistence, new wire fields or latency claim is added.

### ✅ 10.21 Completeness, type consistency and string fingerprints — SETTLED

`agw_stats::field` supplies three standalone comparisons over caller-selected
complete windows for one field/vendor/operation/side/tenant-volume segment.
Samples are `Option<&agw_inspector::Value>`: None is missing, Some(Null) is present
null. No field extraction, transport, clock, registry or policy execution occurs.
Profiles expose opportunities, present/non-null counts, type counts and separate
missing/null/numeric-zero/empty-string counts. Completeness is non-null / all
opportunities. Zero is mathematical zero in a valid JSON number's mantissa,
ignoring sign/exponent; no f64 projection occurs. Integer vs number follows
's lexical rule (decimal point/exponent means number). Booleans are not zeros.
Containers count only as their top-level type; children are not traversed.

Each window needs ≥20 opportunities for completeness. Emit semantic typed reasons
for absolute completeness delta ≥0.2 and for each sentinel rate delta ≥0.2,
using all opportunities as denominator. This records appearance/disappearance,
including optional-field presence; caller-selected semantics remain statistical.
Type consistency excludes missing/null and needs ≥20 non-null values per window.
A baseline's dominant type must have share ≥0.9; ties cannot qualify. Emit a typed
semantic consistency reason when the same type's observed share drops ≥0.2.
These are detector signals, never numeric conversion/repair permissions.

String fingerprints need ≥20 strings per window. Each string contributes at most
once to each character class: ASCII letter, ASCII digit, ASCII whitespace,
ASCII punctuation, control, non-ASCII, and U+FFFD replacement. Control takes
precedence over whitespace (e.g. Tab/newline are control); U+FFFD has its own class
rather than non-ASCII. Profiles expose class sets/counts and format counts.
Class rates divide by string count; absolute deltas ≥0.2 emit typed semantic
reasons. Empty strings contribute no class and have unknown format.

Recognized whole-string ASCII formats are ISO date `[0-9]{4}-[0-9]{2}-[0-9]{2}`,
ISO timestamp date + `T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})`,
month/day `(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) [0-9]{1,2}`,
and digits `[0-9]+`, all anchored at both ends. These are lexical shapes, not
calendar validators. Unknown is counted but cannot emerge as a named format.
A recognized format with baseline share ≤0.05 and observed share ≥0.2 emits a
typed semantic emergence reason containing its anchored pattern and both rates.
Decoded Unicode class shifts suggest changed text, not proof of raw encoding.
Unrecognized format changes and same-fingerprint meaning changes may be invisible.

Each comparison validates path and both windows before evidence: nonempty path
≤4096 UTF-8 bytes, ≤4096 samples/window, ≤4096 bytes/string or retained numeric
token, ≤1 MiB aggregate path/string/numeric text across both windows. Retained
numeric tokens must be valid JSON numbers. Profiles are privately constructed;
errors and insufficient evidence are distinct from sufficient no-findings.
Fixed inclusive thresholds use rational counts for boundary decisions (not
rounded rate subtraction). `profile` exposes a full fingerprint; comparisons
avoid string scans where that detector does not need them. Inputs remain borrowed.

`cargo bench -p agw-stats --bench field_detectors` gates measured nearest-rank
p99 <2 ms for each native detector on fixed 128-sample-per-window fixtures,
including validation, both profiles, comparison, returned evidence and drop.
Setup/parsing are excluded. The report retains toolchain, platform, sample count,
quantiles and fixture descriptions in `target/ti17-field-detectors.json`.
This local gate establishes only that fixture/native scope; no production or WASM
calibration, guard wiring, cross-detector ranking or global latency claim is made.

### ✅ 10.22 Field profile persistence and delivery — SETTLED

Profile identity is `(tenant, vendor, operation, side, tenant_segment,
volume_cohort, JSON Pointer field)`. Every dimension is required; no global
fallback exists. Caller-owned complete windows have immutable IDs, UTC start/end
and observations; ingestion time does not choose the bucket. A window belongs
to its start hour/day/month, even if it spans the boundary. Owns selection
and seasonality; owns relay collection. No runtime integration is implied.

The v1 `field_profile` envelope carries mergeable bounded summaries: eight type
counts (including missing), 64 SHA-256 value buckets, 64 HLL registers, fixed
numeric histogram bins and six Unicode character categories. Null is present
but incomplete. Cardinality is an HLL estimate, not exact enum evidence.
Numeric quantiles report containing intervals, not exact values. Non-finite or
unsafe integer values may not enter numeric bins; producers count those as
excluded numbers. Lossless relay numbers require a deliberate adapter.
Categorical PSI (epsilon 1e-6) and natural-log JS use shared value buckets;
Wasserstein integrates numeric histogram CDF differences over fixed finite bin
midpoints, returning unknown for excluded values or unbounded tails. These are
coarse profile statistics, never calibrated detector confidence or repair proof.

A profile window has at most 4096 observations, 64 KiB input string bytes and
64 KiB serialized message bytes. Messages are schema validated and counters
cross-checked. Rollups merge counters and max HLL registers, then derive metrics;
they never average quantiles, cardinalities or divergence scores. Segments never
merge. Counters stop at safe integers. Raw windows live in monthly Postgres range
partitions. Hourly/daily summaries update atomically with first insertion;
redelivery with identical content is a no-op, conflicting content fails. Row count
and a durable `clickhouse_migration_required` alert update in that transaction at
20,000,000 raw rows. Queries use preaggregates and bounded time ranges.

Queues producer work attaches to caller waitUntil outside the decision path.
Consumers acknowledge after durable insert and warm-cache publication; transient
failures retry, including cache failures after commit. Invalid messages reach the
configured DLQ after bounded retries. A SQLite-backed Durable Object per full
profile key persists versioned snapshots. Revisions follow serialized rollup
updates, never wall clocks. Background refresh installs a bounded, expiring local
snapshot; synchronous hot reads have no reference to storage or network clients.
Cache misses, expiry and refresh faults yield unavailable evidence, never a
policy decision. Default deployment exposes no public ingestion/read HTTP route; adds the
opt-in authenticated ingestion boundary below. Trusted bindings/caller
authorization must supply the tenant. SQL credentials
and Cloudflare provisioning remain operator-owned.

Export uses v1 without collapsing relay baselines: `tenant_segment` is
canonical JSON `["ti48", connection, generation, segment, window]`, bounded by
v1's 256-character/1024-byte limits; `volume_cohort` is mandatory trusted config.
Overlong scope tuples are rejected, never truncated or hashed. Tenant, vendor,
operation, side and field retain their meaning. Closed windows require explicit
UTC millisecond `[start,end)` bounds enclosing every observation and a unique
caller-owned close ID. SHA-256 over scope, close ID, times and summary gives a
stable window ID; the durable spool retains the exact serialized envelope.
Per-observation times choose the interval; they are not uploaded individually.

Closing only detaches immutable samples into bounded local staging; it neither
profiles nor performs I/O, publishes nor advances a baseline. Reject-new overflow
is explicit and counted. Native background work alone builds SHA-256/HLL summaries,
checks bounds, commits an atomic fsync/rename spool, and invokes transport. Only
Observed exact integers within ±(2^53−1) with enabled projection enter numeric
bins; other number lexemes contribute exact value hashes/type counts and an
excluded-number count. Lexical decimals/exponents occupy the number type slot;
no floating conversion is attempted. No raw scalar or payload crosses the network.

The native spool is single-owner, local and bounded by message/count/byte limits;
crash before durable staging can lose uncommitted local observations. After
commit, retries retain exact content/ID across restart, including after a lost
acknowledgement. Retries have persisted attempt counts and UTC backoff deadlines;
exhausted entries are dropped with persisted loss counters. Disk failures retain
pending memory and prohibit sends until a durable commit succeeds. Capacity never
silently promotes or evicts a baseline. The caller schedules background pumping
outside the guard; no sender/storage handle is accepted by guard evaluation.

The opt-in `/profiles` Worker route accepts one bounded envelope after verifying
a tenant-bound bearer secret from operator configuration (SHA-256 digests, at
most 256 tenants). It rejects a body tenant mismatch before Queue publication.
No credentials configured means ingestion disabled. Native HTTPS-only `curl`
transport refuses redirects and uses a bounded timeout; Queue acceptance is the
acknowledgement, not a claim of database commit. The existing consumer
retains idempotence across restart and duplicate/reordered delivery. Provisioning
and hosted qualification remain explicit operator work in staging.

Refresh protocol uses the frozen `profile_snapshot` v1 envelope: registry
provenance `registry-rollup-v1`, generation, grain (hour/day), UTC bucket,
revision, contributing window count and one complete v1 field profile. A caller
passes an exact -selected scope, generation, bucket and profile start/end;
refresh never selects latest or substitutes a seasonal slot. 's canonical
segment tuple binds connection, generation, segment and window. Only that format
is supported. Separate selected slots are separate immutable local entries;
there is no inferred multi-field or multi-season readiness.

The opt-in POST `/profile-snapshots` route uses a separate tenant-bound reader
credential map, checks authorization before lookup, queries exactly one selected
rollup and refuses interval/generation mismatch. A missing rollup is unavailable.
TLS and operator credentials authenticate provenance; the provenance string alone
is not a signature. Native background HTTPS/curl and WASM host-provided async
HTTPS fetch are explicit transports; WASM needs no Worker binding. Redirects are
refused, responses and requests are bounded (128 KiB/8 KiB), and timeout is at
most 30 seconds. Hosts own scheduling/cancellation; no refresh runs from a guard.

Relay selection admission is bounded and reject-new, with no silent eviction.
A local monotonically increasing selection epoch fences delayed completions,
including after scope/generation replacement. Revisions are comparable only
within the exact selected slot. Lower revisions fail; equal revisions require
identical canonical content or fail as conflicts. Full validation precedes one
atomic immutable replacement. Old call handles retain their original content.
Revalidated identical revisions may renew freshness; failed fetches never do.
Freshness uses monotonic time since successful validation, not the historical
baseline's event time. Expiry returns unavailable while retaining revision fences.
Restart loses the cache and requires an authorized refresh before use.

Startup, invalidation notifications and periodic recovery drive background work.
Each selected slot has at most one in-flight fetch, bounded attempts per recovery
cycle and exponential capped backoff; exhausted cycles wait a recovery interval.
Notifications coalesce and cannot bypass backoff. Explicit timeout, refresh period,
freshness TTL, capacity and byte limits are validated at construction. Telemetry
counts attempts, failures, stale/missing local reads, conflicts/rejections and
successful installs, and exposes monotonic refresh age and next due time.
Aggregate consumers receive the original approximate histograms and exclusions.
Exact enum vocabularies, lossless numeric samples, per-sample timestamps,
confidence and repair proof remain unavailable: these snapshots cannot become
Exact windows or make ready. Existing availability handling preserves
original guarded outcomes. Hosted qualification uses staging separately;
local native/WASM tests require neither provisioning nor production traffic.

### ✅ 10.23 False-positive controls — library design settled

This is a standalone local library, not a shipped or production-qualified detector.
Selection requires exact `(tenant, vendor, operation, side, tenant_segment,
volume_cohort, field)` identity; operation must identify the endpoint. There is
no global fallback. Complete current windows align to caller-configured UTC epoch
slots. Select exactly N preceding seasonal periods (1–52), same duration and slot;
period must be a multiple of duration. All selected windows must meet minimum
sample count. Missing, duplicate, incomplete or undersampled slots are unavailable,
never clean observations. Returned indices borrow caller-owned windows; selection
never averages summary metrics. Input is capped at 4096 windows and 1024 bytes
per identity component. Callers retain immutable snapshots for a call.

Materiality accepts caller-supplied finite normalized absolute effect (0–1), valid
p-value (0–1), and the total tested hypothesis count (1–4096). It does not estimate
p-values or reinterpret existing detector confidence as significance. §10.24 adds
raw-window adapters that supply these inputs. Eligibility
requires `p <= alpha / tests` and `effect >= floor`; alpha must be in (0,1).
Defaults and at most 256 unique `(tenant, field, signal)` overrides are explicit
caller configuration. Floors must be in (0,1], preventing zero-effect alerts.
Mean shift receives weight 1; enum novelty, unit hypothesis, completeness, type
consistency and string-format evidence receive weight 2. Eligible evidence ranks
by weight times effect, with agent-relevant signals first even on a score tie.
These are alert decisions, never semantic-write policy overrides or repair proof.
Existing detector-local minimum evidence floors still apply upstream.

One caller-owned budget belongs to one `(tenant, connection)`, shared across fields,
segments and endpoints. UTC epoch day and cap are explicit; no clock or I/O occurs.
Cap zero disables notifications. For cap N>0, N−1 individual notifications may be
emitted; the Nth slot creates one consolidated summary, and all later findings
update it in place without sending notifications. Cap 1 consolidates immediately.
Counters saturate; older days are rejected and a newer day resets the budget.
The host must serialize calls and persist/update summaries as needed; recreation
must not reset a live connection's budget. Alert budgets never suppress policy.

Silent replay runs these same selection/materiality functions, independent of
notification caps. A run requires 30 consecutive populated UTC days, explicit
nonzero daily traffic counts with last-day volume above first-day volume, and
labeled signal opportunities; each day must have labeled negative opportunities.
False-positive rate is eligible negative opportunities / all labeled negative
opportunities, including unavailable selections in the denominator; unavailable
selection causes qualification to fail rather than dilute the rate. Rate must be
strictly below the explicit threshold in (0,1). Report positive detections and
unavailable counts too, so disabling a detector cannot hide its coverage.
Replay is bounded to 100,000 opportunities and 10,000,000 history-slot visits. Production provenance is a separate
caller attestation with a nonempty dataset identifier; synthetic runs cannot
qualify. No attestation alone proves production origin. A real reviewed dataset,
threshold and report are required to close; none is currently supplied.

### ✅ 10.24 Detector significance adapters — SETTLED

Additive standalone `agw-stats::significance` entry points wrap unit,
Enum and completeness/type/charset detectors with the same raw
windows. Existing APIs and local floors remain unchanged. Reports retain the
original detector result and all tested features, including unchanged features.
Each test exposes method, side sample counts, absolute normalized effect, p-value,
and whether the original detector identified that feature as a material finding.
Its evaluation method returns no candidate for a non-finding. Significance
is not repair confidence, semantic proof or permission to change write policy.

For completeness (non-null plus four sentinels), type consistency (six non-null
types), and string fingerprints (seven character classes plus four known formats),
use two-sided probability-ordered Fisher exact tests on integer success/failure
counts. Denominators respectively are all opportunities, non-null values, and
strings. Effect is absolute rate difference. Zero-variance equal features have
p=1. Test all 5/6/11 features before filtering for detector reasons.

Enum adapters require raw repeated baseline strings, not only a vocabulary.
Construct exact counts for every category in the pooled union (at most 512,
256 per side). Conditional on pooled category counts, these category hypotheses
are fixed under group-label permutations; test every category, including known
ones, rather than choosing a test only after observing novelty. Effect is the
absolute category-frequency delta. Still alone decides which novel values
meet its local count/frequency floors. Vocabulary-only calls remain descriptive.

Numeric unit adapters test the ECDFs of absolute magnitudes. Let D be their
supremum distance, n/m the sample counts, and r = nm/(sqrt(n)+sqrt(m))². Use
`min(1, 4 exp(-2 r D²))`: a conservative bound derived by a triangle inequality,
a split at sqrt(m)/(sqrt(n)+sqrt(m)), and the two single-sample Massart DKW bounds.
This derivation avoids assuming a sharp two-sample formula that can fail at small
sample sizes. It covers ties/discrete distributions and unequal sizes. The
normalized effect is D; this tests magnitude distribution, not mean movement or
a conversion's correctness. 's existing PSI/Wasserstein gates and diagnosis
remain authoritative for local findings. ECDF cross-products preserve ties and
avoid subtracting extreme finite numeric values.

The null assumes independent exchangeable observations from identical distributions
within each selected scope/seasonal comparison. Correlated traffic, repeated
peeking, adaptive window selection and population mixing invalidate the nominal
interpretation; the caller must choose independent sampling/comparisons. No
independence verification or production calibration is claimed. Do not pool
seasonal windows as if they were independent without a justified sampling design.

Every adapter requires the total predeclared hypothesis-family size (1–4096),
including features/fields/seasonal comparisons which yielded no finding. Reject
values smaller than the local feature count; hosts must sum all planned local
families before execution. P-values are unadjusted; applies Bonferroni once.
No adjustment is silently applied twice. At least 20 observations per side are
required, with detector-specific denominators; otherwise reports are unavailable
and carry no tests/p-values. Enum baseline availability requires real counts.

Bound inputs before work: 4096 observations per side, 4096 bytes per text token,
1 MiB combined input text, and existing detector category limits. Fisher normalizes
hypergeometric weights about their mode, sums all masses no greater than the
observed mass (relative tie tolerance 1e-10), and adds a conservative 1e-12 rounding
allowance, capped at 1. Numeric D is reduced by 1e-12 before bounding and the same
p-value rounding allowance is added; neither adapter emits a zero p-value from
floating-point underflow. Errors remain local `Result`s for future containment.
No network/storage calls, guard/default-engine wiring, audit-schema extensions,
policy decisions, production replay qualification or latency claims are added.

Method references: [SciPy Fisher exact specification](https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats.fisher_exact.html)
and [Wei/Dudley, DKW inequalities](https://arxiv.org/abs/1107.5356).
The numeric union bound above is our conservative derivation from the stated
single-sample bound, not a claim of the paper's sharp two-sample result.

### ✅ 10.25 Local detection routing and inference accounting — SETTLED

`agw-stats::router` consumes borrowed diff reports and caller-confirmed
invariant findings. Each shape finding selects selective repair, semantic selects
policy, unknown selects conservative policy, invariant selects verification.
These are handler selections, not executed actions or safety decisions. Preserve
every finding, including semantic evidence beside shape/unknown evidence. Internal
`invariant` never extends the three wire drift classes. Empty complete reports
are clean; skipped reports stay unavailable and schedule nothing. Reject more
than 4096 total findings, paths/rule identifiers over 4096 bytes, empty identifiers,
or over 1 MiB combined path/rule text before allocating output. Routing does not
suppress statistical tests or change 's predeclared family.

Only complete routes containing unknown evidence are eligible for optional Tier 2
submission. A local FIFO holds at most 1024 jobs (zero disables it). Submission
copies one nonempty audit reference of at most 4096 bytes, not customer payloads.
It invokes no user callback, worker, network or future. Caller-owned call budgets
are non-cloneable and shared across request/response phases; one admitted job per
call reserves its configured integer micro-currency ceiling. Full/disabled queues,
known/clean/unavailable cases and insufficient budgets reserve nothing; failure to
enqueue never changes local handling. No retry is implicit. Queue delivery and
crash recovery are future work; use unique tenant-scoped audit references.

In-path inference cost is always zero. Dequeued jobs retain their reserved cap.
Background workers authorize each maximum cost before dispatch; the sum of grants
cannot exceed the cap (including unsettled grants). Each consumed grant settles
once with caller-reported actual micro-currency cost and an explicit overrun flag.
Unused reservation is not recycled. Receipts retain the audit reference for
per-call aggregation. A provider exceeding a grant is measured as an overrun,
not silently clamped or claimed bounded. Workers must enforce provider token/price
limits and export receipts; this local seam cannot constrain external billing.

Caller-owned audit-review metrics hold a fixed 4×4 matrix (truth rows, predicted
columns); accuracy is absent before review and otherwise diagonal/total. Only
explicit reviewed labels count. Checked counters reject overflow without mutation.
Callers correlate labels to audit evidence and deliver each review exactly once;
no production accuracy, review transport, persistence or deduplication is claimed.
Owns guard wiring; policy, repair, invariant execution and Tier 2 workers
remain separate. No new wire schema, default-engine activation or latency claim.

---

### ✅ 10.26 Local statistical windows — SETTLED

`agw_stats::windows` supplies bounded local input lifecycle for §10.19–10.21.
Every scope includes tenant, connection, vendor, operation, side, strict JSON
Pointer field, segment, selected window identifier and baseline generation.
Only complete parsed inspections without duplicate object keys are eligible.
Object-only scalar paths preserve missing/null, exact decoded strings and numeric
lexemes. Empty and escaped property names work; arrays, pooling, wildcard syntax,
container leaves and scalar traversal are unsupported. One call contributes at
most one opportunity per selected field. Any rejected selection rolls back the
whole admission batch, including new scopes and readiness.

Cold windows warm until explicit publication freezes a generation's baseline,
clears its live window and preserves old snapshots. Publication requires the
configured sample floor; a generation cannot be republished. Relearning uses an
explicit new generation. Live windows advance only through explicit clear; calls
never enter their own reference baseline or expand its vocabulary automatically.
Ready requires the configured floor on baseline and live samples; warming and
unavailable are explicit, and detector-specific sample sufficiency still applies.
Snapshots expose immutable shared reference/live samples and detector adapters.
Enum timestamps are caller supplied. Field profiles preserve all scalar/missing
sentinels. Unit projections require explicit Observed precision authorization and
are restricted to exact integers in ±(2^53−1); high-precision, retained numeric
lexemes, unsafe integers and unrepresentable tokens skip only unit evidence.
Precision hints must accompany authorization because the inspection tree does not
retain the hint for every number variant.

Configured limits bound scopes, samples/window, accounted store bytes, admission
batch width and inspected nodes/text. Preflight rejects before retained allocation
or mutation; no tenant baseline is implicitly evicted. Byte accounting covers
fixed record overhead plus scope/sample text, not allocator RSS or snapshots held
externally by callers. Outer providers may set local unavailability without
removing baselines; snapshot reads execute no provider callback, clock read, I/O,
remote refresh, policy or guard decision. Explicit guard wiring is described in
 (§10.27); owns detached export (§10.22). Remote refresh remains.
No runtime latency guarantee is introduced. Adds a separate opt-in array
sampling store (§10.28); it does not reinterpret object-only selectors.

### ✅ 10.27 Explicit statistical guard execution — SETTLED

The native library and published SDKs default to the packaged statistical engine.
With no selection, it holds an empty plan: construction needs no credentials or
windows, performs no eligible detector work and emits no fabricated findings.
An empty plan preserves zero-friction passthrough and return identity; engine
faults remain availability bypasses. Selective repair is never a default.
Explicit activation supplies an
immutable local plan with exact tenant/connection/vendor/endpoint/segment and
selected generations on each side; no provider callback runs in-path.
Call entry retains the plan and mode snapshot. Replacement affects later
calls only. Each selected field compares its frozen baseline to its selected live
window plus one transient scalar from the current complete body, retaining at most
4096 opportunities (oldest live opportunity drops at capacity). This transient
window never updates the store, publishes a baseline or learns from a partial body.
Window/season selection is explicit control-plane input; there is no seasonal
fallback or implicit pooling. Independent sampling and repeated-peeking control
remain host obligations under §10.24; this execution seam is not calibration.

Predeclare 535 hypothesis slots per selected field/comparison across BOTH phases:
one magnitude, up to 512 pooled enum categories, five completeness, six types and
11 string features. Unused slots remain in the denominator, including disabled,
insufficient and non-finding features. At most seven field/comparisons per plan
keep this conservative superset below 's 4096 cap. Enum testing requires an
explicit enum-field opt-in. Supplies actual raw-count Fisher/magnitude p-values;
Floors decide eligibility before dispatch. Optional complete local
reports join the findings; their side must match and skipped evidence stays unavailable.
Original typed detector
reports and tests survive alongside shape findings. Unit hypotheses never produce
patches or conversion permission. Suppressed evidence is retained without drift.

The shared matrix is pure: observe records only; repair degrades drifted
reads and shape writes, halts semantic/unknown writes; strict halts unverified
drift on either operation. Unknown operations are writes. Verified repairs may
proceed in repair/strict; statistical hypotheses supply no verified repair.
Complete malformed bodies retain 's separate rule (strict halt, other writes
bypass, reads degrade); incomplete bodies skip statistical work and parser faults
bypass. Response findings cannot undo upstream work: a response halt retains the
completed original outcome and explicitly reports that dispatch occurred. Request
halts invoke upstream zero times; all other paths invoke it exactly once outside
native recovery. No retry, rollback or write-integrity proof is synthesized.

Containment encloses each evaluated phase; shared readiness/breaker
admits and completes once per call, so successful requests cannot erase consecutive
response failures.
One non-cloneable cooperative work allowance spans phases; the first fault skips
subsequent evaluation and records one bypass per call. Work is charged before
parsing, extraction, transient copying, adapters, testing and routing. Accounted
work is a conservative checked byte/sample/feature allowance, not wall-clock
preemption or allocator RSS. Optional investigation uses 's existing bounded
outbox, a tenant-qualified audit reference and one non-cloneable call reservation
shared across phases. Queue admission cannot change policy. No background work,
network, persistence or inference executes here. Audit composition is supplied by
 (§10.41); SDK, streaming integration and WASM statistical budgets remain
unvalidated.

### ✅ 10.28 Bounded call-level array sampling — SETTLED

`agw_stats::windows::sampled` is a separate opt-in local store; object
selectors retain their interpretation (including literal `*` and numeric names).
A typed selector supplies two strict object-only JSON Pointers: `array` from the
root to one array, and `item` from an item object to a scalar/missing leaf. Both
allow escaped/empty names; neither accepts wildcard traversal. Both must start
with `/`; root-array selection is unsupported in v1. One array level is supported. Every array item must be an object; scalar/null/array items reject
the batch, even if unselected. Other unselected object properties may be containers.
Selected container leaves or array traversal reject. Missing, null or empty arrays
return `NoOpportunity`, admitting no field opportunity. Missing selected item
fields count as missing; explicit null leaves remain null.

Policy identity includes a nonempty caller version (at most 128 bytes), algorithm
`uniform-call-v1`, maximum array length (1–4096), both pointers and
all scope components. Projection is fenced at admission within that identity.
The store encodes selector/policy components as a canonical JSON tuple in its private scope field; public snapshots retain the
original scope/selector. A changed projection in an existing identity rejects.
Each admitted call contributes exactly one field opportunity regardless of length;
the sampling unit is an eligible call, not an item. This weights calls equally,
not all items across traffic. Hosts must submit each call once.

For length n, caller supplies a uniform u64 entropy word. Reject words below
`2^64 mod n` as `EntropyRejected`; otherwise select `word mod n`. This removes
modulo bias without a hot-path random provider, clock or retry loop. Entropy must
be fresh and independent per call, and replaying words reproduces selection.
The sampler cannot verify this assumption. It does not claim cross-call
independence, representative traffic, or unbiasedness for nonuniform entropy.

Full-input traversal/byte/depth/duplicate-key checks precede selection, including
unselected items. For sampled admission, `inspection_nodes` also charges pointer
steps/object-entry searches and array item eligibility visits across the batch.
The canonical encoded field must fit the 4096-byte scope-component cap.
Array-length, scope, sample, byte, batch and traversal limits reject the entire batch atomically, preserving published baseline identity.
Publication/readiness/generation/vocabulary rules apply unchanged. Exact
scalar values, precision authorization and caller timestamps survive. Sampling
grants neither numeric projection nor repair permission. No export, refresh,
transient guard integration or pooling is provided.

Sampled snapshots expose immutable descriptive windows with policy metadata.
Their significance adapter requires explicit independent/exchangeable-call and
comparable-population attestations; absent attestations return unavailable with
no tests. The predeclared family is passed to unchanged; applies its
normal materiality gate. Existing detector-specific denominators and floors still
apply (completeness uses admitted calls, enum uses string-bearing calls, unit uses
authorized numeric calls). Statistical eligibility is conditional on host sampling
and comparison design, not a production independence/calibration claim.

### ✅ 10.29 Precision-budgeted unit projections — SETTLED

`agw_stats::windows::projection` adds explicit local numeric materialization.
Existing Disabled/ExactInteger authorization and `Window::unit` retain
behavior. New `Projection::Decimal(Policy)` requires a validated policy; only
`Window::projected_unit` and `projection::compare` use it. No guard, export,
refresh, wire audit or numeric payload conversion is authorized.

A policy fixes ExactOnly or Approximate(Budget), a nonzero caller precision-source
revision and hint (Absent, Observed or HighPrecision), an explicit unsafe-integer
permission and resource limits. Absent is Observed only when the caller attests
that this revision had no precision hint. Observed represents an explicit false
hint; HighPrecision always forbids projection, including small I64/U64 leaves.
The tree alone cannot prove hint provenance: callers must propagate the selected
frozen contract revision/hint, never infer absence from number magnitude/variant.
All policy fields participate in identity. Existing scope/generation admission
rejects changed policy; comparisons require identical policies. New generations
must be explicitly published and never adopt old live novelty as a baseline.

ExactOnly admits a finite f64 only when its exact binary rational equals the
original decimal rational. Approximate requires finite nonnegative absolute and
relative f64 budgets, interpreted as their exact binary rationals. Negative zero
budgets normalize to positive zero. Acceptance is inclusive:
`abs(binary - decimal) <= max(absolute, relative * abs(decimal))`.
There is no division or arbitrary epsilon near zero; either bound may admit the
conversion. For decimal zero the relative bound is zero. The original token is
parsed as an integer coefficient times a power of ten, and f64 bits are decoded
as an integer significand times a power of two. Bounded integer cross-products
prove error and budget comparisons; formatted f64 decimal text is never an oracle.

Plain unsafe integers, and mathematically integral decimal/exponent forms outside
±(2^53−1), require the separate unsafe-integer permission even when exactly
representable. Unrepresentable variants and retained plain integer tokens wider than the i64/u64
model always refuse, regardless of permission.
Invalid JSON numeric lexemes, overflow/nonfinite results, nonzero underflow to zero,
protected precision and exhausted checking bounds make the whole unit window
unavailable. No difficult numeric sample is dropped. Missing/null/string/boolean
opportunities retain 's separate semantics. Signed zero is preserved, including
negative exponent-form zero; its error is exactly zero. Nonzero subnormal f64
results are allowed only if the exact/approximate error test passes.

Hard limits cap token text at 4096 bytes, significant/coefficient digits at 1024
and absolute decimal exponent at 1024, before any arbitrary-size power/product.
Configured limits may be smaller. Checked work charges bound coefficient parsing,
powers, shifts, products, comparisons and subtraction before execution; shared
window work and output-byte budgets reject the complete materialization. Exact
rational provenance is not required to be reduced. Scratch integers have bounded
bit width from these caps; output space is precharged before strings/records are
allocated. Invalid configuration is rejected at policy construction; original
store batch/sample/scope/byte quotas remain atomic. Failed projection does not
alter lossless samples, readiness or immutable reference snapshots.

Every successful materialization carries policy authorization, original numeric
tokens, projected f64 values, exact absolute and relative error fractions and
whether the conversion was exact. Relative error for original zero is unavailable;
absolute error is zero. `projection::compare` retains both materializations with
the original detector result/hypothesis; inferred units are not repair proof.
No clocks, providers, I/O, production calibration or integrated/WASM latency claim.

### 10.30 — standalone invariant definitions and local versions

The bounded typed DSL represents numeric relations, inclusive ranges, extensible
string enum closure, numeric ordering, cross-field sums and cross-endpoint
approximate equality. Compilation resolves unique caller binding names to slots
and selects a specialized pure evaluator per rule. Runtime takes borrowed scalar
slots, allocates nothing, and returns pass, violation, enum addition or unavailable.
Exact i64 inputs are widened to i128 for sums/differences; absolute tolerance is
explicit. Missing/wrong-type inputs are unavailable. Time ordering requires
caller-normalized integer epochs; no string-to-number coercion exists. Enum
addition never implies an invariant violation or authorization to write.

Caller-owned local stores key immutable histories by vendor, operation and contract
version. Explicit version selection, customer pins, overrides and disables produce
immutable compiled snapshots. Controls refer to existing rule IDs; replacement
rules keep those IDs. Rejected publications/controls are atomic. Configured scope,
version and rule bounds never evict live histories silently. Static borrowed slots
are supported; compilation/store/learning are administrative allocation paths.

Offline complete profiles yield observed inclusive ranges and extensible vocabularies
as candidates only. Marginal observations cannot prove arithmetic or meaning, so
learning never activates candidates or authorizes repair. Bounds constrain fields,
vocabulary, identifiers, bindings and rule work. No guard wiring, policy, audit,
remote state, correlation/freshness service, production safety or latency claim.
Owns integrated violation handling; owns cross-endpoint acquisition.

### 10.31 — explicit native invariant execution

Plans may attach frozen request/response selections. Binding order is
retained by the snapshot; object-only JSON Pointers are compiled administratively.
Scope vendor/operation must match the trusted call plan; the host owns tenant
isolation and explicit contract-version selection. No runtime store access.
Lossless i64 and bounded strings project directly; arrays, missing fields,
containers and other numeric representations remain unavailable, per rule.
Incomplete captures never produce invariant findings. Duplicate-key/resource/parser
faults use the existing call-level containment; one bypass preserves the original
outcome and skips later evaluation. Defaults use an empty statistical plan; detection requires explicit windows.

Each violating rule yields semantic evidence with its ID, actual value, computed
expected value/bounds/operator and configured tolerance; enum additions yield
separate informational `enum_expanded` evidence. Unavailable rules are retained
explicitly and never reported clean. Passing rules do not prove write integrity.
The shared policy matrix remains authoritative: observe never blocks; repair reads
degrade loudly, writes/unknown halt; strict opt-in halts all findings (§4.2).
Mixed findings retain the strongest policy decision. Request halts invoke upstream
zero times; response halts retain the completed original attempt without claiming
rollback or pre-dispatch protection.

Per-phase generated v1 audit drafts use open change kinds and metadata for rule
IDs, actual/expected evidence, source versions, controls, unavailable counts and
severity. They retain byte identity, never claim repair or write integrity, and
are accompanied by loud warning/status headers for findings. Status retains the
strongest phase decision when other rules are unavailable. Publication and final
call-wide audit composition remain host responsibilities. No new wire class or
frozen schema change. Native benchmark scope includes parsing, projection,
evaluation, policy and draft construction on a fixed six-form both-phase fixture;
a local p99 <100 µs gate is not a production or WASM guarantee.

### 10.32 — workflow-scoped cross-endpoint consistency

Cross-endpoint quantities correlate only within explicit tenant, connection,
vendor, workflow, entity and stable business revision. Concept, unit, endpoint
operation/side/field and contract version qualify evidence. Hosts own this
attestation; equal marginal profiles do not prove quantity identity. Bounded
complete paired profiles with varied exact integers yield candidate rules only.
Explicit selection fixes rule/contract versions, absolute numeric tolerance,
maximum age and maximum inter-observation skew. No inferred rule activates itself.

A bounded caller-owned observation store atomically records batches using a
non-waiting local try-lock. Monotone sequence and nondecreasing UTC observation
time fence each qualified source. Missing values invalidate prior numeric values.
Capacity, contention and poison return administrative errors without partial
publication or eviction. Immutable workflow snapshots are acquired outside the
request path; guard execution takes no store lock, network or other-cell callback.

The current endpoint replaces its own cached value for the check, including
missing/unsupported values. A stale sequence/time cannot replace newer evidence.
Both exact integer observations must share scope/quantity/unit/contract and be
nonfuture, within maximum age and inclusive timestamp skew. Partial, stale,
skewed, future or unsupported pairs are unavailable without semantic findings.
A confirmed mismatch uses 's named semantic policy/audit path: observe
reports; repair reads degrade and writes/unknown halt; strict remains opt-in
escalation. Request halts dispatch zero times; response halts retain completed
original outcomes. No repair, retry, rollback or write-integrity claim.

Per-phase drafts retain endpoint values/times/sequences, correlation scope,
concept/unit, selected versions, time/numeric limits and unavailable reasons in
open metadata. Current observations are returned for explicit host publication
after the call; request-path execution never mutates the shared observation store.
The host invalidates failed/incomplete captures or advances revision rather than
silently reusing obsolete values. Snapshot isolation permits concurrent workflows
and endpoints observed minutes apart without waiting for peers. No automatic
acquisition, identity inference, conversion, remote delivery or production timing
claim; the local integrated benchmark includes a fixed paired-endpoint fixture.

### 10.33 — local invariant qualification and opt-in activation

The bounded background controller owns one exact tenant/connection/vendor/operation/
contract scope. Candidate sets and their bindings are immutable versioned inputs.
Authorization names allowed DSL families, candidate version bounds, stage/call-mode permissions,
customer-defined sample/coverage/rate limits and expiry. Business ranges and all
relationships require an explicit attestation to the exact candidate definition;
cross-endpoint attestation must additionally identify concept/unit/revision semantics.
No authorization means no activation. Default engines and customer modes stay unchanged.

Replay receipts identify candidate, evidence, policy revision, stage and independent
training/holdout datasets. Hosts attest independent calls, disjoint training/holdout,
complete comparable populations and complete negative evidence. Counts include all
eligible, evaluated, known-valid, false-positive, known-invalid and missed-invalid
samples. Missing/unavailable evaluations reduce coverage; they never count as passes.
Integer cross-products compare inclusive parts-per-million coverage floors and
false-positive/missed-violation ceilings. Zero denominators defer. Thresholds are
customer supplied, never production defaults. Receipts are append-only and bounded:
capacity rejects new work rather than evicting negative evidence. Duplicate IDs are
idempotent only for identical contents. A conflicting duplicate or full receipt
buffer retains one bounded fault receipt, freezes promotion for that controller
instance and invalidates prepared publications. Contradictory receipts reject promotion.
All historical adverse evidence for an unchanged definition survives policy revisions
and version-only relabeling. Every current candidate/policy/stage receipt participates; expired or
incomplete evidence cannot be dropped to improve the decision. Replay and receipt
attestations are a trusted administrative host boundary, not cryptographic proof.

Stages advance at most one step per accepted publication: shadow, reads, blocking
writes. Each next stage needs fresh independent stage-specific evidence from the
prior stage; permissions are explicit. Shadow retains findings/audits but cannot
halt; reads applies existing policy only to reads; blocking applies existing policy
to all operations. Other findings and explicitly selected customer rules keep their
existing policy. A production dataset/calibration attestation with measured counts
is required for blocking; synthetic tests cannot supply it. Passing never proves
write integrity and no stage authorizes repair.

Preparation compiles a complete replacement outside calls. Publication checks the
controller revision, prior generation and evidence/authorization expiry at commit
under exclusive administrative
ownership; failures retain the prior Arc generation. Calls retain immutable mode,
plan and qualification receipts. Per-phase drafts retain qualification evidence
for both local and paired activation sources, including distinct generations and
paired findings on the opposite phase from publication. Customer pins prevent
upgrades, disables persist,
and overrides take precedence via explicit reconciliation. Customer edits revoke
pending work; new receipts use the current policy revision. Revocation, evidence
expiry or newly negative evidence freezes escalation, records requalification and
retains existing semantic protection; it never silently demotes an active blocking
rule. Customer-directed disables/overrides may deliberately remove protection.
An enforcing generation cannot be automatically replaced by a fresh shadow set
or a policy removing its authorized modes; an explicit customer plan change is
required. No autonomous rollback to weaker enforcement is authorized by this initial policy.

The host owns scheduling, complete replay, authorization authenticity and connecting
published selections to its compiled native plans. No guarded call awaits the
controller, replay, registry, database or LLM. No hosted distribution/persistence,
automatic activation by default, production calibration, repair or final audit delivery.
Still owns background peer acquisition;
its read-only authority, deadlines and invalidation handoff remain open.

---

### 10.34 — explicit two-layer native call execution

`agw_guard::gated` connects bounded request/response captures, local detector
reports and selective repair to the shared `agw-policy` matrix. One context,
one permit and one cooperative work allowance cover the call. Hosts supply
local detection/proposals and independent payload verification; these callbacks
must charge work before expensive work and must never synchronously perform I/O.
A proposed patch is committed only after verification succeeds; false retains
original bytes and applies unresolved-drift policy. Errors, panics, invalid or
incomplete evidence bypass before policy, with one call-level bypass count.
Observe never verifies/applies patches or blocks, but records admitted findings.

A request halt dispatches zero times. A response halt retains the completed
upstream outcome and makes no rollback claim. A response fault overrides the
final policy status to bypassed, preserves original response bytes and retains
any verified request patch already sent. The returned audit v1 draft and headers
share the call mode, final decision and reason, with side-qualified patch paths.
`metadata.agindow_policy` reserves request/response reason, decision and fault,
dispatch status and the once-per-call bypass count. Applied audit patch paths
are side-qualified; the patch header carries a compact selective field count.
Payload verification alone never asserts `write_integrity`; idempotency and
call-wide write proof remain separate work. Malformed-body policy follows §4.4.
No default-engine switch, streaming buffering, remote acquisition, statistical
evidence composition, audit publication/storage or SDK wiring is added.

Exposes an explicit packaged-guest playground entry point for one
committed synthetic CRM mechanical rename. Exact reviewed input bytes admit
trusted migration identity; the existing gated repair engine edits the field
key and independently checks the reviewed target bytes before returning
`repairApplied`/`verified` and before/after evidence. Untouched field bytes remain
identical. No upstream operation occurs. Caller-supplied mappings, schema
acceptance and confidence alone confer no authority. This path is never installed
in ordinary Guest::begin or SDK guards; the default has no selective repair.

### 10.35 / — structured feedback and durable recovery measurement

`agw_guard::feedback` exposes the exact §7 taxonomy and serializable six-field
drift feedback. The explicit outcome adapter uses `drift_blocked` for
repair-mode write halts and `drift_halted` for strict halts (409). Degraded calls
and malformed-write passthrough may carry `drift_passthrough` as additive
notification metadata, preserving upstream status/body. Clean, observed, repaired
and availability-bypassed calls carry no policy feedback. No HTTP service or SDK
is installed by this library.

Hints name admitted field paths. Only confirmed shape identity supports a rename
instruction; unavailable expected forms require authoritative schema review.
Semantic/unknown and mixed evidence require verification/human review, never an
automatically safe conversion. Response-side feedback warns that dispatch already
occurred and forbids blind replay. Dates, types, scalar values and conversion
factors are never invented. The audit ID is percent-encoded as one URL path segment;
the link identifies an artifact, without claiming retrieval is deployed.

A caller-owned tenant-scoped bounded tracker measures distinct blocked calls,
retried calls, and verified task recoveries keyed by tool/audit ID. Recovery rate
is recovered/retried (unavailable with no retries). Only host-attested task success
counts; failure/bypass does not. Duplicate reports are idempotent and unknown IDs
or capacity rejection are explicit. Hosts rotate measurement windows and supply
correlation and success verification off-path. This executes no retries and makes
no production recovery or write-integrity claim.

Settles durable production measurement and freezes `recovery_event` v1 as
the cross-cell message. One envelope carries `kind: blocked | retry | outcome`,
the original `block_id` (the policy block's audit ID), `tool`, deployed `surface`,
caller-owned `window`, `feedback { version, arm }`, an optional `cohort` on blocks
and an explicit `retry_id` plus optional `outcome` on later events. Retry and
outcome evidence is linked only by those identities, never inferred from payload
values; measurement never performs a retry. Tenant identity is never in the body:
an authenticated producer credential digest maps to exactly one tenant, readers
use a separate tenant-bound digest map, and routes stay disabled when their map is
absent. `feedback.arm` (`structured`, `unstructured`, `control`) is assigned by
the deployed surface when the block is recorded and copied unchanged through the
chain; the registry never assigns, randomizes or switches arms. Customer
authorization and a predeclared cohort precede any production comparison, which
must retain the same policy protections.

A cohort is declared before collection and is immutable: exact window,
`environment: production | fixture`, verifier identity and provenance, sample-size
and uncertainty method, safety/violation metric and comparison protocol. A
production cohort requires a bounded customer authorization reference and must
not be created from synthetic traffic; a fixture cohort cannot carry an
authorization reference and is always labeled non-production. The reference
identifies the authorization, not credentials or payload values. An empty
predeclared cohort remains available for its configured retention from receipt;
purge cannot remove it merely because collection has not started.

Only `outcome.status = verified_success` with nonempty `verifier` and
`verifier_provenance` counts as recovery. `failed`, `bypassed` and `unverified`
never do: HTTP 200, a verified payload repair, a bypass and the mere absence of
another block are insufficient. A non-success outcome that claims a verifier is
rejected, and a success whose verifier or provenance differs from the cohort's
predeclared values is a visible mismatch, so provenance cannot be changed after
collection begins. `policy_violation` is an independent optional safety
assessment and never changes recovery attribution.

Admission is idempotent by `(tenant, event_id)`. An identical replay is
`duplicate`; a replay with different admitted content is a `conflict`. Explicit
block and retry identities are each admitted once, so a second event for the same
block is count-neutral. Unknown, cross-tenant, mismatched, expired or reordered evidence is
rejected rather than guessed, and each rejection is retained with its reason and
counted as a collection loss. Reordered evidence (outcome before retry, retry
before block) is therefore visible; a corrected redelivery must reuse the explicit
identities. A retry or outcome must repeat the block's window, tool, surface,
feedback version and arm or is a `mismatch`; a retry timestamped before its
block, or an outcome before its retry, is also a `mismatch`, and evidence
timestamped after block expiry is `expired`.

Event bodies are capped at 16 KiB and cohort bodies at 32 KiB. Measurement
retention is 1–3,650 days, default 90. A block and its chain expire at the
earlier of the block's event time and receipt time, plus retention; delayed
outcomes and retries are accepted until that instant and then rejected as
`expired`, and a future-dated timestamp cannot extend retention; an event time
further ahead than retention from receipt is rejected as invalid. A non-success
outcome may be upgraded once by a later verified success; a verified success is
immutable and can never be erased by a later failure. Duplicates never increment
counts. Purge is explicit and only removes expired rows.

A window counts each original block once as blocked, once as retried when any
explicit retry exists, and once as recovered when a verified success exists.
Segments are exact `(cohort, environment, tool, surface, feedback version,
feedback arm)`; collection losses are grouped by reason. `retry_coverage` is
`retried / blocked`, `recovery_rate` is `recovered / retried` and
`policy_violation_rate` is `policy_violations / policy_assessed`; a zero
denominator yields `null`, never zero. Distinct `failed`, `bypassed`,
`unverified`, `pending`, `unretried` and `expired` counts remain visible.
Fixture cohorts never appear as production traffic. A production report is
descriptive: it proves no improvement by itself, and insufficient or missing data
leaves the epic's effectiveness criterion unmet.

The registry exposes opt-in authenticated `POST /v1/recovery/events`,
`POST /v1/recovery/cohorts`, `GET /v1/recovery/windows/:window` and
`GET /v1/recovery/cohorts/:id`. Admission and reporting are background/off-path
only: no exporter, registry, telemetry or guard failure may change dispatch,
policy or a completed original outcome. Stored bodies are limited to this closed
envelope — no credentials, payloads or customer scalar values. A rejection is a
measurement loss, not a guard error. No deployed feedback surface or production
qualification cohort exists yet, so makes no production recovery claim.

### 10.36 — explicit local write-protection controls

Retains 's default shape passthrough. `failClosed` defaults to true;
false additionally degrades unresolved unknown writes in repair mode. Semantic
writes remain closed, strict ignores the flag, verified repairs retain their
normal decision, and reads/availability/malformed handling are unchanged. Mode
and protection are loaded from one atomic at call entry; reloads affect future
calls only. Native control requests require a nonempty bounded actor and explicit
UTC timestamp, return an audit receipt, and return a warning on every false
request, even when strict ignores it. Hosts persist receipts off-path.

The console's local banner projection is tenant-scoped and bounded. A host
adapter supplies accepted control receipts with monotonically increasing
revisions and authoritative cumulative detected-write counts for the active
opt-out revision. Counts cover both phases once per call and include blocked,
observed and repaired drift. Old epochs and decreasing counts are rejected;
control retries are idempotent. Repeated false controls and mode changes retain
that epoch/count. Restoration followed by opt-out starts a new zero-count epoch;
protection restoration removes the banner. Strict opt-outs remain visible with
an explanation that strict ignores them. The standing warning explicitly states
that semantically drifted writes may return 200 and persist wrong data, while
explaining that repair semantic protection remains mandatory. Text is escaped
when rendered. This supplies local control/rendering seams, not HTTP hosting,
a durable audit sink, event delivery or a deployed dashboard.

### 10.37 — local replay safety and explicit native dispatch

The opt-in native replay store is tenant-bound, bounded by 1–10,000 entries
(default 1,024), and keys writes/unknown operations by
`(connection, idempotency_key)`. Keys are 1–256 visible ASCII bytes;
connection/tenant identifiers are nonempty, at most 256 bytes and contain no
controls. Missing/invalid keys are deterministic 400 envelope errors, before inspection or dispatch in
any mode. Reads do not enter the write store. Full unexpired capacity is a
configured 429 admission limit; no live/terminal entry is evicted to make room.
Inspection warming/faults still bypass within the admitted call and cache the
result. No synchronous remote dependency is introduced.

One key owns execution; concurrent followers wait without holding the global
map lock and return the same `Arc` outcome, including upstream errors and policy
blocks. Different keys execute independently. Recursive same-thread access
returns `InProgress`. A caller panic propagates unchanged; reservation cleanup
publishes `Indeterminate` and wakes followers, without re-executing a potentially
completed write. No original result is fabricated for an indeterminate call.

Retention is monotonic, starts at completion/indeterminacy, defaults to 24 hours
and is configurable from nonzero through 7 days. Running reservations never
expire. Expired records may execute again: expiry, restart, separate stores and
multiple hosts are duplicate-write risks unless upstream/durable coordination
covers them. Local replay safety is not global exactly-once execution. Hosts
supply bounded owned outcomes; the store bounds metadata/entry count, not opaque
caller-owned heaps or streaming-reader state.

Immutable forwarding metadata retains the exact supplied key for every explicit
host attempt and claims the configured key header. Durability is an optional
opaque 1–128-byte visible-ASCII hint, applied verbatim through an explicitly
supported header binding; absent support is reported `Unsupported`, never called honored.
No persistence guarantee is inferred from forwarding a hint or HTTP success.
There is no automatic retry/backoff policy (§10.5 remains deferred).

the `execute_once` composes this boundary with existing request/response
policy and availability containment. It caches original upstream outcomes,
terminal audit/feedback, and any effective response repair bytes; it neither
reserializes nor copies unmodified payload captures. Replays skip inspection,
verification and dispatch and preserve the original audit identity. Cached policy
feedback states that the key is terminal; changing arguments under that key does
not reevaluate a block. Reads have no terminal-key or retention claim. Audit proof
records the key but never infers `write_integrity`. Low-level guard APIs remain
available; hosted/SDK owners must explicitly wire this opt-in boundary.

### 10.38 — human review and bounded confidence feedback

Strict halts unresolved drift on every operation. Only independently verified
repairs with supplied confidence ≥0.99 may proceed; clean calls and availability
bypasses remain distinct. `review_case` v1 carries the original strict blocked
audit and complete admitted phase reports/proposals/assessments/verification,
with replacement bytes retained as hex. `review_resolution` v1 embeds that case
and actor, UTC timestamp, note, contiguous revision and one of accept_repair,
accept_original, reject, defer. Scope includes tenant, connection, vendor,
operation, contract version and caller-bound exact repair identity. Case ID/time
are the original audit ID/time. Messages are bounded to 128 KiB, 64 levels and
50,000 nodes; scope/actor IDs are bounded to 256 characters, notes to 4096,
phases to two, changes to 4096 and proposals to 64. Oversized evidence is visibly
unavailable, never presented as complete. No publication failure releases a halt.

A tenant-local console queue retains immutable evidence and revision history;
terminal decisions close it, defer remains open, and accept_repair requires a
proposal. Revisions are bounded to 16; the final revision must be terminal.
Receipts route human intent to the host without executing, waiving
verification, rewriting call audits or replaying terminal keys. Queue admission
is idempotent; conflicting evidence/revisions and tenant mixing are rejected.
Retention is bounded by item/revision/byte limits; no unacknowledged evidence is
evicted. Hosts authenticate reviewers and persist/deliver receipts off-path.

Per-connection SLA is frozen on admission and starts at the original audit UTC
time, including delivery delay. Defer/replay/config changes never reset it.
Polling yields stable breach records until acknowledged; closing after breach
retains the alert. Poll scheduling and external notifications are host-owned.

Registry resolution ingestion validates embedded evidence, scope and revision
history, rejects conflicting replay, and counts each terminal label once. A
separate local prior per exact scope starts at 0.5 with strength 2; accepted
repairs add success, reject/accept_original add failure for proposed plans,
and defer is neutral. The posterior (1+successes)/(2+successes+failures) is capped
at 0.989 and is descriptive evidence only. It does not rewrite immutable versions,
customer pins or detector/verifier evidence, and cannot independently satisfy
strict's 0.99 gate. Persistent delivery, execution, production calibration and
final statistical audit composition remain separate work.

### 10.39 – — customer alert delivery and notification channels

Eligible consolidated Tier 1 alerts (§10.23) produce one logical notification,
emitted and delivered entirely off-path. No inspection, policy, response or
dispatch path awaits emission, delivery, retry or acknowledgement; delivery
faults never throw into a guarded call and never alter its outcome, bytes or
status. Delivery behaviour is mode-independent — it consumes published alert and
audit evidence and never re-evaluates a call — and the containment/readiness
boundaries of §10.9/§10.10 apply unchanged. For the hosted product this service
is the host that §10.38 leaves to "poll scheduling and external notifications".

The channel-neutral envelope carries references only: a stable notification ID,
tenant and scope, finding class, UTC time, occurrence counts, the consolidation
identity and bounded audit references (§3.3; retrieval is the fetch-back
surface). It never carries customer payload bytes, replacement tokens or
credentials. Consolidated-summary updates mutate the same notification identity
and must not send an additional notification. The registered `notification` v1
family is closed and frozen through the `contracts/` procedure with explicit
bounds on counts, identifier lengths (≤256 bytes) and total size; validation
rejects payload and credential-shaped content, and envelope field changes are
contract changes.

Subscriptions are tenant-scoped and tenant-authenticated: channel, scope, instant
or digest, enabled state and destination, each revision immutable in history.
Destination secrets exist only by reference — never in the envelope, ledger,
audit, diagnostics or logs. Conflicting or stale updates fail without mutation;
disabled channels receive nothing. Subscription and ledger retention is bounded
and explicit; no unacknowledged delivery state is evicted, and terminal delivery
evidence is retained until provider reconciliation.

Delivery is at least once and never claims exactly-once. Delivery identity is
(notification ID, channel, destination revision); retries reuse it and never
create a second logical notification. Acceptance is recorded only after the
receiver (webhook) or provider (email) accepts. Retries are bounded with backoff
and jitter; exhaustion reaches a terminal dead-letter state carrying the audit
reference; an endpoint crossing the configured consecutive-failure threshold is
disabled with an owner-visible notice. A missing or disabled destination retains
an explicit not-delivered state — nothing is dropped silently — and no delivery
state is cleared without reconciliation.
Delivered/retried/dropped/expired/dead-letter/disabled counters are exported;
diagnostics never contain payloads or credentials.

Webhook destinations activate only after a challenge handshake proves ownership,
and are HTTPS-only with redirects rejected, bounded timeout and bounded response
read. Every attempt refuses loopback, private, link-local, metadata and
rebinding-prone resolutions, connecting only to the validated address set.
Deliveries carry `X-Agindow-Notification-Id`, `X-Agindow-Delivery-Id`,
`X-Agindow-Timestamp` (UTC) and `X-Agindow-Signature`: HMAC-SHA256, lowercase
hex, over the timestamp and the exact request bytes, using a per-endpoint
secret. Verifiers enforce an explicit replay window; retries re-sign fresh
timestamps. Rotation verifies old and new secrets during a bounded overlap while
only the new secret signs; revoked secrets stop verifying. The customer-facing
verification recipe is documented.

Email delivers only to verified recipients through the fixed provider HTTPS
endpoint, with redirects rejected, bounded timeout and a stable provider
idempotency key reused on retry. Rendered content is the envelope's bounded
subset — kind, scope, severity, counts, audit or review links — never payload
data. Provider-accepted sends are deduplicated within the provider's idempotency
window; uncertain outcomes after it become an explicit requires-action state,
never blind retries or cleared delivery state. Digests coalesce per subscription
window and, like 's budget, may not flood; bounces and complaints suppress
the recipient; one-click disable turns the subscription off with an immutable
audit record.

Fixed-call replay and inbound vendor webhooks remain separate work, and public
delivery claims require hosted evidence, not local fixtures alone.

#### authenticated administration decision (2026-10-23)

The private identity boundary may mount bounded tenant/environment notification
reads and revisioned commands over the – owner stores. Authorization
precedes owner-store access: reads require evidence permission and commands
require notification administration. Each command binds actor, expected global
revision and idempotency identity; exact replay returns the immutable receipt,
while changed replay or stale revision refuses without mutation. Reads expose
opaque destination references, verification/suppression, acceptance state and
redacted key metadata, never addresses, payloads, ciphertext or plaintext.

Webhook registration reuses the existing HTTPS, resolution, SSRF and challenge
policy; email activation reuses verification and suppression. Test-send and
redrive are notification-only commands with a one-minute local rate window.
Redrive admits only explicit dead-letter or expired delivery states and never
replays an upstream customer write.

Recoverable signing material is AES-GCM envelope encrypted with its opaque
reference as authenticated data. A random data key encrypts each secret; a
managed server-held port wraps that data key and records an explicit wrapping-key
version. Rewrap changes the wrapped data key, not the secret ciphertext. Plaintext
is returned once by successful issue/rotation and otherwise resolves only through
the channel host. The default hosted identity profile supplies neither the
administration owner nor wrapping keys, so it returns unavailable and makes no
hosted-delivery claim.

### 10.40 — Audit artifacts, recurrence and durable retrieval

Every call publishes exactly one artifact conforming to the closed §3.3 schema —
clean, observed, degraded, repaired, bypassed or blocked; a bypass is evidence
too, so nothing filters by decision. Publication is a local, best-effort
pipeline in `agw-audit`: validate honesty, stamp recurrence, append to a bounded
append-only store, enqueue once for a host-driven sink outbox and project the
the tech stack metrics as OTLP/JSON with no network I/O. Nothing awaits a
sink, database or network; the only outcome that stops a publication is local
store refusal, and callers may ignore that value. The in-path guard never
depends on publication; owns composing statistical evidence before it.

Honesty rules the closed schema cannot express are enforced before storage or
publication: `patch.applied` and `patch.byte_identical` are complementary (a
repair always breaks byte identity); an applied patch names at least one changed
field; a `repaired` decision requires an applied patch; `occurrences` and
`first_seen` are present or absent together; tenant metadata, when present, is a
bounded identifier (≤256 bytes) that must match the row tenant.

Recurrence is local and says so: one signature is `(class, side, vendor, sorted
changes)` per tenant and process, bounded (default 4,096, hard ceiling
1,000,000). A first observation stamps `first_seen` and `occurrences = 1`;
repetitions keep `first_seen` and increment the count. A full map refuses the
new signature without evicting history or mutating the drift — the artifact
still publishes, unstamped. Callers own any cross-instance merge; this is not
the corpus and no global count is claimed.

Publisher recurrence updates commit only after storage admits the artifact.
Refused publications do not change counts, first-seen timestamps, signature
capacity or recurrence-drop counters; retrying valid evidence counts once.

The local store is append-only and bounded (default 65,536 records, cumulative
64 MiB, 90-day retention; ceilings 1,000,000 records and 512 MiB; retention
1 hour–3,650 days). Identity is the artifact ID: an identical repetition is an
idempotent duplicate that is neither re-stamped nor re-delivered, and different
bytes under one ID is a conflict. Identity is decided on the received evidence
with the publisher-owned recurrence fields normalized away, so replaying the
unstamped original against the stamped record is a repetition. Capacity or byte
ceilings refuse the new record rather than evict evidence; expiry hides a record
from retrieval, and the explicit purge is the only removal path. Publication
into a bounded outbox (default 1,024, ceiling 65,536) is at-least-once and
host-drained; a full outbox drops delivery, never storage.

Direct publisher drains check a sink's declared tenant against the publisher
before any sink I/O, including artifacts without tenant metadata. A mismatch
returns zero delivered and retains all queued evidence and delivery counters;
unscoped sinks such as NDJSON remain supported.

the registry stores artifacts durably in a Postgres table whose trigger
rejects every `UPDATE`, `DELETE` and `TRUNCATE` outside
`purge_expired_audit_artifacts(at)`. Appends go through `append_audit_artifact`
under one advisory lock per `(tenant, id)`: an identical replay returns
`duplicate`, a conflicting body raises, and retention is fixed per record at
append. One record is capped at 1 MiB. Enterprise export returns one bounded
window (≤10,000 artifacts, ≤366 days, oldest first) and never includes expired
evidence.

`GET /v1/audit/:id` is disabled until tenant-bound reader credentials exist
(`AUDIT_READERS`, SHA-256 digests exactly like `PROFILE_READERS`). It accepts one
percent-decoded path segment (≤512 bytes, no controls or slashes), authenticates
before lookup, answers unknown or expired IDs with 404, serves `no-store`,
treats contradictory tenant metadata and storage faults as 503, and never
returns another tenant's record. Retrieval never rewrites a record. Now
composes statistical evidence before publication and forwards artifacts through
this publisher; native delivery and registry ingestion are specified below and implemented by
. The embedded WASM/SDK drain boundary and hosted qualification remain separate work.

#### embedded drain decision (2026-10-20)

Publication is disabled unless Node `Options.audit` explicitly supplies a tenant.
The guest owns a FIFO bounded by count (default 64, maximum 256), serialized bytes
(default 512 KiB, maximum 1 MiB) and individual artifact size (128 KiB). Capacity
rejects the newest artifact and increments a saturating drop counter. Disabled
calls allocate no audit drafts. Publication composes once at completion,
including request halts and availability bypasses that reach a live guest.

The host configures an unpredictable per-instance UUID namespace and supplies
bounded tool identity and UTC timestamp through `GuestCall.audit_context` before
completion. IDs are namespace + monotonic counter, fixed before enqueue and
unchanged on redelivery. `audit_peek` exports the head without removing it;
`audit_ack(id)` removes only an exact head ID. A failed sink retains that head.
`drainAudit(sink, limit)` is an explicit caller-owned schedule; there are no guest
network calls, timers, threads, automatic host scheduler or persistent storage.
The host validates the frozen audit v1 schema before invoking the sink, serializes
drains and acknowledges only after sink success. The bounded queue is in WASM
linear memory; the release module size still must remain below 5 MB.

A retired guest is never re-entered, including by drains. Its in-memory pending
artifacts are lost; a call that cannot enter/complete the guest cannot enqueue
there. Host status exposes drain unavailability instead of claiming delivery.
A fresh SDK instance is the explicit recovery boundary. Host persistence, replay
and duplicate/conflict convergence belong to the sink; no durable guest survival
or exactly-once delivery is claimed. Event buffering/auditId remain independent.

#### delivery/ingestion decision (2026-10-20)

The delivery envelope is the unchanged frozen `audit_artifact` v1 JSON body;
there is no wrapper or new schema. Native senders POST to `/v1/audit` over HTTPS
with a tenant-bound bearer token and `X-Agindow-Tenant`. The route is disabled
(503) without `AUDIT_PRODUCERS`, a SHA-256 digest map using conventions.
Missing/invalid tokens return 401; missing/unknown/mismatched tenant headers or
contradictory artifact tenant metadata return 403. Bad JSON/schema/honesty, IDs that cannot be addressed by the bounded retrieval
route, or expired evidence return 400; body size over 1 MiB returns 413. Authentication
and bounded validation precede archive access. Configuration/storage faults are
503. Responses are `no-store`. Credentials never enter artifacts or process argv.

The idempotency key is `(authenticated tenant, artifact.id)`. Archive JSONB
structural equality defines exact duplicate evidence (object key order is not
identity); no recurrence stamping occurs at ingest. New records return 201,
duplicates 200, and conflicting evidence 409. Only 200/201 acknowledge durable
storage. An ambiguous failure replays the same body; a 409 is a terminal delivery
loss and is counted separately. Other failures retry, including authorization
failures so operators can repair credentials without losing queued evidence.

A native spool is bound to one tenant and destination. Hosts allocate a distinct
path per destination and exactly one process owns that path. Within a process,
opening the same canonical path twice is rejected. Drains require exclusive
mutable ownership; shared hosts serialize via their own mutex, so concurrent
drains cannot send a queued artifact twice. No library starts a thread or timer.
Publisher-to-spool transfer occurs only in a host drain; the publisher front is
removed only after durable spool admission. A blocked/full spool retains that
front, and the bounded publisher outbox counts rejected new deliveries.

Spools persist v1 state by private temporary file, file fsync, atomic rename and
directory fsync. A single private temporary snapshot slot is reclaimed on reopen
after a crash. They bound serialized state including retry metadata: defaults
1,024 entries/64 MiB, ceilings 65,536 entries/512 MiB. One artifact is at most
1 MiB. Admission rejects new entries without evicting old ones. Retry reservations
are persisted before sending; defaults 8 attempts and 1-second exponential
backoff capped at 1 hour (configurable 1..100 attempts, 1 ms..1 hour initial
backoff). Retry exhaustion drops only the head after its deadline. Each tick has
an explicit item limit (hard ceiling 1,024); a failed head stops the tick. Crashes
may consume an attempt or replay an acknowledgement, never reorder the spool.
Filesystem failures stop delivery and retain the unacknowledged publisher front;
after an ambiguous filesystem commit the spool must be reopened before retry.
NDJSON uses the same drain helper and unchanged framing; external sinks must
tolerate at-least-once delivery. Hosts provide nonblocking/timeout-bounded sink
I/O and nonblocking admission to shared publishers: the library neither
interrupts arbitrary blocked writers nor makes guarded calls wait for staging. Durable counters expose deliveries, retries,
drops, admission rejections and conflicts; OTLP/JSON also reports cumulative
measured drain duration and count. Guard outcomes never await or depend on drains.

### 10.41 — Composed per-call statistical audit evidence

's per-phase records are composed into exactly one frozen audit v1 artifact
per call; no schema or wire class changes. Remains the only decision owner,
Owns structured errors and owns publication/storage/retrieval. The
composer neither runs detectors nor repair, alters policy, verifies patches,
persists nor awaits delivery. A host calls it once per completed call and may
pass the result to; recomposition from the same `Outcome` is deterministic.

The artifact's `operation`, `mode` and `decision` come from the frozen call. The
decision is `bypassed` when any phase bypassed (availability/fault). Otherwise
a maximum phase decision of Halt maps to `blocked`, including when an
earlier malformed request passed through under and the response halted.
Only without a halt does a non-observe, non-strict malformed write map to
`bypassed`; remaining phase decisions map Degraded→degraded, Repaired→repaired
and Pass→ok. The same precedence governs `detail`: a halt remains
`drift_halted` rather than `malformed_body_bypass`. `detail` is `availability_bypass`,
`malformed_body_bypass`, `drift_halted`, `drift_passthrough`, `repair_applied`,
`drift_observed` or `clean`. `drift` is present only for actual phase evidence:
the union of report/invariant/cross-endpoint changes deduplicated by
`(path, kind, from, to)`, with class the maximum of shape < unknown < semantic
and side request, response or both when both phases contributed. A clean call
states no drift, a bypassed phase contributes no evidence, and reused drafts
never supply either.

Composed statistical evidence lives in the reserved
`metadata.agindow_statistical` object (`schema: "agindow_statistical/1"`), not in
a new schema version. Composition replaces `agindow_statistical`,
`agindow_policy`, `agindow_invariants`, `agindow_cross_endpoint`,
`agindow_activation` and `agindow_enum_growth` from the caller's draft so
per-phase adapters cannot leave contradictory duplicates; when a statistical plan
is frozen, its tenant becomes the artifact tenant. The subtree carries identity
(tenant/connection/vendor/operation/segment), per-field scope
(side/path/window/generation/segment) with readiness and baseline/live sample
counts, per-phase evaluation (evaluated, skipped, fault, malformed, investigation,
warning, invariant/cross-endpoint counts and the test summary), the final policy
projection, findings, explicit limits/truncation and provenance. Existing
per-phase invariant/cross/activation provenance is retained under an optional
`invariants` object. No credentials or payload values are copied.

One finding entry is emitted per admitted (materiality-eligible) test: `side`,
`path`, `detector`, `class`, selected `generation`, `signal`, sample counts,
effect, p-value, family test count, materiality, a typed `reason` and a
`hypotheses` array. Detectors are `unit`, `enumeration`, `completeness`,
`type_consistency`, `charset`, plus `invariant` and `cross_endpoint`. Reasons are
`magnitude_shift` (Wasserstein/normalized/PSI), `new_value` (exact value, count,
frequency, `window_first_seen`, material flag), `completeness_delta`,
`sentinel_shift`, `type_consistency`, `charset_shift`, `format_emerged`
(recognized format plus anchored pattern) with before/after rates where the
detector supplies them, invariant `violation`/`enum_addition` (actual/expected)
and cross-endpoint `divergence` (concept/unit/tolerance/age/skew). `hypotheses`
currently holds `unit_conversion` entries with baseline/observed medians, ratio,
factor, tolerance and `automatic_conversion: false`; it is an array so future
alternative hypotheses remain typed. The artifact-level `hypothesis` is set only
when exactly one distinct hypothesis kind exists across the call. A unit
hypothesis is diagnosis data for : it never authorizes conversion, repair or
a write-safety claim. `window_first_seen` is the local selected window only;
persistent first-ever history and recurrence stay 's.

Finding order is deterministic under input permutation: side (request before
response), then path, detector, reason kind and the complete serialized entry as
the final tie-breaker. At most 256 findings and 1,024 drift changes are retained
per artifact; findings also share a 256 KiB serialized budget.
`truncated.findings` counts omissions from either finding bound,
`truncated.finding_bytes` is true when the byte budget caused at least one, and
`truncated.changes` counts omitted changes, beside the fixed `limits`. Truncation
never removes provenance, evaluation counters, the decision, the mode or the
policy projection, and never raises or lowers the semantic class. Bounds are
local composition limits, not store or transport limits.

A reused draft contributes only its non-reserved metadata and identity fields:
`patch`, `confidence`, `hypothesis`, `note` and `proof.idempotency_key` are
cleared and `proof.write_integrity` is false. `patch` is populated only from
caller-supplied `Applied` entries that were independently verified and name
fields; fields are side-qualified (`request:/path`, `response:/path`), byte
identity is always false, and the combined patch is selective only when every
applied entry is. Diagnoses, bypasses and response-side detections therefore
never fabricate a repair or write-integrity proof. A call whose final decision is
`repaired` therefore needs the host's verified `Applied` entries; without them
's honesty validation refuses the artifact rather than inventing a patch.

`Outcome::publish` composes and forwards the artifact to 's `Publisher`,
returning its non-blocking publication value or `None` on local refusal. It never
changes the upstream outcome or bypass computation and never awaits a sink.
Native and shared-fixture validation cover the reserved subtree; the frozen
`audit_artifact` schema is unchanged. Hosted delivery, durable/registry
ingestion, final call-wide orchestration, WASM/SDK emission and production
calibration remain //.

#### dashboard aggregation decision (2026-10-23)

Dashboard aggregation is off-path and accepts call-identity evidence under an
exact organization/environment/connection scope. Admission deduplicates by
organization, environment and call ID; an exact replay is idempotent and
different evidence under the same identity is a conflict. Calls, findings,
repairs, blocks and bypasses remain distinct measures: call counts are never
derived by summing findings. Queries use hourly UTC bins, at most 31 days and a
bounded optional connection filter; duplicate filters and partial boundaries are
refused. Late and out-of-order admission updates its owning bin without creating
another call.

Every result declares source, unit, production cohort, sample count, ingestion
watermark and qualification. Percentiles require the admitted raw latency
distribution. Recovery exposes eligible cohort, numerator and denominator; a
missing denominator is unavailable, not zero. A sampled-only source marks total
traffic measures `total_traffic_source_unavailable`; it cannot infer all calls
from audit findings. Billing incidents remain a separate source. The
private HTTP route authorizes before source access and is disabled when the host
injects no source. Aggregation and dashboard availability never participate in
a guarded call.

### 10.42 — embedded Node distribution

The local `agindow` source package ships both JS module formats,
declarations and the release Rust/WASM guest. It is not published to npm.
Wrapper API §12.7 records its deliberately bounded public surface: generic tools
and the default statistical guest, with explicit local statistical windows selecting
. Dedicated platform/client/HTTP adapters remain. No remote registry,
LLM, network, timer or filesystem access occurs during a guarded call.

The guest's split `statistical::Session` preserves one immutable CallContext,
plan, cooperative work budget and readiness permit between request and response.
The existing native dispatch API uses that same lifecycle. Node retains all
upstream values/errors; only bounded inspection copies enter the guest. Host
monotonic timestamps advance the guest readiness clock. A trapped guest is
retired permanently, and its unfinished state is abandoned. A replacement SDK
instance creates a separate linear memory; no automatic call retry occurs.

Any loading/evaluation/disposal fault produces at most one local bypass event
per call and preserves the original outcome. Unsupported inspection values,
accessors, cycles and limits also bypass. Opaque HTTP responses/streams are never
consumed. Request policy halts dispatch zero times; response halts retain the
completed result or original error as cause and warn against blind replay.
No repair, idempotency proof, fabricated audit ID or write-integrity claim is
exposed. Owns guest audit draining; owns full feedback/observability.

Mode precedence is explicit > AGINDOW_MODE > observe. A runtime configure update
changes subsequent calls; both phases of an in-flight call keep its prior mode.
Strict calls while loading also fail open; callers await ready explicitly.
Missing configuration starts without detection, and missing/failed WASM never
prevents upstream work. No telemetry data leaves the process without host code.

The installed archive must be <5,000,000 unpacked bytes including WASM, both
module formats, declarations and licenses. Measures 5000 alternating
paired individual steps after 1000 warmups in its local agent tool-selection/
observation fixture, gating nearest-rank p50(guarded minus control) <100 µs.
Timing includes planning, dispatch, host/guest transitions, copies/allocations
actually repeated per call and observation handling. Model/vendor network time
is excluded; cold loading and both individual distributions are reported
separately with raw samples and environment. This default-engine measurement is
not statistical latency qualification: owns that full embedded matrix.

### 10.43 — explicit HTTP fallback boundary

Node `guardRemote` and Python `guard_remote` are explicit opt-in HTTP APIs;
the Node API is asynchronous and Python also supplies a worker-thread variant. It submits the §2
execution envelope once; it never resubmits an operation through AGINDOW.
Any 500–599 response, refused/reset connection, timeout, or invalid transport
response invokes the supplied original callable once and emits one `bypass`
event. The callable's result/error is returned by reference. A successful
upstream HTTP result is retained without consuming its bytes. A 5xx carrying a
recognized owned status header retains that already completed upstream response
and emits one response bypass; it never dispatches that completed operation again.
Only validated six-field 409 feedback matching the tool and mode is a policy
halt. Other HTTP responses remain HTTP results, including non-policy 4xx results.

Writes and conservative unknown operations require an idempotency key at setup.
The exact same envelope, including that key, reaches the original callable on
fallback. The callable/credential binding must enforce upstream deduplication:
a reset can occur after dispatch, and transport fallback alone cannot prove
exactly-once execution. No automatic blind retry or durable replay is claimed.
The timeout covers response headers and bounded 409 feedback consumption;
streaming successful responses retain their normal upstream lifecycle.

### 10.44 — Node release-WASM timing decision

The supported Node runtime (recorded verbatim per report) release
`wasm32-unknown-unknown` measurements use individual calls,
1,000 warmups and 5,000 alternating paired guarded/unguarded samples per case.
Quantiles are nearest rank (`ceil(n*p)-1`); added latency is each paired
subtraction. Reports include both distributions, mean and population standard
deviation. A negative paired difference is retained. Cold module/instance setup
and file size are reported separately; the runtime compiles a fresh guest at
setup. All repeated capture, parsing, allocations, window adaptation, five
detectors, significance, routing, policy, boundary crossings and result disposal
stay in the warm timer. Vendor/network/model work is excluded from both sides.

The default-engine SDK gate retains paired added p50 <100 µs. Explicit
statistical SDK calls have guarded p50 <200 µs and p99 <2,000 µs, including both
phases. These are distinct suites; statistical work is not subtracted to make
the SDK budget pass. The <10 µs availability target applies to native;
embedded normal-bypass measurements retain the same target and failures remain
visible. Trap/retirement disposal and fresh recovery are reported separately,
not mixed into steady bypass samples. Regression ceilings use a frozen fixture
baseline only after an accepted qualifying report; absent qualification is
reported unavailable, never treated as an invented baseline or a passing claim.
Python, Worker and sidecar remain separately qualified. CI retains raw reports
even on a failed absolute budget. No production SLA follows.

### 10.45 // — explicit HTTP and agent surfaces

The sidecar binds loopback HTTP and an optional mode-0600 Unix HTTP socket.
Existing paths are never unlinked at startup. Environment configuration supplies
an allowlisted `connection/tool` route, upstream method, operation and bound
headers; clients send connection handles, never third-party authorization headers.
Inline envelope fields outside §2 are rejected. Raw `args` tokens become the
upstream body without JSON re-encoding. Write keys are forwarded unchanged;
redirects, automatic transport retries and environment HTTP proxies are disabled.
The API is §2 HTTP, not a TLS interception proxy. A client must already support
that envelope or explicitly adapt its tool callable.

Both HTTP hosts use the identical Rust statistical Session and immutable mode
snapshot. Local selections are explicit setup-only inputs. The statistical engine
is the default; without windows it emits no findings and has no selective repair. Complete bounded
responses can be inspected; streaming, unknown-length and oversized responses
retain the stream and bypass unavailable inspection. Policy halts return the
six-field 409, while internal faults preserve upstream bytes/status with
one owned bypass header whenever an upstream outcome remains available. Response
halts retain a bounded, process-local completed outcome exposed through an
`X-Agindow-Outcome` recovery handle. The SDK may retrieve it from the same origin;
recovery failure never replays a known completed write. Eviction/restart loses
local retrieval; no durable replay or write-integrity guarantee follows.

Native registration is an immutable startup record exposed at
`/v1/registration`; `/healthz` reports readiness and call count. This is local
registration, not a claim that a remote control plane received it. Sidecar
artifacts/outcomes are FIFO bounded to 128 entries and 16 MiB. Hosted recovery
is isolate-local, credential-bound, 64 entries and 16 MiB. The native overhead header sums active host/engine work and excludes awaited
network intervals. Cloudflare production clocks advance only after I/O, so they
cannot measure synchronous engine CPU work inline. The Worker therefore sends
`X-Agindow-Latency-Ms: unavailable` and `X-Agindow-Latency-Clock: unavailable`
by default. An explicitly local-only `AGINDOW_LATENCY_CLOCK=workerd-local`
configuration enables numeric active intervals and the matching clock label;
it excludes incoming upload, upstream and body-network waits. Never use that
local configuration for a deployed Worker. Platform post-request CPU metrics
remain separate from this header; no I/O or timing workaround is inserted into
the engine to manufacture a production clock. This explicit availability value
is the HTTP-host exception to §6's numeric latency header.

The workers-rs build is pinned to 0.7.5 to retain the repository's Rust minimum.
Scoped key digests bind tenants to explicit routes and plan windows. Remaining
quota headers describe an isolate-local window, labeled
`X-Agindow-Quota-Scope: isolate`; they do not promise global distributed billing
limits. A separate administrator endpoint probes the configured registry using
an mTLS Fetcher binding. Registry probes never run in a guard decision path.
Production certificate deployment, global quotas, production qualification and
public endpoint availability require independent operational evidence.

The MCP surface uses the official SDK's stateless HTTP transport with explicit
origin allowlists, tenant authorization, 64 KiB request bounds and read-only
store callbacks. Its five tools are `lookup_contract`, `query_drift`,
`fetch_audit`, `simulate_repair` and `coverage_report`. Simulation never dispatches
and only previews explicit confirmed local sibling renames; coverage never
infers production safety from absent observations. Agent docs cross-reference
`agindow.brand/1` and contain runnable examples. `/openapi.json` signs the exact
published tool schemas with Ed25519 over `agindow.canonical-json/1`: UTF-8 JSON
with recursively sorted object keys, JSON primitives, no non-finite numbers.
Consumers supply an independently trusted verification key. Public MCP registry
listings and deployed authorization/storage are operational prerequisites, not
claims made by local handler tests.

### 10.46 — embedded statistical host/guest payload boundary

The Node SDK serializes each captured request or response once and passes the
result to the release guest as UTF-8 JSON bytes, not as a JavaScript string.
The guest parses and classifies those bytes exactly as before; malformed bytes
remain §4.4 classification input, never a thrown client error. The wasm-level
call ABI is unchanged — a pointer/length pair — so the Python host, which
already writes UTF-8, requires no new encoding and shares the identical guest.
No public wrapper API, wire schema, audit artifact, payload cap, timing
boundary or decision threshold changes. This is an internal boundary decision,
not a new observable behaviour: the same parser, five detectors, window
adaptation, significance, routing, policy and evidence disposal run.

Host capture keeps a fast path only for strings that require no JSON escaping;
strings with quotes, backslashes, control characters or lone surrogates retain
`JSON.stringify`/rejection semantics. The fast path exists because measured
64-KiB attribution found host capture at 28 µs p50 and each guest phase at
~76 µs; numeric lexeme formatting dominated the guest phase and was fixed
separately. Budgets, the 351-case matrix and the retained samples are unchanged
by this decision; still had to prove the case-by-case tails after the
change.

### 10.47 — asynchronous drift corpus ingestion

An explicit host background exporter accepts bounded, detached drift evidence;
it never runs a Queue or database call from the engine. The host ticks its
1-second flush window (configurable 100–5,000 ms), or flushes at 100 messages.
At most 1,000 contributions / 4 MiB including in-flight messages are retained;
a full buffer refuses new evidence and reports loss, without changing a call.
Queue sends split at 100 messages / 240 KB; failures retain evidence for retry.
The registry Queue consumer waits up to 5 seconds or 100 messages, retries five
times and sends poison messages to a DLQ. These are local defaults, not
production-calibrated throughput claims.

The host binds tenant, vendor, operation and selected contract version, rather
than trusting event metadata. Before enqueueing, the background projection
validates frozen `drift_event` v1, drops metadata/connection/payloads, hashes
paths and change values, and retains only allowlisted change kinds. Unknown
kinds become a hash. Canonical sorted changes define a SHA-256 fingerprint;
input change order, event IDs, timestamps and selected version do not alter it.
These hashes minimize evidence but do not guarantee anonymity. Trusted scope
labels must be schema identifiers, never customer values or credentials.

Postgres atomically inserts a tenant/event-hash receipt and updates the private
corpus by `(tenant, vendor, operation, drift_fingerprint)`; §10.49 adds an
explicitly consented shared aggregate by `(vendor, operation, drift_fingerprint)`. Redelivery counts once; conflicting
reuse of an event ID is rejected. Distinct events increment occurrences and
preserve the earliest event time as first_seen. Each receipt records the wire
version and selected contract_version; an input occurrences value is never
trusted. No public frozen message schema changes.

Raw samples are excluded from HTTP ingress and default Queue export. A separate
library admission requires a trusted enterprise tenant policy with retention
1–30 days; samples are tenant-private and expire at the earlier of event time
and receipt time plus retention. Expired samples cannot be read and retries
never extend expiry. Hosts must run purge and apply private storage/access
controls before enabling this library path. Credentials must be scrubbed by the
host even with consent. This is not automatic payload capture or R2 delivery.

### 10.48 — structural contract push cache

A tenant/connection has a signed complete structural snapshot, with monotonic
revision, immutable version history and complete signed pin history. Ed25519
verification, shape normalization and digest checks run during background
installation only. Snapshots cannot omit or replace an installed version or pin;
replay is idempotent, conflicting or older revisions are refused. The default
snapshot bound is 128 versions / 4 MiB. Publishing public keys and customer pin-authority keys are separate and
tenant/key scoped. A trusted publisher cannot mint a customer pin; absent pin
keys allow no pin-bearing snapshot.

The synchronous guard lookup reads only a bounded local LRU (128 connections by
default). It performs no crypto, database access, network access or promise work.
Cold/missing/unresolved lookups return observe with no guessed contract. The host
may separately load the per-connection Durable Object in a background task;
remote failure leaves the local selection usable. DO state persists before
publication; registry updates push, and a failed push notification is retried by
the host using the same revision. There is no polling timer. Signed pins always
win over newer compatible versions, including across push updates. LRU eviction
requires background reinstallation and never silently reinstates an unsigned pin.
The measured target is local synchronous lookup p99 below 1 ms; remote background
refresh latency is outside the request budget and is not production-qualified.

### 10.49 — private corpus and explicit sharing boundaries

Every tenant has a private corpus. No consent means no shared contribution or
shared read, including for evidence predating this migration. Legacy
aggregates are rebuilt privately from retained receipts; no historical evidence
is published on opt-in. A host-authorized sharing receipt records a contiguous
revision, actor, UTC timestamp, consent and named reciprocal benefit. Enabling
sharing permits prospective minimized schema/hash contributions and access to
shared aggregate evidence. Revocation retracts the tenant's shared contributions
and shared access atomically; its private history, including failures, remains.
Retries and concurrent ingestion serialize against tenant consent changes.

Tenant-bound repositories scope all private reads; SQL RLS additionally filters
non-owner reads by the host-set tenant context. Database credentials and owner
functions are not given to customers. Owners can administer migrations; this
is not a claim that SQL owners are sandboxed. Shared rows contain only schema
hashes/taxonomy, vendor/operation/fingerprint, earliest time and counts. They
never contain tenant/connection IDs, event IDs, selected private contract
versions, raw samples, arbitrary metadata or repair replacement bytes. Hosts
must use schema labels free of customer values. Hashes remain access-controlled,
not anonymized. Actual scalar payloads are inspected transiently on the local
runtime; no customer values are embedded in detector math or exported as model
parameters. Raw retention, if separately opted into, remains tenant-private.
Signed versions and signed pins follow §10.48 and §10.16; consent cannot change
contract selection or silently upgrade a pin.

### 10.50 — outcome corpus, conservative confidence and provenance

Each repair outcome references a tenant-private drift contribution, an exact
plan SHA-256 and a host-bound logical attempt hash. One contribution/plan counts
as at most one attempt. Immutable observation receipts deduplicate by tenant and
observation ID hash; contradictory outcomes on the same attempt are retained,
and any failure is absorbing. A policy violation is also a failed repair. Later
success cannot erase either; evidence has no decay or expiry. Successful outcomes
require an independent verifier kind and a hashed proof reference; customer
payloads and repair replacement bytes are not persisted in this corpus.

A proposal is scored only against the same vendor, operation, fingerprint and
exact plan. Private scoring uses the tenant's attempts; fleet scoring includes
only explicitly shared source contributions and requires current sharing consent.
Confidence is the minimum of detector confidence and the 95% Wilson lower bound
for verified attempt successes, with a bounded human rejection penalty (up to
5%). Review acceptances can reduce this penalty but never exceed the verified
outcome bound. No outcomes means confidence zero; unknown drift is never eligible.
Per-class floors are 0.97 shape and 0.99 semantic; strict floors both at 0.99.

Threshold tuning considers paired assessments of the same fixed cohort with at
least 1,000 assessed calls. It selects the highest tested threshold at or above
the class floor whose observed policy-violation rate is no greater than the
no-repair rate. Recovery is not an input. Missing/unsafe calibration prevents
eligibility, even if the confidence floor is met. These empirical local
assessments do not claim production calibration or statistical equivalence.
Corpus eligibility remains evidence, not guard authorization; independent
runtime verification and the mode/class policy remain mandatory.

Private export is bounded and resumable, includes exact entry/plan identity,
revision and counts, and omits raw samples and other tenants' receipts. Shared
exports contain only consented aggregate evidence. Every scored proposal records
its corpus identity, evidence revision, counts and threshold/calibration cohort
so its confidence is attributable. Revocation changes subsequent shared scoring
and exports; previously exported copies require host governance and deletion.

### 10.51 — shadow review and connection promotion

Strict blocked review_case/review_resolution v1 remain unchanged. A separate
shadow_review v1 carries an original observe audit, explicit dispatched flag and
observed outcome, complete admitted phase changes/proposals, and an optional
resolution with actor, timestamp, note and contiguous revision. Observe evidence
never claims an applied patch or write-integrity proof. The console renders
hypothetical changes beside what actually happened, retains immutable full
evidence and bounds items/bytes/revisions. Approve/reject/defer record intent;
bulk approval applies only when every proposal confidence meets the caller's
threshold. Missing confidence never qualifies. No decision executes a write.

Background review feedback verifies the frozen evidence, derives the exact plan
hash from ordered side/path/kind/target/replacement tuples, and binds it to a
private source contribution with matching vendor/operation/fingerprint/version.
Receipts preserve actor/time attribution as hashes, reject conflicting replay,
retain defer history and count each terminal label once. They update the
separate human penalty in §10.50; they cannot erase execution failures or count
as successful writes. Full replacement bytes stay in the private review case,
not in shared corpus output. Hosts authenticate and durably deliver decisions. The bounded console background
drain retains each immutable revision until transport acknowledgement; lost
acknowledgements replay the same receipt, which the registry deduplicates.

Each connection starts in shadow. An explicit actor-attributed, revisioned
promotion may advance only one step: shadow → reads → writes. It requires fresh,
independent stage evidence, zero policy violations, a named calibration cohort,
minimum assessed call count and threshold-qualified confidence. Write promotion
additionally requires an explicit write consent and prior reads-stage evidence;
connection/tenant mixing, stale evidence and revision replay conflicts fail.
Promotion receipts describe host configuration intent only. Unknown operations
remain conservative writes; existing semantic fail-closed policy, strict mode,
customer pins and independent runtime verification cannot be waived by review.
No default SDK wiring, hosted activation or production qualification is claimed.

### 10.52 – / – — bounded asynchronous investigation

Implementation decision for. Local library/Worker/SQLite evidence is
recorded in; hosted and provider qualification
remain pending. The following specifies the explicit background path.

The cortex Worker owns authenticated admission, a SQLite Durable Object per
tenant/investigation, and Cloudflare Workflows execution. Relay owners explicitly
drain 's volatile FIFO outside calls. A closed `investigation_job` v1
envelope carries tenant, audit/call IDs, stable idempotency identity, creation/
expiry, integer micro-USD ceiling and a pinned price-context ID. No customer
payload travels in that envelope. Admission authenticates a tenant-bound token
digest, validates size/version/scope, commits the immutable job before ack, and
retries workflow creation using the same instance ID. A duplicate conflicting
identity is refused. Accepted jobs persist for 30 days; terminal records retain
audit correlation. Local-only/uncertain jobs can be lost on relay crash; no
end-to-end exactly-once or durable local admission is claimed.

Exporters have one outstanding request, at most 1024 records, 8 attempts with
exponential backoff capped at 30 seconds, 5-second request deadlines and a
10-minute job lifetime. The hosted consumer retries unavailable audit evidence
within that lifetime; unauthorized/expired evidence terminates explicitly.
Audit resolution must match authenticated tenant and original call, contain
unknown evidence and never fabricate absent evidence. Workflows checkpoint
bounded steps, each with at most 3 attempts, and terminate after 5 minutes of
execution or job expiry. Partial evidence and dead letters remain inspectable.
No new callback or dependency is introduced into guard execution.

Paid text synthesis initially supports OpenAI Responses with pinned
`gpt-4.1-mini-2025-04-14`, temperature 0, no tools, images, reasoning, conversation
history, streaming or automatic fallback. A host-maintained, expiring price
record identifies model, currency, version, price-source URL and integer micro-USD
rates per million tokens. No bundled rate is production authority. Input is
bounded plain text and charged conservatively at its UTF-8 byte count plus a
256-token framing allowance; this allowance requires host qualification before
paid dispatch is enabled. Output is provider-limited to at most 2048 tokens.
Integer arithmetic rounds each charge upward. Unknown/stale/unqualified pricing
or modes dispatch nothing. The durable ledger atomically authorizes and marks
a grant dispatched before network I/O; a replay never re-dispatches a consumed
grant. Lost outcomes retain uncertain charges. Retries need distinct grants.
Receipts deduplicate by provider receipt identity; full overruns are retained and
halt further paid work. A bill is never clamped or guaranteed by local accounting.

Vendor evidence is fetched only from host-configured HTTPS public sources with
redirects disabled, deadlines and byte caps. Quotes are exact excerpts, dates
are scored against first-seen, and description-only OpenAPI changes remain
visible. Missing sources are findings. Community/customer reports require
vendor+field+drift correlation and attribution; customer content additionally
requires explicit consent. Reports raise awareness only. Synthesis receives
bounded statistical/invariant/declaration evidence, references admitted citations
only and labels uncertainty/hypotheses. Fingerprint caches are tenant/version
scoped; only explicitly public vendor evidence may be reused across tenants.
Investigation fingerprints include drift class/side and canonical sorted change
identities (path, kind and optional before/after descriptions). Only the digest
is retained in plan checkpoints; differing descriptions cannot reuse an answer
merely because the field and change kind match.
No diagnosis authorizes repair or weakens semantic fail-closed policy.

Versioned `finding` v2 enrichment records evidence kind, exact fingerprint,
field/contract scope, provenance, expiry, uncertainty and recommended action.
Registry revisions are append-only, reversible and expire; direct observations
rank above vendor declarations, which rank above hypotheses. Community awareness
cannot become classification. Tenant-private findings never cross scopes.
Completion content uses transport, carries citations and partial status,
and selects digest for reads. Repair-now creates an expiring actor/scoped request
for elevated authority; it neither executes a write nor grants authority. Public
incident content is rendered only from expressly public, anonymized evidence.

Acquisition uses an explicitly enabled, versioned tenant/connection recipe:
fixed HTTPS GET endpoint, host-owned credential reference and attested workflow,
entity, revision, concept, unit, endpoint/field, contract version and causal
sequence. Jobs use the same durable admission/execution machinery. No inferred
URL, mutable method, redirect or credential in jobs/audits is allowed. Each fetch
has a 5-second deadline and 64-KiB complete-body limit; attempts and total
lifetime share workflow limits. Complete responses must carry exact matching
identity/revision/unit/version and scalar integer text, UTC time and sequence.
Publication is an atomic host-owned admission with stale-sequence fencing,
including explicit missing/unavailable invalidation for failed attempts. Failed
or late publication cannot relabel old evidence as current. Recipe updates or
disables invalidate pending authority; no acquisition activates a rule or pin.
Local fixtures and dry runs do not constitute hosted/provider qualification.

---

## 11. The benchmark — sequencing and timeline ✅ SETTLED

**The "ship a public benchmark in 14 days" recommendation was wrong.** Correcting it here rather than leaving it in the GTM as a live instruction.

### Why 14 days was wrong

A credible public benchmark needs: a drift-case corpus, an eval harness, **a violation-rate metric** (per §10.2), comparably-measured baselines, and reproducibility. You also cannot benchmark a repair engine that doesn't exist yet. For reference, ToolMisuseBench is 1,000 tasks of research artifact. A thin benchmark is *worse than none*: it becomes a target, and if a competitor runs it and beats your number, **you have published a benchmark your competitor won and now defends.**

### The confusion to avoid

**The public benchmark is not the hedge against FlareCanary's Fixer.** The hedge is the **write-integrity position**: you block semantic drift writes, Fixer doesn't. A benchmark *amplifies* a position — it doesn't create one. Publishing numbers before the product can honor them just arms a competitor with your own yardstick.

### What the timeline should actually be

| When | What | Audience | Purpose |
|---|---|---|---|
| **Day 14** | **Internal** eval, CI-wired. ~40 drift cases across the 7 universal classes. 3 arms: no-repair baseline, naive full-payload repair, selective repair. Metrics: recovery + violation rate. | us | Unblocks engineering decisions. Nobody outside sees it. |
| **Before results** | Publish **methodology + reproducible harness + public development corpus + held-out protocol**. Invite 2–3 external validators to co-sign the methodology and run the no-repair / naive baselines. **No Agindow score is published or implied.** | ecosystem, validators | Establishes the category framing and scoring rules before anyone's numbers. |
| **Day 60–75** | Repair engine + violation metric solid. Publish **first official results**. | press, analysts, YC | Establishes the yardstick with numbers the product can honor. |
| **Day 90+** | Recurring public leaderboard. Third parties run their own agents against it. | ecosystem | The category standard. |

### Amendment — methodology may precede results ( split)

This section originally bundled methodology, first results and the leaderboard into one Day 60–75 publication. That bundling is superseded: is split into (methodology, reproducible harness, public development corpus and held-out protocol) and (first official results, Day 60–75; recurring leaderboard, Day 90+).

**Why.** The constraint above — publishing numbers before the product can honor them arms a competitor — binds *results*, not *framing*. The methodology is the yardstick (drift classes, the dual recovery + violation requirement, invariant-violation scoring, and the repo-and-PR vs runtime category split); publishing the harness discloses it whether or not an Agindow score is attached, so withholding scores does not withhold the yardstick. Withholding the methodology instead leaves the first published Agindow number to a third party running the harness against a library seam or the default engine. The moat is unchanged and is not in the benchmark: it is the proprietary corpus and production repair quality. A competitor adopting this scoring has conceded the framing that violation rate, not recovery alone, is the metric.

**What does not change.** No Agindow recovery or violation number is published, claimed or implied under; the launch scores are validator-run no-repair and naive-repair baselines only, so the board is never empty. First official results still wait until a publicly obtainable configuration runs detection plus repair end-to-end — not from fixture-supplied findings, which is all the internal eval measures today — and the leaderboard still waits for at least two committed external entries and 2–3 validators. The corpus splits into a public development set and a held-out set committed by version and hash only; official scores are verified against held-out cases that are never published, and benchmark cases are synthetic fixtures only — never derived from any tenant's private corpus evidence.

**The 14-day number was always achievable — just as an internal instrument, not a public claim.** That's the whole correction.

### What to do instead in the first 14 days

The highest-value inoculation asset is ready on **day 1** and costs nothing: **the artifact demo.** A live `unit_conversion` hypothesis with a blocked write and a named diagnosis convinces a buyer in 30 seconds. A benchmark number convinces nobody who can't run it.

| Audience | Wants | Asset | Ready |
|---|---|---|---|
| **Buyer** | Proof this works on my stack | Live artifact demo | **Day 1** |
| **Investor / YC** | Trajectory + defensible method | Internal eval + stated roadmap | **Day 14** |
| **Press / analyst** | Reproducible public numbers | Published benchmark | **Day 60–75** |

Don't hold the buyer asset hostage to the press asset.

---

### 10.53 – — credential evidence and operator cutover

Credential classification is additive to §7. It never changes upstream status,
retryability, bytes, dispatch or the policy decision. Host-supplied connection
and version identifiers are references, never credential values or hashes.
Response analysis accepts bounded, complete evidence only; bypasses, faults,
truncated/malformed bodies and 429/5xx yield no credential diagnosis. Generic
WWW-Authenticate error directives distinguish rejected and explicitly expired
credentials. Quota markers on 403 are independent of credential validity.
Repeated 403 requires a prior success of the same operation, argument identity
and version before it can be labelled inferred scope reduction. Ineligible
evidence never advances that history or consumes a recovery. Credential
inspection accepts at most 128 response headers, 4096 bytes per auth challenge
and 128 bytes per quota marker; exceeding these bounds yields no diagnosis and
does not alter forwarding.

`credential_event` v1 is closed. All references have finite bounds; there is no
metadata or arbitrary payload slot. Expiry/revocation require provider or host
authority. Scope reduction is inferred and requires count/window evidence.
Rotation is host-declared with prior/current version references. Recovery names
the prior failure event and cannot claim unknown evidence. Timestamp is UTC Z.
Consumers reject unknown properties before ingestion; native consumers use
`agw_audit::credential_wire::parse` for the conditional rules that typify does
not express. String-shape screening rejects known credential formats; hosts
must supply identifiers, because arbitrary opaque strings cannot mathematically
be distinguished from secrets. No response header or raw body is serialized.

Health has independent cause and state. Quota does not advance the failure
ladder. Explicit expiry/revocation is terminal; otherwise three consecutive
credential failures is the default failing threshold. Success recovers once.
The in-memory projection is tenant-owned, bounded, and performs no I/O.
Sequence numbers are host-assigned per binding; duplicate/stale deliveries are
ignored. A bounded timestamp batch can be canonicalized in place with
`apply_window`; single-event delivery uses host ordering. Arbitrary historical
failure/success sequences cannot be reconstructed after a window is finalized.

Cutover receives opaque active/candidate handles and runs only on explicit
operator commands. Production captures one immutable binding handle before
execution and never replays an in-flight call. Only dual-key providers qualify;
unknown capability refuses cutover. Read probes are the default; writes and
unknown operations require per-probe consent and a host-carried idempotency key.
Probes bypass statistical collection entirely and carry a canary audit marker.
Promotion requires completed successful probes. Quota and other inconclusive
non-credential HTTP outcomes consume the bounded retry budget without diagnosing
a bad credential. Credential failures, probe execution faults, timeout, explicit
abort or exhaustion without a complete successful plan abort verification.
Unsupported receipts distinguish atomic invalidation from unknown capability.
Hosts drive all deadlines and overlap ticks. Candidate failure reverts
future dispatch to active; the candidate is quarantined for that cutover ID.
Clean overlap produces one local retirement signal. The host persists state
before releasing its old binding; distributed/external exactly-once delivery is
not claimed. Restore cross-checks the plan, recorded probe results, completion
flags, counts and lifecycle/window coherence; missing or invalid restart state
freezes to active with unknown. The host restores matching opaque bindings and
protects its persisted snapshot integrity; snapshots are not authenticated by
this local library.

The offline readiness report consumes bounded NDJSON credential events and
host-curated capability facts. It never reads a binding or calls a network.
Missing, stale or disputed capability facts yield unknown; dual-key with any
mint mode is verifiable, atomic invalidation is flag_day, and no replacement is
unsupported. Reports expose sources and observed operations without inferring
granted scopes, expiry or credential age. `observed_since` is a lower bound.
Report health follows the latest host-declared credential rotation and excludes
late outcomes on superseded versions. Recent mixed-version evidence without
rotation authority is unknown. Equal event timestamps follow host export order;
quota never changes credential health, including at the start of an export.

### 10.54 — reviewed vendor semantic declarations

Vendor declarations are optional background explanation evidence under §4a.
The registry admits bounded, cited facts for field semantics, unit conventions,
enum evolution and versioning, with exact API/resource/field applicability and
explicit limitations. Documentation review dates describe a snapshot, never a
promise about another version or a particular customer write.

Submissions default to tenant-private. Host-authorized independent reviewers
approve or reject immutable revisions; contributors cannot approve themselves.
Only explicitly public submissions whose citations are attested public vendor
sources can become public. Private customer sources cannot be promoted. Public
exports omit tenant/contributor/reviewer identities. Reads require host-bound
tenant authority; tenant IDs in request bodies never establish authority.
Revision heads fence stale submissions; rejected or pending replacements do not
replace the last approved revision. Withdrawals suppress approved entries.
The local store is bounded and volatile; hosted IAM, persistence and hosting
remain explicit obligations, not implied by a local catalog export.

Enrichment returns the original detection by reference plus matching reviewed
facts. It raises explanatory support from unverified to documented, never
numerically boosts calibrated detector confidence, changes classification,
authorizes repair, overrides pins or weakens policy. No matching declaration,
disabled enrichment or an unavailable registry leaves detection unchanged.
No guard imports, calls or waits for this registry. Community contributions use
the same citation/review boundary; source attribution is evidence, not proof.

### 10.55 — background verified drift incident accounting

The local meter consumes existing audit evidence outside call execution. Full
Tier 1 detection, policy and availability behavior never depend on entitlements,
a metering acknowledgement or registry availability. An authenticated host
attests complete capture and delivery, and supplies an immutable connection,
distinct finding and UTC window of at most 31 days. Tenant authority comes from
the host, not audit metadata. A non-observe `repaired` audit must have admitted
drift, an applied patch and independent write-integrity proof to count. Proposals,
halts, blocks, passthrough notifications, bypasses, detection-only findings and
truncated captures are unbilled; feedback or a native draft alone is insufficient.

One tenant/connection/vendor.operation/finding/window is one verified drift
incident. Repeated applied repairs contribute included work, never another unit.
The original idempotency key deduplicates logical writes across operations
on the same connection; without a key the original audit identity must survive
internal retries. Key identities and audit identities occupy distinct namespaces.
A host-configured operational ceiling (1–1,000,000) saturates included work;
overflow is visible and never adds a charge or changes a call outcome.

Window start fixes the accounting month. A transaction commits immutable audit
identity digests, logical-write fences, incident state and monthly rollups
atomically. Conflicting evidence is rejected; concurrent redelivery is idempotent.
UTC monthly partitions follow. Admission ends 120 days after window start;
identity fences and details expire together, so replay after expiry is rejected
rather than re-billed. Scheduled maintenance drops expired monthly partitions
and retains 13 months of aggregate history. A tenant has at most 100,000 receipts
within the admission horizon. Full capacity is explicit background rejection,
never a request-path failure. Queue batches contain at most 100 messages;
acknowledgement follows durable admission, and storage faults request redelivery.

Launch entitlements follow pricing-research §2/§4: Free has one monthly incident,
one production connection and seven-day cloud history; Pro has two monthly
incidents, five connections and 30-day history; Team has 15 incidents per UTC
calendar quarter, 25 connections and 90-day history. Enterprise requires explicit
agreed terms. Fresh usage projections show allowance, excess, production scope,
retention and the monthly hard cap. Launch overage charges are zero and the cap
is base only, satisfying the stricter bound within the standing 2×-base ceiling.
No per-incident price is enabled. Exhaustion offers an upgrade choice without
changing detection or weakening protection. Unavailable usage stays unavailable,
never zero. Authenticated hosting, refresh scheduling, cloud-history enforcement,
invoicing and production qualification remain separate host obligations.

### 10.56 – — bounded machine identity (2026-10-22)

Approved multi-org extension (October 27, 2026):
supersedes 's per-org operator policy binding for the opt-in hosted profile.
A tenant-free format-2 host policy constrains the platform signer; verified
membership selects deterministic isolated org authority/admission profiles.
First registration durably pins generation without a per-org CLI bootstrap.
Registration IDs carry profile routing hints; signature/tenant/profile checks
supply authority, never the hint. A bounded host directory renews enrolled
unchanged reviews only. Read catalog materialization is local to verified owners;
no directory/identity/database call enters guard. Separate disabled configs and
explicit primary/a smoke retirement avoid legacy imports. There are no existing
customers (October 28 clarification), so no customer migration is required. Coordinates
public guidance; no production enablement or qualification is authorized.

Self-serve admission (October 27, 2026; user approved before implementation):
Adds a separate private platform
signer for verified owner-session intents. The signer and authority both
enforce the installed reviewer policy: exact tenant/Test origin/profile/storage
generation, connection/audience/read tools and immutable version/deadline. A
constrained reviewer key cannot bootstrap multiple rows, change multiple rows,
resume stopped/expired/revoked authority or extend an existing review deadline.
Offline operator reviewer trust remains separate. Renewal cannot create rows.

Public-only intents persist before signing, with actor/request digest idempotency,
rolling quotas and generation/revision CAS. Uncertain commands reconcile before
new mutation; background alarms do not depend on a browser remaining open.
Admission storage is anchored to its original tenant/profile/generation and
immutable policy versions. Empty admission storage cannot bootstrap over an
already-active authority. Migration/recovery requires explicit operator setup;
no manual-secret import or automatic resurrection is supplied. Intent receipts
and tombstones fail closed at bounded capacity without evicting live fences.

Adds owner-only Workloads UI
and public setup. Submitted, admitted, delivered and fresh exchange-ready states
are distinct. Ready requires matching owner acknowledgement, completed recovery,
read-route validation, native issuer preparation and <=30-second freshness.
Public handoff fields are re-viewable; private keys remain customer-side. Only
the customer's actual signed exchange/read demonstrates key possession and dispatch.
Dashboard cookies confer no token/guard authentication. No control/identity/SQL
read enters guarded dispatch. Default feature/backend policy remains unconfigured;
no hosted qualification or / completion is implied.

Approved renewal decision (October 27, 2026; local implementation):
Selects B: a private durable
reviewed-policy authority with a renewal-only background job. Exact canonical
registration digests, signed reviewer admissions, generation/revision fences and
a maximum 30-day reviewed renewal mandate bound unchanged-row renewal. Admission
snapshots remain at most 3600s; expired/revoked authority cannot silently resume.
Lease delivery and renewal I/O stay outside guard execution. Auto mode separates
exchange lease expiry from validation of already-issued tokens under local
TTL/epoch/scope rules. Conservative stale-owner revocation lag is 66 minutes under
the stated clock assumptions; execution disablement is a separate operator action.
The user approved B before implementation. Local compiled fixtures cover these
boundaries; hosted qualification remains pending. Replay/epoch/quarantine
and qualification remain intact.

Self-hosted owner profile (October 27, 2026; local implementation):
route each registered workload to one execution owner. Keep guard proof admission
local. Rotate issuer keys with every owner or WASM replay-state incarnation;
old tokens/descendants return 503 instead of being admitted after state loss.
Persist registration+jti assertion fences only at token exchange, before exposing
a minted token. Bound access/assertions to 300s plus 30s skew, proof fences to
180s/1024 entries and assertion fences to 1024 retained through expiry+30s.
Fresh/lost durable state quarantines exchange for 360s. Keep a monotonic clock
floor and never evict live fences. Recovery uses existing background SDK refresh,
not an in-call IdP or store lookup. Allow 335s for a warm authority and 370s after
empty-store quarantine (360s + 5s retry + 5s exchange). These bounds require an
available authority, prompt local assertion signing and a running SDK scheduler;
sustained partitions, expired host policy or a suspended workload have no finite
recovery guarantee. Cache partitions derive from trusted owner activation, with
1024 proof fences per workload and at most 256 retained native activations per
isolate; saturation rejects without eviction. Workload routing
hints are locators only. No global quota or exactly-once vendor-write claim is
introduced. See the detailed owner/routing/storage migration decision in
. Live qualification is separate.

Decision owner: relay maintainers. This is an opt-in local implementation,
not deployed identity infrastructure. Vendor credentials remain outside AGINDOW.
Authenticated tenant administrators register issuer/subject rules and public
client keys through an injected control-plane registration source; request bodies
cannot create registrations. An identifier selects a registration, never supplies
tenant authority. Hosts own durable storage, authenticated administration and
background JWKS/discovery refresh. No in-path I/O is permitted.

| Surface | Primary credential | Audience | Infrastructure fault | Rejection |
|---|---|---|---|---|
| Hosted guard / SDK remote | DPoP or mTLS access JWT | `agindow:guard` | 503; SDK calls original with one bypass | 401 (invalid), 403 (tenant/scope); no fallback |
| Registry producers | Bound access JWT | `agindow:registry:write` | 503, background retry | 401/403 |
| Registry readers | Bound access JWT | `agindow:registry:read` | 503, retain local snapshot | 401/403 |
| Remote MCP | Bound access JWT | `agindow:mcp` | unavailable tool result | auth denial |
| Loopback/UDS sidecar | OS process/socket permissions | none | existing bypass | OS access denial |
| Embedded SDK / local MCP stdio | process trust, no credential | none | existing availability gate | existing policy |
| Exchange | registered OIDC/JWT-SVID or private_key_jwt | registration audience | 503; setup error initially, background retry later | 401/403, no issuance |

Remote surfaces must explicitly adopt the verifier; this decision does not turn
existing registry, MCP or hosted handlers into deployed auth services. Access
JWTs use EdDSA/Ed25519 only, `typ=at+jwt`, exact configured issuer and `kid`;
required claims: `iss, sub, aud, tenant, environment, connection, tools, iat,
nbf, exp, jti, cnf, agw_act`. Audience is one table value, never an array or wildcard.
Connection is one literal handle; tools are 1–64 distinct literals, no wildcards.
`cnf` contains exactly one of `jkt` or `x5t#S256`. Access TTL defaults to 300s,
maximum 900s; CLOCK_SKEW_SECONDS=30, including expiry edges. Expiry plus skew
is the revocation bound (at most 930s); no individual access-token denylist is
claimed. Signing-key removal propagates at the next host refresh, bounded by
300s cache freshness, with no stale verification. Unknown kid/issuer and stale
keys are unavailable infrastructure, never synchronous discovery or acceptance.
Known bad signature, expiry, tenant, audience, scope and replay are rejections.
Unavailable auth never grants anonymous hosted access: client bypass executes
its own original binding. These map to existing bypass/policy outcomes (§7).

Initial issuer profile: EdDSA/ES256 OIDC and ES256 SPIFFE JWT-SVID, with exact subject
matching (no glob/regex), exact registered audience, exp/nbf/iat checks, and
registered environment/scope ceilings. JWT-SVID accepts its standard sub/aud/exp
claims without requiring iss/iat/nbf, and uses the registered trust-domain key;
optional issuer/time claims are still checked. OIDC requires iss/iat and accepts
optional nbf. JWT-SVID audiences may be a bounded array (at most eight). RS256 providers require an explicit
additional algorithm profile; GitHub-shaped fixtures do not qualify GitHub's
live issuer. SPIFFE X.509 exchange and HTTP Message Signatures are deferred;
Selects only private_key_jwt. Client assertions require iss=sub=registered
client ID, unique jti and TTL <=300s. Current registrations govern all new
issuance immediately; old verification keys may overlap for <=300s, but a
non-current client registration cannot mint. Assertion replay is rejected.

DPoP uses EdDSA, public OKP JWK and RFC 7638 thumbprint, `typ=dpop+jwt`,
`htm`, canonical absolute `htu` excluding query/fragment, `ath` SHA-256 of the
access token, `iat` within 60s plus skew and unique `jti`. MTLS requires a
thumbprint from the authenticated TLS peer, never a client-supplied header.
Replay entries live 180s; live entries are never evicted to accept new proofs.
Full replay state refuses admission with a counter. Default limits: 64 cached
keys (maximum 256), 1024 replay entries (maximum 65536), JWT <=16384 bytes.
Key replacement is host-driven outside calls; expiry/drop/full counters are
observable without credential values.

Legacy keys are explicit migration mode, labelled unbound. A keyless-only tenant
rejects keys before guard evaluation. Bounded overlap <=900s permits migration;
removal criterion is zero unbound usage followed by keyless-only activation.
Legacy issuance/digest/revocation remains. SDK setup never silently falls
back: fallback requires a separate explicit switch and a legacy diagnostic.
A missing initial identity raises AgindowSetupError; refresh failure retains a
still-valid token, then bypasses after expiry, once per call. Acquisition and
refresh run at setup/background time, never while deciding a guarded call.

Delegation proves possession of the parent at exchange, preserves tenant,
environment, audience and connection, selects a strict subset of tools or
shorter expiry, binds the delegatee key, and never outlives any ancestor.
The private `agw_act` claim records ancestor IDs, subjects, tenant, environment, audience, connection,
tools and expiries (no token values). It is an AGINDOW extension, not the
registered RFC 8693 `act` object: scope/expiry restrictions need explicit local
semantics beyond actor identity. Local validation checks every link's
intersection. Default maximum depth 3 (configurable 1–8), actor bytes <=8192,
and 60 exchanges per subject per minute in bounded local state. Full state
rejects rather than evicts a live rate fence. Revocation/expiry bounds apply
to descendants too; removing a trust registration prevents new issuance, not
immediate revocation of previously issued descendants. Distributed replay/rate
state and deployment qualification are host obligations, not provided here.

Required evidence: defect matrices, tenant-A/tenant-B isolation, stolen-token
proof replay, keyless-only refusal, capacity saturation, controlled clock edges,
SDK lifecycle parity and credential-output scans. No new contracts family or
§7 taxonomy entry is introduced.

### 10.57 — Explicit SDK selective repair

Resolved: Node/Python ordinary inspection remains non-mutating by default.
Construction may opt into reviewed tool/side-scoped top-level object-key rename
rules, a migration identity, confidence and a local synchronous independent
verifier over immutable original/patched JSON text. The shared Rust repair
library prepares only targeted raw-span edits. The SDK applies only when the
verifier returns true and mode is repair/strict; observe never mutates. Missing
source preserves identity; collision, low confidence and verification refusal
remain unresolved shape drift. Unresolved shape drift passes through in repair mode under shared Rust policy;
strict halts unresolved matched drift. Existing statistical findings, halts,
malformed results and bypass are never erased by repair preparation. Verifier
faults, async results and unavailable capture use the availability bypass path,
without retries, and retain original upstream exceptions.

Boundaries: 16 rules, one per tool/side, simple 64-character ASCII object keys,
complete captures <=1 MiB, existing.97/.99 thresholds. Opt-in divides the work
allowance across statistical inspection and request/response preparation. No
nested/array/unit/enum transformations or automatic discovery. Facts and business
snapshots refresh outside calls; callbacks must not add synchronous cell, DB or
LLM dependencies. Guest audit FIFO plus host-applied repair is a setup error
until its artifact can accurately cover the combined path; repair events are
local metadata, not durable evidence or persisted write correctness proof.
See and wrapper API §12.23.
