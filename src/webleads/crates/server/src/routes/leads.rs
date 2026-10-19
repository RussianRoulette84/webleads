//! Reading the call list, and the trades we search for.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use webleads_store::{StoredLead, Wanted};

use crate::error::{ApiError, ApiResult};
use crate::routes::{DEFAULT_LIMIT, MAX_LIMIT};
use crate::state::SharedState;

#[derive(Debug, Default, Deserialize)]
pub struct Paging {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Narrow the list to one state — "everyone I still have to ring".
    pub status: Option<String>,
    /// Free text over the name, address, trade, telephone and your own notes.
    pub search: Option<String>,
    /// Fewest reviews to show.
    pub least_reviews: Option<i64>,
    /// Lowest rating to show. Raising this also drops the unrated.
    pub least_rating: Option<f64>,
    /// Furthest away to show, in metres.
    pub within_m: Option<i64>,
    /// One trade only.
    pub trade: Option<String>,
    /// One reason only.
    pub why: Option<String>,
    /// Which column to order by.
    pub sort: Option<String>,
    /// Biggest or latest first.
    #[serde(default)]
    pub descending: bool,
    /// Only what one search was the first to find.
    pub from_run: Option<i64>,
    /// Only what the searches of one area first found.
    pub from_area: Option<String>,
    /// Only one city, as the top row of chips offers it.
    pub city: Option<i64>,
    /// Only businesses rung at least this many times with no answer.
    pub rings_at_least: Option<i64>,
    /// Only calls promised by "overdue", "today" or "week".
    pub due: Option<String>,
}

