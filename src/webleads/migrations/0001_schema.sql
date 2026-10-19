-- The whole database, in one go.
--
-- A city carries what is true of a place: its country, its clock, the language its
-- trades describe themselves in, and the point distances in it are measured from.
-- Areas are the places inside it you search; runs are the searches; a business is
-- remembered once however often it is found, and a lead is what one run decided
-- about it.

CREATE TABLE city (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL UNIQUE,
    -- Two lower-case letters, as the address lookup reports them: 'hu', 'ae'.
    country_code TEXT NOT NULL,
    -- An IANA name, e.g. 'Europe/Budapest'. Used for this city's opening hours and
    -- nothing else — a ring-back is in your own time, wherever the shop is.
    zone         TEXT NOT NULL,
    -- What its trades describe themselves in.
    lang         TEXT NOT NULL,
    -- Distances in this city are measured from here, and this is what it is called.
    centre_lat   REAL NOT NULL,
    centre_lon   REAL NOT NULL,
    added_on     TEXT NOT NULL,
    centre_label TEXT
);

CREATE TABLE area (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    -- What you call it, and what goes into the search words: "borbély Zugló".
    label    TEXT NOT NULL UNIQUE,
    -- The address it was looked up from, so a coordinate can always be traced back
    -- to the words that produced it.
    address  TEXT NOT NULL,
    lat      REAL NOT NULL,
    lon      REAL NOT NULL,
    -- NULL means no fence: keep everything the search returns.
    radius_m INTEGER,
    added_on TEXT NOT NULL,
    city_id  INTEGER REFERENCES city(id)
);

CREATE TABLE run (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    -- The file or search this came from.
    source        TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    finished_at   TEXT,
    -- waiting | working | held | done | stopped | failed
    state         TEXT NOT NULL DEFAULT 'done',
    -- import | scrape
    kind          TEXT NOT NULL DEFAULT 'import',
    note          TEXT,
    -- Where it searched, written down with it: an area can be forgotten later.
    area_label    TEXT,
    area_lat      REAL,
    area_lon      REAL,
    area_radius_m INTEGER,
    -- The settings it was asked to use, as the JSON the scraper is sent.
    options_json  TEXT,
    city_id       INTEGER REFERENCES city(id)
);

CREATE TABLE business (
    -- Google's id, or name + rounded position.
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    phone           TEXT,
    category        TEXT,
    address         TEXT,
    maps_url        TEXT,
    lat             REAL NOT NULL,
    lon             REAL NOT NULL,
    -- ISO dates: set once, and moved every time we see it again.
    first_seen      TEXT NOT NULL,
    last_seen       TEXT NOT NULL,
    -- One telephone number however Google wrote it, so duplicates can be found.
    phone_canonical TEXT,
    -- When the website was last judged.
    checked_on      TEXT,
    -- A duplicate points at the listing it was folded into.
    merged_into     TEXT REFERENCES business(id),
    trade           TEXT,
    found_by        TEXT,
    -- Google's opening hours as it wrote them, and the opening minute of each day.
    open_hours      TEXT,
    opens_mon       INTEGER,
    opens_tue       INTEGER,
    opens_wed       INTEGER,
    opens_thu       INTEGER,
    opens_fri       INTEGER,
    opens_sat       INTEGER,
    opens_sun       INTEGER,
    -- The search that found it first.
    first_run_id    INTEGER,
    city_id         INTEGER REFERENCES city(id)
);

CREATE TABLE lead (
    business_id TEXT NOT NULL REFERENCES business(id) ON DELETE CASCADE,
    run_id      INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
    -- no_site | social_only | dead_site | insecure_site | live_site
    verdict     TEXT NOT NULL,
    reason      TEXT NOT NULL,
    website     TEXT,
    social_url  TEXT,
    reviews     INTEGER NOT NULL DEFAULT 0,
    rating      REAL,
    distance_m  INTEGER NOT NULL DEFAULT 0,
    -- What we learned about the certificate on the way.
    tls         TEXT NOT NULL DEFAULT 'unknown',
    tls_detail  TEXT,
    -- The newest verdict for its business; the call list reads only these.
    is_latest   INTEGER NOT NULL DEFAULT 1,
    tls_rank    INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (business_id, run_id)
);

