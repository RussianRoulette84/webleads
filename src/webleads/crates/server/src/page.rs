//! The call list as one file you can open with our server switched off.
//!
//! # What this file is here to teach
//!
//! Building a whole page as a value. Everything that varies is turned into JSON once
//! and handed to a script that is otherwise fixed — no templating language, no
//! string-stitched HTML around somebody's business name, and therefore no way for a
//! name holding a quotation mark to break the page.
//!
//! The style sheet, the map drawing and the page's own script are baked into the
//! binary at compile time, so the file that comes out needs nothing of ours at all.

use serde::Serialize;
use webleads_store::StoredLead;

/// Our own look, compiled in. The same files the live page uses.
const TOKENS: &str = include_str!("../../../../web/css/tokens.css");
const BASE: &str = include_str!("../../../../web/css/base.css");
const COMPONENTS: &str = include_str!("../../../../web/css/components.css");

/// The map, drawn by exactly the same code as the live page — so the pins, the
/// grouping and the bubbles are the same thing rather than a lookalike.
const MAP: &str = include_str!("../../ui/map.js");

/// This page's own behaviour: search, chips, sorting.
const SCRIPT: &str = include_str!("page.js");

/// Leaflet, from its own address. The only thing here that is not ours.
///
/// Loaded with `defer` on purpose: a plain script tag blocks the page until it
/// arrives, so a slow or unreachable address meant a white screen rather than a
/// table. Deferred, the list is there at once and the map appears when it can.
const LEAFLET_CSS: &str = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.css";
const LEAFLET_JS: &str = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.js";

/// One business, flattened into what the page actually shows.
///
/// A shape of its own rather than the stored lead: this is read by a script, and a
/// field it does not use is a field somebody has to wonder about later.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub trade: Option<String>,
    pub address: Option<String>,
    pub maps_url: Option<String>,
    /// The reason, as we store it — what the chips match on.
    pub why: String,
    /// The reason in words, for the tag.
    pub why_said: String,
    /// Which colour that reason wears.
    pub tone: String,
    pub reason: String,
    /// What to say on the telephone.
    pub pitch: String,
    pub reviews: u32,
    pub rating: Option<f32>,
    pub distance_m: u32,
    pub hours_today: Option<String>,
    /// The whole week on one line, so the page you keep says when to ring.
    pub hours_week: Option<String>,
    pub status: String,
    /// How many times nobody answered, so the kept page says why it is dead.
    pub no_answers: i64,
    /// When you said you would ring back, if you did.
    pub ring_back: Option<String>,
    pub note: String,
    /// The map reads these two directly, so they carry the map's own names.
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

/// What the top of the page says.
#[derive(Debug, Clone)]
pub struct Heading {
    /// The area this list came from.
    pub area: String,
    /// The middle of it, for the map's first view.
    pub centre: (f64, f64),
    /// The day it was made.
    pub made_on: String,
}

/// Turn one stored lead into what the page shows.
pub fn row_for(lead: &StoredLead) -> Row {
    let inner = &lead.lead;
    Row {
        id: inner.id.clone(),
        name: inner.name.clone(),
        phone: inner.phone.clone(),
        trade: inner.shown_trade().map(str::to_owned),
        address: inner.address.clone(),
        maps_url: inner.maps_url.clone(),
        why: inner.verdict.as_str().to_owned(),
        why_said: inner.verdict.describe().to_owned(),
        tone: inner.verdict.tone().to_owned(),
        reason: inner.reason.clone(),
        pitch: inner.pitch(),
        reviews: inner.reviews,
        rating: inner.rating,
        distance_m: inner.distance_m,
        hours_today: lead.hours_today.clone(),
        hours_week: inner.hours.week_line(),
        status: lead.contact.status.describe().to_owned(),
        no_answers: lead.contact.not_reached_counter,
        ring_back: lead.contact.follow_up_tstamp.clone(),
        note: lead.contact.note.clone(),
        lat: inner.position.map(|(lat, _)| lat),
        lon: inner.position.map(|(_, lon)| lon),
    }
}

