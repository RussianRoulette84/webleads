//! What counts as being on the call list.
//!
//! One definition, used by everything that reads or counts it. Two pieces of SQL for
//! the list and its counts would drift apart until the numbers on screen contradicted
//! each other; one rule in one place cannot.

use webleads_domain::{today_here, Sort, Status, Verdict};

/// What a business with no contact row of its own counts as, in SQL.
///
/// **The word comes from the domain, never typed here.** A business we have merely
/// heard of has no contact row at all, so four separate queries had `'new'` written
/// into them as "what that means" — and renaming the first state would have left the
/// list and the counts quietly disagreeing. There is one copy of it now.
pub(crate) fn stored_status() -> String {
    format!("COALESCE(c.status, '{}')", Status::default().as_str())
}

/// The states that keep a business on the call list, as a SQL list.
///
/// Also generated rather than written out: with six states there are three words to
/// keep in step, and this file's own header explains what happens when the same rule
/// exists in two places.
fn words_on_the_list() -> String {
    Status::words_on_the_list()
        .iter()
        .map(|word| format!("'{word}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Is this a moment we wrote, and nothing else?
///
/// A belt-and-braces check before a timestamp is pasted into a query. The value comes
/// from our own clock through [`webleads_domain::Due`], so this can only ever fail if
/// somebody later hands it something else — and then it narrows to nothing rather
/// than passing anything through.
fn is_a_moment(text: &str) -> bool {
    text.len() == 16
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == ':' || c == ' ')
}

/// Sorting by state, as how much work each one still represents.
///
/// The order is [`Status::work_left`], turned into SQL. Ordering by the stored word
/// would sort them alphabetically — dead, ring back, to call, said no, no answer,
/// sold — which is an order with no opinion in it at all.
fn status_order() -> String {
    let arms: String = Status::ALL
        .iter()
        .map(|status| format!(" WHEN '{}' THEN {}", status.as_str(), status.work_left()))
        .collect();
    format!("(CASE {}{arms} ELSE 99 END)", stored_status())
}

/// Does this business match something you have hidden?
///
/// One business by its id, or a whole chain by a pattern over its name. Written once
/// here because the rows, the counts, the map and the exports all have to agree about
/// it — a business hidden in one view and visible in another is worse than not hiding
/// at all.
const HIDDEN_MATCH: &str = "
        SELECT 1 FROM hidden h
        WHERE (h.business_id IS NOT NULL AND h.business_id = b.id)
           OR (h.name_like IS NOT NULL AND LOWER(b.name) LIKE h.name_like ESCAPE '\\')";

/// The opening-time column for each day, Monday first.
///
/// A list we wrote, indexed by a number — never a name off the request. Sorting by
/// "open today" has to reach a different column on a Sunday than on a Monday, and
/// this is how that happens without any text from outside getting near the SQL.
const OPENS_BY_DAY: [&str; 7] = [
    "b.opens_mon",
    "b.opens_tue",
    "b.opens_wed",
    "b.opens_thu",
    "b.opens_fri",
    "b.opens_sat",
    "b.opens_sun",
];

/// One day's opening minute, moved onto your own clock.
///
/// SQLite has no timezone arithmetic, and it does not need any: the difference
/// between two zones is a number of minutes, and the city row carries the zone name
/// the number comes from. It is worked out per query rather than stored, because
/// Hungary keeps summer time and the Gulf does not — a number written down in
/// September is an hour wrong by November.
///
/// A business with no city, or a city whose clock we cannot read, is not moved. Its
/// own minute is the best we have and pretending otherwise would sort it wrongly with
/// more confidence.
///
/// Pure, and both inputs are ours: the day is an index into a list we wrote, and the
/// offsets are numbers we worked out from our own city table. Nothing off a request
/// gets near the SQL.
fn opens_here(today: u8, ahead: &[(i64, i32)]) -> String {
    let column = OPENS_BY_DAY[usize::from(today.min(6))];
    if ahead.is_empty() {
        return column.to_owned();
    }
    // One arm per city we keep. There are two of them; a hundred would still be a
    // shorter expression than a join, and every value in it is an integer of ours.
    let arms: String = ahead
        .iter()
        .map(|(city, minutes)| format!(" WHEN {city} THEN {minutes}"))
        .collect();
    format!("({column} - CASE b.city_id{arms} ELSE 0 END)")
}

/// What makes a business appear on the call list, written once.
///
/// Anything that counts or reads the list goes through this same text, so the counts
/// and the total can never disagree.
///
/// The parameters, in order:
/// `?1` a status to narrow to (NULL for all), `?2` a search pattern (NULL for none),
/// `?3` include the refused, `?4` least reviews, `?5` least rating, `?6` furthest
/// away in metres, `?7` a trade, `?8` a verdict. Readers add `?9`/`?10` for the page.
/// The call-list rule, ready to be pasted into a query.
///
/// A function rather than a constant because two pieces of it are generated from the
/// domain — see [`stored_status`] and [`words_on_the_list`].
pub(crate) fn on_the_list() -> String {
    let status = stored_status();
    let on_the_list = words_on_the_list();
    format!(
        "
    FROM lead l
    JOIN business b ON b.id = l.business_id
    LEFT JOIN contact c ON c.business_id = b.id
    -- LEFT, so a business from before cities existed is still on the list. Here in
    -- the shared rule rather than in the one query that groups by it, because the
    -- city carries the clock a row's opening hours are read in.
    LEFT JOIN city ON city.id = b.city_id
    WHERE l.is_latest = 1
      AND l.verdict <> 'live_site'
      -- A business merged into another no longer exists on its own.
      AND b.merged_into IS NULL
      -- No telephone number, no lead. The whole product is businesses you can ring
      -- that have no website; one you cannot ring is not half a lead, it is none.
      AND b.phone IS NOT NULL
      AND TRIM(b.phone) <> ''
      -- Busy businesses have money. NULL means the slider is at its minimum.
      AND (?4 IS NULL OR l.reviews >= ?4)
      -- A business nobody has rated is kept while the slider is down, and dropped
      -- once you raise it. Dropping the unrated the moment you touch it would hide
      -- exactly the businesses most worth ringing.
      AND (?5 IS NULL OR (l.rating IS NOT NULL AND l.rating >= ?5))
      AND (?6 IS NULL OR l.distance_m <= ?6)
      -- The trade we searched for, falling back to Google's category — the same
      -- order `Lead::shown_trade` uses, so the dropdown and the filter agree.
      AND (?7 IS NULL OR COALESCE(NULLIF(TRIM(b.trade), ''), b.category) = ?7)
      AND (?8 IS NULL OR l.verdict = ?8)
      -- Asking for everybody means everybody still worth ringing, so a business
      -- that has said no is left out. Asking for that state by name is how you look
      -- at them again — which is the only way back from a misclick.
      --
      -- `?3` is the exception: counting every state has to see the refused ones too,
      -- or the button offering them would for ever say nought.
      AND (CASE WHEN ?1 IS NOT NULL THEN {status} = ?1
                WHEN ?3 THEN 1
                ELSE {status} IN ({on_the_list}) END)
      AND (?2 IS NULL OR
           b.name      LIKE ?2 OR
           b.address   LIKE ?2 OR
           b.category  LIKE ?2 OR
           b.phone     LIKE ?2 OR
           COALESCE(c.note, '') LIKE ?2)
"
    )
}

/// Which part of the call list somebody is asking for.
///
/// One struct rather than four loose arguments: the count and the page must be asked
/// the same question, and passing the same value to both is how that is guaranteed.
#[derive(Debug, Clone)]
pub struct Wanted {
    /// Narrow to one state, or `None` for everybody on the list.
    pub status: Option<Status>,
    /// Free text over the name, address, trade, telephone and your own notes.
    pub search: Option<String>,
    /// Fewest reviews to show. `None` while the slider is at its minimum.
    pub least_reviews: Option<i64>,
    /// Lowest rating to show. Raising this also drops the unrated.
    pub least_rating: Option<f64>,
    /// Furthest away to show, in metres.
    pub within_m: Option<i64>,
    /// One trade only, as the dropdown offers it.
    pub trade: Option<String>,
    /// One reason only — no website, social only, broken, insecure.
    pub why: Option<Verdict>,
    /// Which column to order by, and which way.
    pub sort: Sort,
    pub descending: bool,
    pub limit: i64,
    pub offset: i64,
    /// Only what one search was the first to find.
    ///
    /// **The search that first found it, not the search whose row is newest.** The
    /// obvious version reads `lead.run_id`, and a search whose businesses a later one
    /// re-saw would show nothing at all — which is exactly what happened when it was
    /// measured against the real database.
    pub from_run: Option<i64>,
    /// Only what the searches of one area first found.
    pub from_area: Option<String>,
    /// Only the businesses of one city.
    ///
    /// The broadest of the three narrowings and the first one you reach: a city, then
    /// one of its searches, then a state. It is the city that **first found** the
    /// business, kept on the row itself — unlike the area above it, which has to be
    /// looked up through the searches made in it and comes loose when an area is
    /// forgotten.
    pub city: Option<i64>,
    /// Turn the list inside out: **only** what you have hidden.
    ///
    /// For counting them, and for the panel that offers them back. Everything else
    /// leaves them out.
    pub hidden_only: bool,
    /// Which day it is in Budapest, Monday being 0.
    ///
    /// Only the "open today" order uses it. It is a field rather than a call inside
    /// the ordering so a test can ask for a Sunday without waiting for one.
    pub today: u8,
    /// Only businesses rung at least this many times with no answer.
    ///
    /// The option that belongs to the "no answer" state: past two or three tries you
    /// are looking at a different kind of lead.
    pub rings_at_least: Option<i64>,
    /// Only businesses promised a call back no later than this.
    ///
    /// A timestamp our own clock produced — see [`webleads_domain::Due`] — never
    /// text off a request.
    pub due_before: Option<String>,
    /// Include the businesses that said no.
    ///
    /// Off for the list itself — that is what "not interested" means. On only for
    /// counting, so the button that offers to show them can say how many there are.
    pub every_state: bool,
}

impl Default for Wanted {
    fn default() -> Self {
        Self {
            status: None,
            search: None,
            least_reviews: None,
            least_rating: None,
            within_m: None,
            trade: None,
            why: None,
            sort: Sort::Distance,
            descending: false,
            limit: 50,
            offset: 0,
            from_run: None,
            from_area: None,
            city: None,
            hidden_only: false,
            rings_at_least: None,
            due_before: None,
            today: today_here().number_days_from_monday(),
            every_state: false,
        }
    }
}

impl Wanted {
    /// The extra `WHERE` clause this request needs, if any.
    ///
    /// Appended after [`ON_THE_LIST`] rather than living inside it, because a
    /// parameter added there has to be bound by every reader — and a reader that
    /// forgets simply stops working. `new_since` is an id we read from our own
    /// database, never text off the request, so there is nothing to smuggle in.
    pub fn narrowing(&self, area_runs: Option<&[i64]>) -> String {
        // One search. An id we read from our own database, never text off a request.
        let one_search = match self.from_run {
            Some(run) => format!("      AND b.first_run_id = {run}\n"),
            None => String::new(),
        };

        let hiding = if self.hidden_only {
            format!("      AND EXISTS ({HIDDEN_MATCH})\n")
        } else {
            format!("      AND NOT EXISTS ({HIDDEN_MATCH})\n")
        };

        // One area, as the searches that were made in it. The name itself never
        // reaches the query: it is looked up first and turned into the ids of our own
        // searches, so what lands here is a list of numbers we wrote ourselves.
        //
        // An area with no searches yet narrows to nothing, which is the truth —
        // rather than being quietly ignored and showing the whole list.
        let one_area = match area_runs {
            Some([]) => "      AND 0\n".to_owned(),
            Some(runs) => {
                let list: Vec<String> = runs.iter().map(i64::to_string).collect();
                format!("      AND b.first_run_id IN ({})\n", list.join(","))
            }
            None => String::new(),
        };

        // One city. An id, and one we can check is a number — the widest narrowing
        // there is, and the one every other clause sits underneath.
        let one_city = match self.city.filter(|id| *id > 0) {
            Some(id) => format!("      AND b.city_id = {id}\n"),
            None => String::new(),
        };

        // The two options that belong to a state. A number we clamped ourselves and a
        // timestamp our own clock wrote — nothing here comes off the request as text.
        let rung = match self.rings_at_least.filter(|n| *n > 0) {
            Some(least) => format!(
                "      AND COALESCE(c.not_reached_counter, 0) >= {}\n",
                least.clamp(1, 99)
            ),
            None => String::new(),
        };
        let due = match self.due_before.as_deref().filter(|w| is_a_moment(w)) {
            Some(when) => format!(
                "      AND c.follow_up_tstamp IS NOT NULL AND c.follow_up_tstamp <= '{when}'\n"
            ),
            None => String::new(),
        };

        format!("{one_city}{one_search}{one_area}{rung}{due}{hiding}")
    }

    /// The `ORDER BY` for this request.
    ///
    /// **Built from a `match`, never from text off the request.** The obvious way to
    /// write this is to paste the column name straight into the query, and that is
    /// how somebody ends up sorting by `name; DROP TABLE lead`. A [`Sort`] cannot
    /// hold anything we did not write down, so every branch below is a fragment we
    /// wrote ourselves.
    ///
    /// `ahead` is how many minutes each city is in front of your clock, worked out
    /// fresh by the caller. Only the "open today" order uses it, and only because the
    /// stored opening minute is the shop's own.
    pub fn order_by(&self, ahead: &[(i64, i32)]) -> String {
        let way = if self.descending { "DESC" } else { "ASC" };

        let column: String = match self.sort {
            Sort::Name => "b.name COLLATE NOCASE".to_owned(),
            Sort::Trade => {
                "COALESCE(NULLIF(TRIM(b.trade), ''), b.category) COLLATE NOCASE".to_owned()
            }
            // The canonical number, so `+36 1…` and `06 1…` land together.
            Sort::Phone => "COALESCE(b.phone_canonical, b.phone)".to_owned(),
            Sort::Why => "l.verdict".to_owned(),
            // How much it matters — broken, then unknown, then fine.
            Sort::Certificate => "l.tls_rank".to_owned(),
            // When they open today, in minutes. A shop that is shut has no opening
            // time and belongs at the end whichever way round you sort.
            // The column holds the shop's own opening minute, so a Dubai shop that
            // opens at ten reads 600 while a Budapest one that opens at ten also
            // reads 600 — and they are two hours apart. Shifted onto your clock
            // here, from the city already joined, because a stored offset would be
            // an hour wrong for half the year.
            Sort::Hours => opens_here(self.today, ahead),
            Sort::Reviews => "l.reviews".to_owned(),
            Sort::Distance => "l.distance_m".to_owned(),
            Sort::FirstSeen => "b.first_seen".to_owned(),
            // By how much work is left, not by the stored word.
            Sort::Status => status_order(),
        };

        // Anything we do not know goes last, whichever way round you sort.
        //
        // SQLite puts NULL first when ascending, so without this a shop that is shut
        // today would lead the list of who to ring — and a business with no rating
        // would outrank one with five stars.
        let unknowns_last = match self.sort {
            Sort::Hours | Sort::Trade | Sort::Certificate => {
                format!("{column} IS NULL ASC, ")
            }
            _ => String::new(),
        };

        // A second column so the order is the same every time. Without it two
        // businesses with the same number of reviews swap places between page one
        // and page two, and a row appears twice while another is never seen.
        format!("{unknowns_last}{column} {way}, b.id ASC")
    }

    /// The search turned into something SQL's `LIKE` understands.
    ///
    /// `%` either side means "contains". The user's own `%` and `_` are escaped so a
    /// note containing a percent sign searches for a percent sign rather than
    /// matching everything.
    pub fn search_pattern(&self) -> Option<String> {
        let text = self.search.as_ref()?.trim();
        if text.is_empty() {
            return None;
        }
        let escaped = text.replace('%', "\\%").replace('_', "\\_");
        Some(format!("%{escaped}%"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_search_becomes_a_contains_pattern() {
        let want = Wanted {
            search: Some("borzas".into()),
            ..Default::default()
        };
        assert_eq!(want.search_pattern().as_deref(), Some("%borzas%"));
    }

    #[test]
    fn nothing_typed_is_no_search_at_all() {
        for typed in [None, Some(String::new()), Some("   ".into())] {
            let want = Wanted {
                search: typed.clone(),
                ..Default::default()
            };
            assert_eq!(want.search_pattern(), None, "for {typed:?}");
        }
    }

    #[test]
    fn the_wildcards_sql_understands_are_escaped() {
        // Unescaped, a bare `%` would mean "anything" and match every business —
        // so typing a percent sign would look like the search was broken.
        let want = Wanted {
            search: Some("50%".into()),
            ..Default::default()
        };
        let pattern = want.search_pattern().expect("a pattern");
        assert!(pattern.contains("\\%"), "got {pattern}");

        let underscore = Wanted {
            search: Some("a_b".into()),
            ..Default::default()
        };
        assert!(underscore
            .search_pattern()
            .expect("a pattern")
            .contains("\\_"));
    }

    #[test]
    fn surrounding_space_is_ignored() {
        let want = Wanted {
            search: Some("  borzas  ".into()),
            ..Default::default()
        };
        assert_eq!(want.search_pattern().as_deref(), Some("%borzas%"));
    }

    #[test]
    fn asking_for_nothing_in_particular_is_the_default() {
        let want = Wanted::default();
        assert_eq!(want.status, None);
        assert_eq!(want.search, None);
        assert_eq!(want.offset, 0);
        // The list never shows the refused unless asked; only counting turns it on.
        assert!(!want.every_state);
    }

    #[test]
    fn the_open_today_order_moves_each_city_onto_your_clock() {
        // The column holds the shop's own opening minute, so a Dubai shop that opens
        // at ten and a Budapest one that open at ten both read 600 — two hours apart.
        let said = opens_here(0, &[(2, 120), (3, 180)]);
        assert!(said.contains("b.opens_mon"), "{said}");
        assert!(said.contains("WHEN 2 THEN 120"), "{said}");
        assert!(said.contains("WHEN 3 THEN 180"), "{said}");
        assert!(
            said.contains("ELSE 0"),
            "a city on your own clock is not moved"
        );
    }

    #[test]
    fn with_every_city_on_your_clock_the_order_is_the_plain_column() {
        // One city, yours: no arithmetic worth doing.
        assert_eq!(opens_here(0, &[]), "b.opens_mon");
        assert_eq!(opens_here(6, &[]), "b.opens_sun");
    }

    #[test]
    fn the_day_is_an_index_into_our_own_list_and_cannot_run_off_the_end() {
        // It comes from a weekday number, but a wrong one must pick a column rather
        // than panic in the middle of a query.
        assert_eq!(opens_here(200, &[]), "b.opens_sun");
    }

    #[test]
    fn nothing_off_a_request_reaches_the_open_today_order() {
        // Both inputs are ours: a day index and integers we worked out from our own
        // city table. There is no text to smuggle anything in.
        let said = opens_here(2, &[(1, -60)]);
        assert!(said.contains("WHEN 1 THEN -60"), "{said}");
        assert!(!said.contains('\''), "{said}");
    }
}