CREATE TABLE contact (
    business_id         TEXT PRIMARY KEY REFERENCES business(id) ON DELETE CASCADE,
    -- need_call | not_reached | follow_up | they_call | sold | not_interested | dead
    status              TEXT NOT NULL DEFAULT 'need_call',
    note                TEXT NOT NULL DEFAULT '',
    not_reached_counter INTEGER NOT NULL DEFAULT 0,
    -- When the next call is due, as `2026-09-21 14:30`.
    follow_up_tstamp    TEXT,
    updated_at          TEXT NOT NULL
);

CREATE TABLE call (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    business_id TEXT NOT NULL REFERENCES business(id) ON DELETE CASCADE,
    called_on   TEXT NOT NULL
);

CREATE TABLE run_word (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id      INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
    -- The order they were queued in, so picking a run back up is deterministic.
    position    INTEGER NOT NULL,
    -- What we actually search Google for: "borbély Budapest".
    phrase      TEXT NOT NULL,
    trade       TEXT,
    -- waiting | searching | done | failed
    state       TEXT NOT NULL DEFAULT 'waiting',
    -- The scraper's own id, so a restart can pick a job back up rather than
    -- searching the same word twice.
    remote_id   TEXT,
    -- How many businesses this word returned. Zero is the interesting number.
    found       INTEGER,
    note        TEXT,
    started_at  TEXT,
    finished_at TEXT
);

CREATE TABLE hidden (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- One of these two is set and the other is NULL: one shop, or a whole chain.
    business_id TEXT,
    name_like   TEXT,
    -- What to show you in the panel: the shop's name, or the chain's words.
    label       TEXT NOT NULL,
    added_on    TEXT NOT NULL,
    CHECK ((business_id IS NULL) <> (name_like IS NULL))
);

-- The few things that must outlive a restart: which area is chosen, whether the
-- queue is holding, the search language.
CREATE TABLE setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX business_by_canonical ON business (phone_canonical);
CREATE INDEX business_by_checked   ON business (checked_on);
CREATE INDEX business_by_merge     ON business (merged_into);
CREATE INDEX business_by_first_run ON business (first_run_id);
CREATE INDEX lead_by_run           ON lead (run_id, verdict);
CREATE INDEX lead_ranking          ON lead (reviews, distance_m);
CREATE INDEX lead_call_list        ON lead (is_latest, verdict, reviews, distance_m);
CREATE INDEX contact_by_status     ON contact (status);
CREATE INDEX contact_by_follow_up  ON contact (follow_up_tstamp)
    WHERE follow_up_tstamp IS NOT NULL;
CREATE INDEX call_by_business      ON call (business_id, called_on);
CREATE INDEX run_word_next         ON run_word (run_id, state, position);
CREATE INDEX run_queue             ON run (state, id);
CREATE INDEX hidden_by_business    ON hidden (business_id);

-- Somewhere to start: Budapest, measured from Hősök tere until you set your own spot
-- on the page.
INSERT INTO city (name, country_code, zone, lang, centre_lat, centre_lon, added_on, centre_label)
VALUES ('Budapest', 'hu', 'Europe/Budapest', 'hu', 47.5149, 19.0778, DATE('now'), 'Hősök tere, Budapest');

INSERT INTO area (label, address, lat, lon, radius_m, added_on, city_id)
VALUES ('Budapest', 'Hősök tere, Budapest, Hungary', 47.5149, 19.0778, NULL, DATE('now'),
        (SELECT id FROM city WHERE name = 'Budapest'));
