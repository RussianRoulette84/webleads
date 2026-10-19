// The small amount of JavaScript the map needs.
//
// Leaflet is a JavaScript library and a full typed Rust binding for it would be far
// more code than the map itself. These two functions are the whole seam: Rust says
// "make a map" and "here are the pins", and nothing else crosses over.
//
// The map is a VIEW of the filter. It never decides what is shown — it is told.

let theMap = null;
let pinLayer = null;
// Where the list is measured from, marked with our own icon, and what it is called.
let homeMarker = null;
let homeWanted = null;
let everyPin = [];
// Fitted once at the start and after a search. Never again: panning is the user's
// to undo, and a map that jumps back every time a slider moves loses your place.
let hasBeenFitted = false;

// Group pins only when the view is crowded. Below this, every pin shows on its own,
// so zooming in always ends at real pins rather than at another number.
const CROWDED = 100;

// Our own colours, from the stylesheet rather than invented here.
const INK = '#0b0a08';
const BONE = '#e8e2d4';
const ASH = '#6b6459';
const BRASS = '#8a6820';
const BRASS_LIT = '#c9962f';
const RUST = '#a8453a';

// The colour names Rust sends, turned into our own colours. Which reason gets which
// name is decided in Rust, where it is tested — see `tone_for`.
const TONE = {
  accent: BRASS_LIT,
  warn: BRASS,
  bad: RUST,
  dim: ASH,
};

window.webleadsMapInit = function (elementId, lat, lon) {
  const element = document.getElementById(elementId);
  if (!element || theMap) return;

  theMap = L.map(element, { scrollWheelZoom: false, zoomControl: true })
    .setView([lat, lon], 12);
  // The map data's credit stays — its licences require it — but not Leaflet's own
  // name and flag in front of it.
  theMap.attributionControl.setPrefix(false);

  // No landmass at all. This layer draws streets, railways and their names on a
  // transparent background, so what shows behind them is the page's own ink — which
  // is why the map is dark rather than grey or brown.
  //
  // `keepBuffer` holds on to tiles just outside the view, so panning back does not
  // fetch what we already had.
  //
  // `maxNativeZoom` is the last zoom the service actually draws; past it Leaflet
  // stretches the tile it has. This layer draws to 19, and pinning it to 16 was the
  // bug behind two complaints at once: every street was stretched double, quadruple
  // and then eight times over, so the lines went fat and soft and at the closest
  // zooms there was nothing recognisable left to see.
  L.tileLayer(
    'https://server.arcgisonline.com/ArcGIS/rest/services/Reference/World_Transportation/MapServer/tile/{z}/{y}/{x}',
    {
      maxZoom: 19,
      maxNativeZoom: 19,
      attribution: 'Esri · © OpenStreetMap',
      className: 'wl-tiles',
      keepBuffer: 4,
      // Above the water. Leaflet's own ordering, not a `z-index` in the stylesheet:
      // the stylesheet rule was lost in a duplicated block and the map went black,
      // because the crushed water layer was painting over the streets.
      zIndex: 5,
    },
  ).addTo(theMap);

  // The water, underneath, crushed until only the rivers and the lake read as shapes.
  // Without it the Danube is missing, and half of Budapest stops making sense.
  L.tileLayer(
    'https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}',
    // Sixteen is this one's real limit: past it the service answers with a grey
    // "map data not yet available" tile, so it stretches its last good one instead.
    // The river is a broad shape; stretched, it still reads as the river.
    { maxZoom: 19, maxNativeZoom: 16, className: 'wl-water', keepBuffer: 4, zIndex: 1 },
  ).addTo(theMap);

  pinLayer = L.layerGroup().addTo(theMap);

  // The bubble is rebuilt each time it opens, so the link is wired when it appears
  // rather than once at the start.
  theMap.on('popupopen', (event) => {
    const link = event.popup.getElement()?.querySelector('.wl-jump');
    if (!link) return;
    link.addEventListener('click', (click) => {
      click.preventDefault();
      theMap.closePopup();
      highlightRow(link.dataset.lead);
    });
  });
  // Regroup as the view changes — the count that matters is what is on screen.
  theMap.on('zoomend moveend', draw);

  // Hold ⌘ (or Ctrl) and scroll to zoom, centred on the pointer rather than on the
  // middle of the map. Plain scrolling keeps moving the page, which is what you want
  // when the map is halfway down a long list.
  element.addEventListener(
    'wheel',
    (event) => {
      if (!event.metaKey && !event.ctrlKey) return;
      event.preventDefault();
      const where = theMap.mouseEventToLatLng(event);
      const step = event.deltaY < 0 ? 1 : -1;
      theMap.setZoomAround(where, theMap.getZoom() + step);
    },
    { passive: false },
  );

  placeHome();
};