/// The whole page, as one string.
///
/// Pure, so what comes out can be checked without a browser or a server.
pub fn build(leads: &[StoredLead], heading: &Heading) -> String {
    let rows: Vec<Row> = leads.iter().map(row_for).collect();
    let total = rows.len();
    let never_reviewed = rows.iter().filter(|r| r.reviews == 0).count();
    let json = safe_json(&rows);

    let area = escape(&heading.area);
    let made_on = escape(&heading.made_on);
    let (lat, lon) = heading.centre;

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Webleads — {area}</title>
<link rel="stylesheet" href="{LEAFLET_CSS}"/>
<script src="{LEAFLET_JS}" defer></script>
<style>
{TOKENS}
{BASE}
{COMPONENTS}
body{{background:var(--wl-ink);color:var(--wl-bone);font-family:var(--wl-font-body);
     margin:0;padding:24px 20px}}
table.table th, table.table td{{padding:10px 12px;vertical-align:top}}
th[data-sort]{{cursor:pointer;user-select:none}}
.leaflet-container{{background:var(--wl-ink);font-family:var(--wl-font-body)}}
.wl-tiles{{filter:grayscale(1) sepia(.8) hue-rotate(350deg) saturate(2.6) brightness(.85);z-index:2}}
.wl-water{{filter:grayscale(1) brightness(.28) contrast(1.9)}}
.wl-pin{{background:none;border:0}}
.wl-pin svg{{display:block;filter:drop-shadow(0 2px 3px rgba(0,0,0,.7))}}
.leaflet-popup-content-wrapper{{background:var(--wl-ink);color:var(--wl-bone);
     border:1px solid var(--wl-brass);border-radius:0;box-shadow:none}}
.leaflet-popup-tip{{background:var(--wl-ink)}}
.leaflet-container a{{color:var(--wl-brass-lit)}}
@keyframes wl-found{{
  0%{{background:rgba(201,150,47,.26)}}
  70%{{background:rgba(201,150,47,.12)}}
  100%{{background:transparent}}
}}
tr.wl-found td{{animation:wl-found 2s ease-out both}}
.leaflet-popup-content .wl-jump{{display:inline-block;margin-top:6px;color:var(--wl-brass-lit);
  border-bottom:1px solid var(--wl-brass);text-decoration:none}}
</style>
</head>
<body>
<div class="panel">
  <h4 class="heading">The call list — {area}</h4>
  <p class="is-dim" style="font-size:15px;margin:0 0 16px">
    <span id="shown">{total}</span> of {total} shown · {never_reviewed} never reviewed ·
    made {made_on}
  </p>

  <div style="display:flex;gap:10px;flex-wrap:wrap;margin-bottom:12px">
    <input class="field field--search" id="search" type="search"
           style="flex:1;min-width:220px"
           placeholder="search name, address, trade, number or notes…"/>
  </div>

  <div style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:14px">
    <button class="btn" data-why="">any reason</button>
    <button class="btn btn--quiet" data-why="no_site">no website</button>
    <button class="btn btn--quiet" data-why="social_only">social only</button>
    <button class="btn btn--quiet" data-why="dead_site">broken site</button>
    <button class="btn btn--quiet" data-why="insecure_site">certificate broken</button>
  </div>

  <div class="panel" style="padding:0;margin-bottom:18px;overflow:hidden">
    <div id="webleads-map" style="height:360px;width:100%"></div>
  </div>

  <div style="overflow-x:auto">
  <table class="table u-wide">
    <thead><tr>
      <th data-sort="name">Business<span class="arrow"></span></th>
      <th data-sort="trade">Trade<span class="arrow"></span></th>
      <th data-sort="phone">Telephone<span class="arrow"></span></th>
      <th data-sort="why_said">Why<span class="arrow"></span></th>
      <th data-sort="hours_today">Open today<span class="arrow"></span></th>
      <th data-sort="reviews" style="text-align:right">Reviews<span class="arrow"></span></th>
      <th data-sort="distance_m" style="text-align:right">Away<span class="arrow"></span></th>
      <th data-sort="status">State<span class="arrow"></span></th>
      <th>Notes</th>
      <th></th>
    </tr></thead>
    <tbody id="rows"></tbody>
  </table>
  </div>
</div>

<script type="application/json" id="leads">{json}</script>
<script>const CENTRE = [{lat}, {lon}];</script>
<script>{MAP}</script>
<script>{SCRIPT}</script>
</body>
</html>
"#
    )
}

/// The rows as JSON that is safe to sit inside a `<script>` tag.
///
/// A business named `</script>` would otherwise end the tag early and the rest of the
/// page would be shown as text. Escaping the slash is the whole fix, and it is still
/// valid JSON.
pub fn safe_json(rows: &[Row]) -> String {
    serde_json::to_string(rows)
        .unwrap_or_else(|_| "[]".to_owned())
        .replace("</", "<\\/")
}

/// Text that is safe to put in the page as text.
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
#[path = "page_tests.rs"]
mod tests;