impl Paging {
    /// What the caller is actually asking for, with the limits applied.
    ///
    /// The ceiling is there so a mistyped `limit=100000` cannot ask the database for
    /// everything at once. An unknown status narrows to nothing rather than failing:
    /// the page builds it from its own buttons, so a word we do not know means a bug
    /// here, not a caller doing something odd.
    pub fn wanted(&self) -> Wanted {
        Wanted {
            status: self.status.as_deref().and_then(|s| s.parse().ok()),
            search: self.search.clone(),
            // A slider at its minimum sends nothing, and nothing means no narrowing.
            least_reviews: self.least_reviews.filter(|n| *n > 0),
            least_rating: self.least_rating.filter(|r| *r > 0.0),
            within_m: self.within_m.filter(|m| *m > 0),
            trade: self.trade.clone().filter(|t| !t.trim().is_empty()),
            why: self.why.as_deref().and_then(|w| w.parse().ok()),
            // An unknown column falls back to the default order rather than failing
            // the request — the page builds these from its own headings, so an
            // unknown one means a bug here, not a caller doing something odd. And a
            // `Sort` cannot hold anything we did not write down, so there is nothing
            // to smuggle in.
            sort: self
                .sort
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
            descending: self.descending,
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: self.offset.unwrap_or(0).max(0),
            from_run: self.from_run,
            // A blank name narrows nothing, the same as a blank trade.
            from_area: self.from_area.clone().filter(|a| !a.trim().is_empty()),
            // An id, and only a real one: a nought would narrow to nothing under a
            // chip that says otherwise.
            city: self.city.filter(|id| *id > 0),
            rings_at_least: self.rings_at_least.filter(|n| *n > 0),
            // A word, turned into a moment by our own clock. The page cannot work
            // this out: knowing what time it is in Budapest needs a timezone
            // database, and a WebAssembly build has none.
            due_before: self
                .due
                .as_deref()
                .and_then(|w| w.parse::<webleads_domain::Due>().ok())
                .map(webleads_domain::Due::cut_off_now),
            // Never off a request: the hidden are shown by the panel that offers
            // them back, not by an address somebody can type.
            hidden_only: false,
            // Which day it is decides which day's opening time the hours order uses.
            // The browser cannot work this out — it has no timezone database.
            today: webleads_domain::today_here().number_days_from_monday(),
            // The list itself never shows the refused unless you ask for them by
            // name; only the counting turns this on.
            every_state: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LeadsResponse {
    /// How many came back on this page.
    pub count: usize,
    /// How many are in each state, so the page can say "3 to call, 1 no answer".
    pub counts: Vec<(String, i64)>,
    /// The trades actually on the list, for the dropdown.
    pub trades: Vec<(String, i64)>,
    /// The cities with anything on the list: which one, what it is called, and how
    /// many. The top row of chips is built from this.
    pub cities: Vec<(i64, String, i64)>,
    /// The language the list as a whole reads in: the chosen city's if one is chosen,
    /// otherwise the place we are searching.
    ///
    /// The trade dropdown and the trade picker use it. Each row carries its own
    /// city's language separately, so a mixed list reads correctly row by row — this
    /// is for the controls, which belong to no single row.
    pub lang: String,
    /// How far the sliders need to reach, and what the rating one would hide.
    pub extremes: webleads_store::Extremes,
    /// How many there are altogether, so the page can say "50 of 380".
    pub total: i64,
    /// How many this request is keeping off the screen because you hid them.
    pub hidden_count: i64,
    /// The city whose clock is yours, which is the one the page opens on.
    pub home_city: Option<i64>,
    /// The pictures we already hold, by business id.
    pub shots: std::collections::HashMap<String, crate::routes::shots::Shot>,
    pub offset: i64,
    pub leads: Vec<StoredLead>,
}

/// `GET /api/leads?limit=&offset=` — the call list, best first.
///
/// Fifty at a time by default rather than five hundred in one lump. The ceiling is
/// there so a mistyped `limit=100000` cannot ask the database for everything at once.
pub async fn leads(
    State(state): State<SharedState>,
    Query(paging): Query<Paging>,
) -> ApiResult<Json<LeadsResponse>> {
    let want = paging.wanted();

    // The same `want` goes to every one of them, which is what keeps the numbers on
    // screen from contradicting each other.
    let leads = state.store.leads(&want).await?;
    let total = state.store.lead_count(&want).await?;
    let counts = state
        .store
        .status_counts(&want)
        .await?
        .into_iter()
        .map(|(status, n)| (status.as_str().to_owned(), n))
        .collect();

    let trades = state.store.trades_on_the_list(&want).await?;
    let cities = state.store.cities_on_the_list(&want).await?;
    let lang = list_language(&state, want.city).await;
    let extremes = state.store.extremes().await?;
    let hidden_count = state.store.hidden_count(&want).await?;
    let home_city = state.store.home_city().await?;
    // Read from the folder, so a picture that has been deleted stops being offered.
    let shots = crate::routes::shots::taken(&state.shots_dir, &leads).await;

    Ok(Json(LeadsResponse {
        count: leads.len(),
        total,
        hidden_count,
        home_city,
        shots,
        offset: want.offset,
        counts,
        trades,
        cities,
        lang,
        extremes,
        leads,
    }))
}

/// Which language the list's own controls should read in.
///
/// The city you have narrowed to, if you have narrowed to one — that is the list you
/// are looking at. Otherwise the city of the place you are searching, because that is
/// the work in front of you.
///
/// **Empty when we do not know**, rather than the program's default: the page can say
/// "we do not know", where a default would quietly pretend.
async fn list_language(state: &SharedState, chosen: Option<i64>) -> String {
    state.searching_language(chosen).await.unwrap_or_default()
}

/// What the next search would do: the trades, where it would look, and in what
/// language.
///
/// One answer rather than three, because the panel needs all of it at once and
/// asking separately was a race it lost: the trades arrived first, the language was
/// still unknown, and the picker's tier counts froze at nought.
#[derive(Debug, Serialize)]
pub struct Trades {
    pub trades: Vec<webleads_domain::Target>,
    /// The language searches are made in, as last picked. Empty when it could not be
    /// read — the page says so rather than guessing.
    pub lang: String,
    /// The name of that place, which is also what gets appended to every search word.
    pub place: String,
}

/// `GET /api/targets` — the trades we search for, already in rank order.
pub async fn targets(State(state): State<SharedState>) -> Json<Trades> {
    Json(Trades {
        trades: state.targets(),
        lang: state.store.search_lang().await.unwrap_or_else(|err| {
            tracing::warn!("⚠️ WARNING: could not read the search language: {err}");
            String::new()
        }),
        place: state.searching().label,
    })
}

/// `PUT /api/targets` — replace the trade list with an edited one.
///
/// The whole list, not one row: the file is the list, and sending a row at a time
/// would mean deciding what a row's identity is and what happens when two people
/// edit at once. There is one of you.
///
/// Refused rather than half-applied when anything in it is unusable — a trade with
/// no name, or no word to search for in any language, would sit on the screen for
/// ever being impossible to search.
pub async fn save_targets(
    State(state): State<SharedState>,
    Json(edited): Json<Vec<webleads_domain::Target>>,
) -> ApiResult<Json<Vec<webleads_domain::Target>>> {
    if let Err(wrong) = check_targets(&edited) {
        return Err(ApiError::BadRequest(wrong));
    }

    let mut kept = edited;
    // Rank order, as the file is kept and the page offers it. Sorted here so an
    // edited rank takes effect without the page having to re-sort itself.
    kept.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.english.cmp(&b.english)));

    state
        .set_targets(kept.clone())
        .map_err(|e| ApiError::Internal(format!("could not write the trades: {e}")))?;
    Ok(Json(kept))
}

/// Is this list of trades usable? The reason it is not, if it is not.
///
/// Pure, and tested. The same rules the file's reader applies, said once so an edit
/// on the page cannot write something the next start-up would silently drop.
pub fn check_targets(targets: &[webleads_domain::Target]) -> Result<(), String> {
    if targets.is_empty() {
        return Err("a list with no trades in it would leave nothing to search".to_owned());
    }

    let mut seen: Vec<String> = Vec::new();
    for target in targets {
        let name = target.english.trim();
        if name.is_empty() {
            return Err("every trade needs a name".to_owned());
        }
        // The name is the key: every business already on the list records its trade
        // by it, so two trades of one name would be one trade with two sets of words
        // and no way to tell which a lead came from.
        let lowered = name.to_lowercase();
        if seen.contains(&lowered) {
            return Err(format!("there are two trades called {name}"));
        }
        seen.push(lowered);

        if target.languages().next().is_none() {
            return Err(format!(
                "{name} has no word to search for in any language, so it could never \
                 be searched — give it one or take the row out"
            ));
        }
        if !(1..=5).contains(&target.no_site_odds) {
            return Err(format!("{name}: the odds of no website run from 1 to 5"));
        }
        if !(1..=5).contains(&target.budget) {
            return Err(format!("{name}: what they can pay runs from 1 to 5"));
        }
    }
    Ok(())
}

/// `GET /api/leads/pins` — every match, as a pin.
///
/// Its own route rather than part of the list because the map wants **all** of them
/// and the table wants one page. Sharing one route meant the map quietly showed
/// whatever page happened to be loaded.
pub async fn pins(
    State(state): State<SharedState>,
    Query(paging): Query<Paging>,
) -> ApiResult<Json<Vec<webleads_store::Pin>>> {
    // The same question the table is asked, so the two can never disagree.
    Ok(Json(state.store.pins(&paging.wanted()).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use webleads_domain::{Status, Target};

    fn asking(limit: Option<i64>, offset: Option<i64>) -> Paging {
        Paging {
            limit,
            offset,
            status: None,
            search: None,
            ..Default::default()
        }
    }

    #[test]
    fn a_page_size_nobody_asked_for_is_the_sensible_default() {
        let want = asking(None, None).wanted();
        assert_eq!(want.limit, DEFAULT_LIMIT);
        assert_eq!(want.offset, 0);
        assert_eq!(want.status, None);
        assert_eq!(want.search_pattern(), None);
    }

    #[test]
    fn a_silly_page_size_is_brought_back_into_range() {
        // A mistyped limit=100000 must not ask the database for everything at once,
        // and limit=0 or a negative offset must not produce an empty page forever.
        for (asked, expected) in [(100_000, MAX_LIMIT), (0, 1), (-5, 1), (75, 75)] {
            assert_eq!(
                asking(Some(asked), None).wanted().limit,
                expected,
                "limit={asked}"
            );
        }
        for (asked, expected) in [(-20, 0), (0, 0), (150, 150)] {
            assert_eq!(
                asking(None, Some(asked)).wanted().offset,
                expected,
                "offset={asked}"
            );
        }
    }

    #[test]
    fn a_status_the_page_does_not_know_narrows_to_nothing_rather_than_failing() {
        // The page builds this from its own buttons, so an unknown word means a bug
        // here, not a caller doing something odd. Failing the whole request would be
        // worse than showing everybody.
        let odd = Paging {
            status: Some("banana".into()),
            ..asking(None, None)
        };
        assert_eq!(odd.wanted().status, None);

        let real = Paging {
            status: Some("not_reached".into()),
            ..asking(None, None)
        };
        assert_eq!(real.wanted().status, Some(Status::NotReached));
    }

    #[test]
    fn the_search_is_carried_through_untouched() {
        let searching = Paging {
            search: Some("borzas".into()),
            ..asking(None, None)
        };
        assert_eq!(searching.wanted().search.as_deref(), Some("borzas"));
        assert_eq!(
            searching.wanted().search_pattern().as_deref(),
            Some("%borzas%")
        );
    }

    #[test]
    fn the_paging_and_the_count_are_always_asked_the_same_question() {
        // The bug this replaced: the list and the counts were two different pieces of
        // SQL, and one of them counted refused businesses. Building one `Wanted` and
        // handing it to all three is what stops that happening again.
        let asked = Paging {
            limit: Some(10),
            offset: Some(20),
            status: Some("not_reached".into()),
            search: Some("hair".into()),
            ..Default::default()
        };
        let first = asked.wanted();
        let second = asked.wanted();
        assert_eq!(first.status, second.status);
        assert_eq!(first.search, second.search);
        assert_eq!(first.limit, second.limit);
        assert_eq!(first.offset, second.offset);
    }

    #[test]
    fn an_unknown_column_falls_back_to_the_default_order() {
        // The page builds these from its own headings, so an unknown one means a bug
        // here rather than a caller doing something odd. Failing the whole request
        // would be worse than showing the usual order.
        let odd = Paging {
            sort: Some("banana".into()),
            ..Default::default()
        };
        assert_eq!(odd.wanted().sort, webleads_domain::Sort::Distance);
    }

    #[test]
    fn an_attempt_to_smuggle_sql_through_the_sort_gets_the_default_order() {
        // The real defence is that `Sort` cannot hold anything we did not write
        // down. This asserts the route agrees.
        for nasty in ["name; DROP TABLE lead", "reviews, (SELECT 1)", "1=1--"] {
            let asked = Paging {
                sort: Some(nasty.into()),
                ..Default::default()
            };
            let want = asked.wanted();
            assert_eq!(want.sort, webleads_domain::Sort::Distance, "`{nasty}`");
            // And what actually reaches the query is text we wrote ourselves.
            assert!(!want.order_by(&[]).contains("DROP"), "`{nasty}`");
            assert!(!want.order_by(&[]).contains("SELECT"), "`{nasty}`");
        }
    }

    #[test]
    fn a_real_column_is_carried_through_with_its_direction() {
        let asked = Paging {
            sort: Some("reviews".into()),
            descending: true,
            ..Default::default()
        };
        let want = asked.wanted();
        assert_eq!(want.sort, webleads_domain::Sort::Reviews);
        assert!(want.descending);
        assert!(want.order_by(&[]).contains("DESC"));
    }

    #[test]
    fn a_slider_at_its_minimum_narrows_nothing() {
        // Zero means "I have not touched this", not "show me businesses with zero
        // reviews" — which would be every one of them anyway.
        let untouched = Paging {
            least_reviews: Some(0),
            least_rating: Some(0.0),
            within_m: Some(0),
            ..Default::default()
        };
        let want = untouched.wanted();
        assert_eq!(want.least_reviews, None);
        assert_eq!(want.least_rating, None);
        assert_eq!(want.within_m, None);
    }

    #[test]
    fn a_slider_that_has_been_moved_does_narrow() {
        let moved = Paging {
            least_reviews: Some(20),
            least_rating: Some(4.0),
            within_m: Some(3000),
            ..Default::default()
        };
        let want = moved.wanted();
        assert_eq!(want.least_reviews, Some(20));
        assert_eq!(want.least_rating, Some(4.0));
        assert_eq!(want.within_m, Some(3000));
    }

    #[test]
    fn an_empty_trade_is_the_same_as_choosing_none() {
        for blank in [Some(String::new()), Some("   ".into()), None] {
            let asked = Paging {
                trade: blank.clone(),
                ..Default::default()
            };
            assert_eq!(asked.wanted().trade, None, "for {blank:?}");
        }
    }

    #[test]
    fn a_reason_we_do_not_recognise_narrows_to_nothing_rather_than_failing() {
        let odd = Paging {
            why: Some("banana".into()),
            ..Default::default()
        };
        assert_eq!(odd.wanted().why, None);

        let real = Paging {
            why: Some("no_site".into()),
            ..Default::default()
        };
        assert_eq!(real.wanted().why, Some(webleads_domain::Verdict::NoSite));
    }

    #[test]
    fn the_whole_filter_is_built_once_and_shared() {
        // The list, the count, the trades and the spreadsheet are all asked the same
        // question. Building it twice is how two numbers on screen start disagreeing.
        let asked = Paging {
            least_reviews: Some(20),
            trade: Some("Barbers".into()),
            sort: Some("distance".into()),
            descending: true,
            ..Default::default()
        };
        let first = asked.wanted();
        let second = asked.wanted();
        assert_eq!(first.least_reviews, second.least_reviews);
        assert_eq!(first.trade, second.trade);
        assert_eq!(first.sort, second.sort);
        assert_eq!(first.order_by(&[]), second.order_by(&[]));
    }

    // ---- saving an edited trade list ----

    fn one_trade(english: &str, words: &[(&str, &[&str])], odds: u8, budget: u8) -> Target {
        Target {
            rank: 1,
            tier: webleads_domain::Tier::S,
            english: english.to_owned(),
            words: words
                .iter()
                .map(|(lang, said)| {
                    (
                        (*lang).to_owned(),
                        said.iter().map(|w| (*w).to_owned()).collect(),
                    )
                })
                .collect(),
            no_site_odds: odds,
            budget,
            note: String::new(),
        }
    }

    #[test]
    fn a_usable_trade_list_is_accepted() {
        let list = [one_trade(
            "Plumbers",
            &[("hu", &["vízvezeték-szerelő"])],
            4,
            4,
        )];
        assert!(check_targets(&list).is_ok());
    }

    #[test]
    fn a_trade_with_no_name_is_refused() {
        let list = [one_trade("   ", &[("hu", &["valami"])], 4, 4)];
        assert!(check_targets(&list).is_err());
    }

    #[test]
    fn a_trade_with_no_word_in_any_language_is_refused() {
        // It would sit on the screen for ever being impossible to search.
        let list = [one_trade("Plumbers", &[], 4, 4)];
        let refused = check_targets(&list).expect_err("refused");
        assert!(refused.contains("Plumbers"), "{refused}");
    }

    #[test]
    fn two_trades_of_one_name_are_refused() {
        // The name is the key every business records its trade by, so two of them would
        // be one trade with two sets of words and no way to tell them apart.
        let list = [
            one_trade("Plumbers", &[("hu", &["a"])], 4, 4),
            one_trade("plumbers", &[("en", &["b"])], 4, 4),
        ];
        assert!(check_targets(&list).is_err());
    }

    #[test]
    fn a_score_outside_one_to_five_is_refused() {
        assert!(check_targets(&[one_trade("A", &[("hu", &["a"])], 0, 4)]).is_err());
        assert!(check_targets(&[one_trade("A", &[("hu", &["a"])], 6, 4)]).is_err());
        assert!(check_targets(&[one_trade("A", &[("hu", &["a"])], 4, 0)]).is_err());
    }

    #[test]
    fn an_empty_list_is_refused() {
        // Saving one by accident would leave nothing to search and no way back except
        // the file on disk.
        assert!(check_targets(&[]).is_err());
    }
}