// The spot every distance on the list is measured from. Asked for before the map may
// exist, so it is remembered and placed as soon as there is a map to place it on.
window.webleadsMapHome = function (lat, lon, label) {
  homeWanted = { lat, lon, label };
  placeHome();
};

function placeHome() {
  if (!theMap || !homeWanted) return;
  const { lat, lon, label } = homeWanted;
  if (!homeMarker) {
    homeMarker = L.marker([lat, lon], {
      icon: L.icon({
        iconUrl: '/img/favicon.svg',
        iconSize: [34, 34],
        iconAnchor: [17, 17],
        className: 'wl-home-pin',
      }),
      zIndexOffset: 1000,
      keyboard: false,
    }).addTo(theMap);
  } else {
    homeMarker.setLatLng([lat, lon]);
  }
  homeMarker.unbindTooltip().bindTooltip('Measured from ' + label, { direction: 'top' });
}

window.webleadsMapPins = function (json, startAgain) {
  if (!theMap) return;
  try {
    everyPin = JSON.parse(json) || [];
  } catch (err) {
    console.error('map: could not read the pins', err);
    return;
  }

  // Fitted once, and again when the page says you are looking at another city —
  // keeping the old view across a move of a thousand miles shows you an empty sea.
  // Otherwise the view is yours: panning and zooming are never undone.
  if (startAgain) hasBeenFitted = false;
  if (!hasBeenFitted && everyPin.length > 0) {
    const points = everyPin.map((p) => [p.lat, p.lon]);
    theMap.fitBounds(L.latLngBounds(points), { padding: [30, 30], maxZoom: 15 });
    hasBeenFitted = true;
  }
  draw();
};

// Draw whatever belongs in the current view.
function draw() {
  if (!theMap || !pinLayer) return;
  pinLayer.clearLayers();

  const view = theMap.getBounds();
  const inView = everyPin.filter((p) => view.contains([p.lat, p.lon]));

  if (inView.length > CROWDED) {
    for (const group of intoGroups(inView)) {
      // A group of one is just a pin: a "1" in a box hides the very thing it counts.
      if (group.pins.length === 1) {
        drawPin(group.pins[0]);
        continue;
      }
      L.marker([group.lat, group.lon], { icon: groupIcon(group.pins.length) })
        .on('click', () => theMap.setView([group.lat, group.lon], theMap.getZoom() + 2))
        .addTo(pinLayer);
    }
    return;
  }

  for (const pin of inView) drawPin(pin);
}

// One business on the map. Clicking it opens its bubble and nothing else. Being
// thrown down the page the instant you touch a pin is how you lose the map.
function drawPin(pin) {
  L.marker([pin.lat, pin.lon], { icon: pinIcon(pin) })
    .bindPopup(bubbleFor(pin))
    .addTo(pinLayer);
}

// Gather pins that land on the same patch of screen.
//
// Done in screen space rather than by distance, so a group is always a group at the
// zoom you are actually looking at.
function intoGroups(pins) {
  const CELL = 60; // pixels
  const cells = new Map();

  for (const pin of pins) {
    const at = theMap.latLngToContainerPoint([pin.lat, pin.lon]);
    const key = `${Math.floor(at.x / CELL)},${Math.floor(at.y / CELL)}`;
    if (!cells.has(key)) cells.set(key, []);
    cells.get(key).push(pin);
  }

  return [...cells.values()].map((group) => ({
    pins: group,
    lat: group.reduce((sum, p) => sum + p.lat, 0) / group.length,
    lon: group.reduce((sum, p) => sum + p.lon, 0) / group.length,
  }));
}

