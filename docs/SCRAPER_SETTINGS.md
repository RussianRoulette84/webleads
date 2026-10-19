# Zoom and depth: what was actually measured

Everything here was measured against the running scraper on **20 September 2026** —
70 create-probes and 20 full searches. Nothing is read off the vendor's source or
guessed from its help text. The README carries the summary; this is the record behind
it.

Scraper: `bin/gmaps` (gosom/google-maps-scraper) on `127.0.0.1:8080`.

---

## The answer

| mode | zoom needed? | depth needed? | what the value buys |
|---|---|---|---|
| normal | No — absent, 0, 12, 99 all work | Yes (absent/0 → `422 missing depth`) | depth is the lever: 1→16, 5→40, 10→78, 12→88, 15→108, 18→0, 20→0 |
| fast | Yes, 1–21 only — outside that the job dies and jams the scraper | Yes (absent/0 → `422 missing depth`) | nothing: every depth and every legal zoom returns exactly 20 |

**We send depth 15 and zoom 12.**

---

## Will it accept the job?

Every combination of mode × zoom × depth was posted to `POST /api/v1/jobs` and the
answer recorded. Accepted jobs were deleted at once so nothing ran.

- zoom ∈ {absent, 0, 1, 12, 21, 22, 99}
- depth ∈ {absent, 0, 1, 10, 255}
- mode ∈ {fast, normal}

**70 probes, one rule:** the answer depends on the depth alone.

| depth | answer |
|---|---|
| absent | `422 missing depth` |
| 0 | `422 missing depth` |
| 1, 10, 255 | `201 Created` |

The zoom is never checked when the job is made — not even 99, not even absent — and
the mode makes no difference. Zero counts as missing: their side cannot tell "nought"
from "not sent".

## Will it actually run?

Eleven jobs, run to the end, same word and point throughout.

| mode | zoom | depth | ended as | businesses |
|---|---|---|---|---|
| fast | 1 | 10 | ok | 20 |
| fast | 12 | 10 | ok | 20 |
| fast | 21 | 10 | ok | 20 |
| fast | **absent** | 10 | **stuck at `working` for ever** | 0 |
| fast | **0** | 10 | **stuck** | 0 |
| fast | **22** | 10 | **stuck** | 0 |
| fast | **99** | 10 | **stuck** | 0 |
| normal | absent | 1 | ok | 20 |
| normal | 0 | 1 | ok | 20 |
| normal | 12 | 1 | ok | 20 |
| normal | 99 | 1 | ok | 20 |

Each stuck job logs `error scraping job <id>: invalid zoom level: N` and then never
finishes. **The scraper runs one job at a time, so a stuck one blocks everything
behind it** — this was found the hard way, when a later sweep sat idle for twenty
minutes behind a job that had already failed. `DELETE /api/v1/jobs/{id}` clears it.

## How much does depth buy?

Normal mode, `fodrászat Budapest`, from one point, `max_time` generous enough not to
be the limit.

| depth | businesses | took |
|---|---|---|
| 1 | 16 | 30 s |
| 5 | 40 | 71 s |
| 10 | 78 | 145 s |
| 12 | 88 | ~150 s |
| 15 | **108** | ~170 s |
| 18 | **0** | ~65 s |
| 20 | **0** | 65 s |

Roughly seven more businesses per step, up to Google's own ceiling of about 120 per
search. Then it falls off a cliff: 18 and 20 were each run **twice** and produced an
empty file every time. Depth 20 completed nought scrape jobs in sixty seconds — it is
not a slow run being cut short, it is a run that does nothing and reports success.

**16 and 17 were never tried.** The cap is 15 because that is the highest value
measured working.

## How much does anything buy in fast mode?

Nothing. Fast mode is a single request to Google's own endpoint that returns one page
of twenty.

| depth (zoom 12) | 1 | 5 | 10 | 20 |
|---|---|---|---|---|
| businesses | 20 | 20 | 20 | 20 |

| zoom (depth 10) | 1 | 10 | 12 | 14 | 16 | 21 |
|---|---|---|---|---|---|---|
| businesses | 20 | 20 | 20 | 20 | 20 | 20 |

All twenty within 3 km of the point, median 0.3 km, in every one of those runs — the
results are already local, whatever the zoom says.

**The way to get more out of fast mode is more words**, not a bigger number:
"fodrászat Óbuda" and "fodrászat Zugló" give forty between them; "fodrászat" twice
gives the same twenty twice.

### What fast mode does return

Contrary to what its name suggests, it does not strip the fields we need. One run,
twenty businesses:

| field | how many had it |
|---|---|
| phone | 19 of 20 |
| website | 17 of 20 |
| opening hours | 20 of 20 |

It looks wherever the words point rather than staying near your coordinates, so name
the place in the words.

---

## What this made us change

- `USUAL_DEPTH` = 15, and the page will not offer more (`crates/domain/src/options.rs`).
- `CITY_ZOOM` = 12, always sent, because absence is what hangs a fast job.
- Zoom is checked against 1–21 and depth against 1–15 before a search is allowed.
- Fast mode takes the zoom over, sets it to 12, and hides both switches — one of them
  does nothing and the other can hang the scraper.
- The area's radius fence is gone entirely: it also fed the scraper a filter of its
  own in fast mode, which returned empty files for days.
