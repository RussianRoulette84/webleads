// The standalone page's own behaviour: search, chips, sorting, and the map.
//
// It talks to nothing. Every lead is already in the file, in a script tag, which is
// what lets this work with our server switched off.

const LEADS = JSON.parse(document.getElementById('leads').textContent);

let showing = LEADS.slice();
let why = null;
let sortBy = null;
let biggestFirst = false;

function matches(lead) {
  if (why && lead.why !== why) return false;
  const text = document.getElementById('search').value.trim().toLowerCase();
  if (!text) return true;
  return [lead.name, lead.address, lead.trade, lead.phone, lead.note]
    .filter(Boolean)
    .some((field) => field.toLowerCase().includes(text));
}

function sorted(rows) {
  if (!sortBy) return rows;
  const way = biggestFirst ? -1 : 1;
  return rows.slice().sort((a, b) => {
    const left = a[sortBy] ?? '';
    const right = b[sortBy] ?? '';
    if (typeof left === 'number' && typeof right === 'number') return (left - right) * way;
    return String(left).localeCompare(String(right), 'hu') * way;
  });
}

function draw() {
  showing = sorted(LEADS.filter(matches));
  document.getElementById('shown').textContent = showing.length;

  const body = document.getElementById('rows');
  body.innerHTML = '';
  for (const lead of showing) {
    body.appendChild(rowFor(lead));
  }

  if (window.webleadsMapPins) {
    webleadsMapPins(JSON.stringify(showing.filter((l) => l.lat !== null)));
  }
}

function rowFor(lead) {
  const row = document.createElement('tr');
  row.dataset.lead = lead.id;

  const dial = (lead.phone || '').replace(/[^\d+]/g, '');
  const cells = [
    `<strong>${esc(lead.name)}</strong>
     <div class="is-accent" style="font-size:11px">${esc(lead.pitch)}</div>
     <div class="is-dim" style="font-size:12px">${esc(lead.address || '')}</div>`,
    lead.trade ? esc(lead.trade) : '',
    dial ? `<a class="link" href="tel:${esc(dial)}">${esc(lead.phone)}</a>` : '',
    `<span class="tag is-${lead.tone}">${esc(lead.why_said)}</span>
     <div class="is-dim" style="font-size:12px">${esc(lead.reason)}</div>`,
    `${esc(lead.hours_today || '')}
     <div class="is-dim" style="font-size:12px">${esc(lead.hours_week || '')}</div>`,
    lead.reviews === 0 ? '<span class="tag is-accent">none</span>' : String(lead.reviews),
    lead.distance_m < 1000 ? `${lead.distance_m} m` : `${(lead.distance_m / 1000).toFixed(1)} km`,
    `${esc(lead.status)}
     ${lead.no_answers ? `<div class="is-dim" style="font-size:12px">rung ${lead.no_answers}×, no answer</div>` : ''}
     ${lead.ring_back ? `<div class="is-accent" style="font-size:12px">${esc(lead.ring_back)}</div>` : ''}`,
    esc(lead.note || ''),
    lead.maps_url ? `<a class="link" href="${esc(lead.maps_url)}" target="_blank" rel="noreferrer">Map</a>` : '',
  ];

  for (const [index, html] of cells.entries()) {
    const cell = document.createElement('td');
    // Numbers read better against the column they line up with.
    if (index === 5 || index === 6) cell.style.textAlign = 'right';
    cell.innerHTML = html;
    row.appendChild(cell);
  }
  return row;
}

function esc(text) {
  return String(text ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[c]);
}

// ---- the controls ----

document.getElementById('search').addEventListener('input', draw);
document.getElementById('search').addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.target.value = '';
    draw();
  }
});

for (const chip of document.querySelectorAll('[data-why]')) {
  chip.addEventListener('click', () => {
    why = chip.dataset.why || null;
    for (const other of document.querySelectorAll('[data-why]')) {
      other.className = other === chip ? 'btn' : 'btn btn--quiet';
    }
    draw();
  });
}

for (const heading of document.querySelectorAll('[data-sort]')) {
  heading.addEventListener('click', () => {
    const column = heading.dataset.sort;
    // Same column again turns it round; a new one opens the way it reads best.
    biggestFirst = sortBy === column ? !biggestFirst : column === 'reviews';
    sortBy = column;
    for (const other of document.querySelectorAll('[data-sort]')) {
      other.querySelector('.arrow').textContent =
        other === heading ? (biggestFirst ? ' ↓' : ' ↑') : '';
    }
    draw();
  });
}

draw();

// The map comes last and is optional. Leaflet is deferred, so it is not here yet
// when this file runs — waiting for `load` is what lets the table appear at once,
// and what keeps the page working when Leaflet cannot be reached at all.
window.addEventListener('load', () => {
  if (!window.L || !window.webleadsMapInit) {
    document.getElementById('webleads-map').remove();
    return;
  }
  webleadsMapInit('webleads-map', CENTRE[0], CENTRE[1]);
  draw();
});