// One pin: a stepped art-deco frame with the page's own fleuron inside it.
//
// The three states differ in fill and weight rather than in colour, so the reason
// colours stay readable: untouched is filled, rung is hollow, sold is faded.
function pinIcon(pin) {
  const colour = TONE[pin.tone] || ASH;
  const rung = pin.status === 'not_reached' || pin.status === 'follow_up';
  const sold = pin.status === 'sold';

  const fill = rung ? INK : colour;
  const mark = rung ? colour : INK;
  const fade = sold ? 0.45 : 1;

  const svg = `
    <svg width="30" height="38" viewBox="0 0 30 38" opacity="${fade}">
      <!-- The stepped frame, the same cut corners as the panels on the page. -->
      <path d="M15 37 L3 20 L3 8 L8 3 L22 3 L27 8 L27 20 Z"
            fill="${fill}" stroke="${colour}" stroke-width="2"
            stroke-linejoin="miter"/>
      <path d="M8 6 L22 6" stroke="${mark}" stroke-width="1" opacity=".5"/>
      <text x="15" y="19" text-anchor="middle" font-size="13" fill="${mark}"
            font-family="Georgia, serif">&#10086;</text>
    </svg>`;

  return L.divIcon({
    html: svg,
    className: 'wl-pin',
    iconSize: [30, 38],
    iconAnchor: [15, 37],
    popupAnchor: [0, -34],
  });
}

// A group of pins: the same frame, with how many are inside it.
function groupIcon(count) {
  const svg = `
    <svg width="40" height="40" viewBox="0 0 40 40">
      <path d="M20 2 L34 8 L38 20 L34 32 L20 38 L6 32 L2 20 L6 8 Z"
            fill="${INK}" stroke="${BRASS_LIT}" stroke-width="2"
            stroke-linejoin="miter"/>
      <text x="20" y="25" text-anchor="middle" font-size="13" fill="${BONE}"
            font-family="Georgia, serif">${count}</text>
    </svg>`;

  return L.divIcon({
    html: svg,
    className: 'wl-pin wl-pin--group',
    iconSize: [40, 40],
    iconAnchor: [20, 20],
  });
}

// What the bubble says. The telephone number first — it is the one thing you want
// from a map and it was the one thing missing.
function bubbleFor(pin) {
  const dial = (pin.phone || '').replace(/[^\d+]/g, '');
  const lines = [`<strong>${escapeHtml(pin.name)}</strong>`];

  if (dial) {
    lines.push(`<a href="tel:${escapeHtml(dial)}">${escapeHtml(pin.phone)}</a>`);
  }
  if (pin.trade) lines.push(escapeHtml(pin.trade));
  lines.push(escapeHtml(sayWhy(pin.why)));
  if (pin.status !== 'need_call') lines.push(`<em>${escapeHtml(sayState(pin.status))}</em>`);
  // The way down to the row, taken only when you ask for it.
  lines.push(`<a href="#" class="wl-jump" data-lead="${escapeHtml(pin.id)}">Jump to the row ↓</a>`);

  return lines.join('<br>');
}

// The state in the same words the table uses. The stored word is `not_reached`; what
// a person reads is "no answer".
function sayState(status) {
  return {
    need_call: 'to call',
    not_reached: 'no answer',
    follow_up: 'ring back',
    sold: 'sold',
    not_interested: 'said no',
    dead: 'line dead',
  }[status] || status;
}

function sayWhy(why) {
  return {
    no_site: 'no website at all',
    social_only: 'only a social page',
    dead_site: 'website is down',
    insecure_site: 'certificate is broken',
  }[why] || why;
}

// Scroll to a pin's row and light it up. Published on `window` as well, because the
// coming-up panel points at a row the same way a pin does — and two copies of the
// same animation would drift apart.
window.webleadsShowRow = function (id) {
  highlightRow(id);
};

function highlightRow(id) {
  const row = document.querySelector(`[data-lead="${cssEscape(id)}"]`);
  if (!row) return;
  row.scrollIntoView({ behavior: 'smooth', block: 'center' });
  // Restarting the animation needs the class off for a frame; without that, asking
  // for the same row twice does nothing at all the second time.
  row.classList.remove('wl-found');
  void row.offsetWidth;
  row.classList.add('wl-found');
  setTimeout(() => row.classList.remove('wl-found'), 2000);
}

// A business name is somebody else's text. It goes into the page as text, never as
// markup.
function escapeHtml(text) {
  const box = document.createElement('div');
  box.textContent = text ?? '';
  return box.innerHTML;
}

// Ids are built from names and coordinates, so they hold characters a selector would
// otherwise read as syntax.
function cssEscape(value) {
  return window.CSS && CSS.escape
    ? CSS.escape(value)
    : String(value).replace(/["\\]/g, '\\$&');
}
