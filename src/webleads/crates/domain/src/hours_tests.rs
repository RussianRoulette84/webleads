//! Tests for [`crate::hours`].

use super::*;

#[test]
fn the_first_clock_is_one_the_clock_actually_knows() {
    assert!(zone_named(FIRST_ZONE).is_some());
    assert_eq!(your_zone_name(), FIRST_ZONE);
}

#[test]
fn a_zone_is_found_by_its_name_and_a_typo_is_not() {
    assert!(zone_named(" Asia/Dubai ").is_some());
    assert!(zone_named("Europe/Budapset").is_none());
    assert!(zone_named("").is_none());
}

#[test]
fn every_zone_offered_is_one_the_clock_knows_in_order() {
    let zones = all_zones();
    assert!(zones.len() > 300, "{}", zones.len());
    assert!(zones.contains(&"Europe/Budapest") && zones.contains(&"Asia/Dubai"));
    assert!(zones.windows(2).all(|w| w[0] < w[1]), "A to Z, once each");
}

/// Exactly what a real Dutch search returned, narrow spaces and all.
const A_REAL_DUTCH_WEEK: &str = r#"{"Friday":["8 am–5 pm"],"Monday":["8 am–5 pm"],"Saturday":["9 am–4 pm"],"Sunday":["Closed"],"Thursday":["8 am–5 pm"],"Tuesday":["8 am–8 pm"],"Wednesday":["8 am–5 pm"]}"#;

/// What a Hungarian search is expected to return. Google answers in the language it
/// was asked in, and the same database holds rows from both.
const A_HUNGARIAN_WEEK: &str = r#"{"Hétfő":["08:00–17:00"],"Kedd":["08:00–17:00"],"Szerda":["08:00–17:00"],"Csütörtök":["08:00–17:00"],"Péntek":["08:00–16:00"],"Szombat":["09:00–13:00"],"Vasárnap":["Zárva"]}"#;

#[test]
fn a_real_dutch_week_is_read_in_full() {
    // Taken from `out/test.csv`, so this is proved against data Google actually sent.
    let hours = Hours::parse(A_REAL_DUTCH_WEEK);
    assert!(hours.known());
    assert_eq!(hours.days_known(), 7);
    assert_eq!(
        hours.line_for(Weekday::Monday).as_deref(),
        Some("8 am–5 pm")
    );
    assert_eq!(
        hours.line_for(Weekday::Tuesday).as_deref(),
        Some("8 am–8 pm")
    );
    assert_eq!(hours.line_for(Weekday::Sunday).as_deref(), Some("Closed"));
}

#[test]
fn a_hungarian_week_is_read_just_as_well() {
    let hours = Hours::parse(A_HUNGARIAN_WEEK);
    assert_eq!(hours.days_known(), 7);
    assert_eq!(
        hours.line_for(Weekday::Monday).as_deref(),
        Some("08:00–17:00")
    );
    assert_eq!(
        hours.line_for(Weekday::Saturday).as_deref(),
        Some("09:00–13:00")
    );
    assert_eq!(hours.line_for(Weekday::Sunday).as_deref(), Some("Zárva"));
}

#[test]
fn the_invisible_spaces_google_uses_are_turned_into_ordinary_ones() {
    // A narrow no-break space cannot be seen or typed, and survives into every
    // export and clipboard paste looking like a mistake.
    let hours = Hours::parse(A_REAL_DUTCH_WEEK);
    let monday = hours.line_for(Weekday::Monday).expect("a line");
    assert!(!monday.contains('\u{202f}'), "an invisible space survived");
    assert!(!monday.contains('\u{00a0}'));
    assert_eq!(monday, "8 am–5 pm");
}

#[test]
fn accents_are_not_required_to_match_a_day() {
    // In case a scrape ever comes back without them.
    let plain = r#"{"Hetfo":["08:00–17:00"],"Csutortok":["09:00–18:00"]}"#;
    let hours = Hours::parse(plain);
    assert_eq!(
        hours.line_for(Weekday::Monday).as_deref(),
        Some("08:00–17:00")
    );
    assert_eq!(
        hours.line_for(Weekday::Thursday).as_deref(),
        Some("09:00–18:00")
    );
}

#[test]
fn capitals_do_not_matter() {
    let hours = Hours::parse(r#"{"MONDAY":["9–5"],"tuesday":["9–5"]}"#);
    assert_eq!(hours.days_known(), 2);
}

#[test]
fn a_shop_that_shuts_for_lunch_reads_as_one_line() {
    // Two openings in a day is how a person would say it, with a comma.
    let split = r#"{"Monday":["09:00–12:00","14:00–18:00"]}"#;
    let hours = Hours::parse(split);
    assert_eq!(
        hours.line_for(Weekday::Monday).as_deref(),
        Some("09:00–12:00, 14:00–18:00")
    );
    assert_eq!(hours.on(Weekday::Monday).expect("lines").len(), 2);
}

#[test]
fn fast_mode_sends_nothing_and_that_is_not_an_error() {
    // `{}` is exactly what fast mode returns, and it is most of what we have.
    for nothing in ["{}", "", "   ", "null"] {
        let hours = Hours::parse(nothing);
        assert!(!hours.known(), "`{nothing}` should be no hours at all");
        assert_eq!(hours.line_for(Weekday::Monday), None);
    }
}

#[test]
fn rubbish_never_stops_a_business_being_shown() {
    // Hours are a nicety. Refusing to show a lead because its opening times would
    // not parse would be the wrong trade entirely.
    for rubbish in ["not json", "[1,2,3]", r#"{"Monday":"9-5"}"#, "{{{"] {
        let hours = Hours::parse(rubbish);
        assert!(!hours.known(), "`{rubbish}` should simply give nothing");
    }
}

#[test]
fn a_day_we_cannot_place_is_dropped_rather_than_guessed_at() {
    // Showing Tuesday's hours on Wednesday is worse than showing nothing.
    let odd = r#"{"Monday":["9–5"],"Montag":["9–5"],"Lundi":["9–5"]}"#;
    let hours = Hours::parse(odd);
    assert_eq!(hours.days_known(), 1, "only the day we recognised");
    assert_eq!(hours.line_for(Weekday::Monday).as_deref(), Some("9–5"));
}

#[test]
fn a_day_with_no_lines_is_the_same_as_not_being_told() {
    let empty_day = r#"{"Monday":[],"Tuesday":["  "],"Wednesday":["9–5"]}"#;
    let hours = Hours::parse(empty_day);
    assert_eq!(hours.days_known(), 1);
    assert_eq!(hours.line_for(Weekday::Wednesday).as_deref(), Some("9–5"));
    assert_eq!(hours.line_for(Weekday::Monday), None);
}

#[test]
fn a_business_that_only_opens_some_days_says_so() {
    // Plenty of small trades work three days a week, and inventing the other four
    // would send you to a closed door.
    let partial = r#"{"Tuesday":["10:00–16:00"],"Thursday":["10:00–16:00"]}"#;
    let hours = Hours::parse(partial);
    assert!(hours.known());
    assert_eq!(hours.days_known(), 2);
    assert_eq!(hours.line_for(Weekday::Monday), None);
    assert_eq!(
        hours.line_for(Weekday::Tuesday).as_deref(),
        Some("10:00–16:00")
    );
}

#[test]
fn today_here_is_a_real_day() {
    // Mostly a guard that the timezone database is actually compiled in — without it
    // this would not build, and with the wrong one it would answer wrongly at
    // midnight.
    let today = today_here();
    assert!(today.number_days_from_monday() < 7);
}

#[test]
fn the_week_survives_being_written_down_and_read_back() {
    // It travels to the page as JSON, so this is the round trip that matters.
    let hours = Hours::parse(A_HUNGARIAN_WEEK);
    let written = serde_json::to_string(&hours).expect("writes");
    let read: Hours = serde_json::from_str(&written).expect("reads");
    assert_eq!(read, hours);
    assert_eq!(
        read.line_for(Weekday::Saturday).as_deref(),
        Some("09:00–13:00")
    );
}

#[test]
fn the_week_survives_being_flattened_into_something_else() {
    // The failure this guards against: `serde`'s `flatten` buffers the JSON before
    // reading it, which loses the knowledge that a key like `"0"` was meant to be a
    // number. With numeric keys the whole call list failed to parse and the table
    // simply did not appear — and the plain round-trip test above passed happily,
    // because on its own it works.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct AsThePageSeesIt {
        name: String,
        #[serde(flatten)]
        inner: Inner,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Inner {
        hours: Hours,
    }

    let sent = AsThePageSeesIt {
        name: "Borzas Barber".into(),
        inner: Inner {
            hours: Hours::parse(A_HUNGARIAN_WEEK),
        },
    };
    let json = serde_json::to_string(&sent).expect("writes");
    let back: AsThePageSeesIt = serde_json::from_str(&json).expect("reads back flattened");

    assert_eq!(back.inner.hours.days_known(), 7);
    assert_eq!(
        back.inner.hours.line_for(Weekday::Sunday).as_deref(),
        Some("Zárva")
    );
}

// ---- opening times, for sorting ----

#[test]
fn a_twenty_four_hour_clock_time_is_read_into_minutes() {
    let hours = Hours::parse(A_HUNGARIAN_WEEK);
    // 08:00 is four hundred and eighty minutes past midnight.
    assert_eq!(hours.opens_at(Weekday::Monday), Some(8 * 60));
    assert_eq!(hours.opens_at(Weekday::Saturday), Some(9 * 60));
}

#[test]
fn an_english_am_pm_time_is_read_too() {
    let hours = Hours::parse(A_REAL_DUTCH_WEEK);
    assert_eq!(hours.opens_at(Weekday::Monday), Some(8 * 60));
    assert_eq!(hours.opens_at(Weekday::Saturday), Some(9 * 60));
}

#[test]
fn an_afternoon_opening_is_not_read_as_a_morning_one() {
    let afternoon = Hours::parse(r#"{"Monday":["1 pm–9 pm"],"Tuesday":["13:30–19:00"]}"#);
    assert_eq!(afternoon.opens_at(Weekday::Monday), Some(13 * 60));
    assert_eq!(afternoon.opens_at(Weekday::Tuesday), Some(13 * 60 + 30));
}

#[test]
fn noon_is_midday_and_not_midnight_plus_twelve() {
    // The classic one. "12 pm" means noon; adding twelve would give midnight.
    let noon = Hours::parse(r#"{"Monday":["12 pm–8 pm"]}"#);
    assert_eq!(noon.opens_at(Weekday::Monday), Some(12 * 60));
}

#[test]
fn a_closed_day_has_no_opening_time_in_either_language() {
    let hu = Hours::parse(r#"{"Monday":["Zárva"]}"#);
    assert_eq!(hu.opens_at(Weekday::Monday), None);
    let en = Hours::parse(r#"{"Monday":["Closed"]}"#);
    assert_eq!(en.opens_at(Weekday::Monday), None);
}

#[test]
fn a_day_we_were_told_nothing_about_has_no_opening_time() {
    let partial = Hours::parse(r#"{"Tuesday":["09:00–17:00"]}"#);
    assert_eq!(partial.opens_at(Weekday::Monday), None);
    assert_eq!(partial.opens_at(Weekday::Tuesday), Some(9 * 60));
}

#[test]
fn a_line_we_cannot_read_gives_nothing_rather_than_a_guess() {
    // A wrong opening time would send you to a locked door.
    for odd in ["by appointment", "ring first", "99:99–10:00", ""] {
        let hours = Hours::parse(&format!(r#"{{"Monday":["{odd}"]}}"#));
        assert_eq!(hours.opens_at(Weekday::Monday), None, "`{odd}`");
    }
}

#[test]
fn open_around_the_clock_is_as_early_as_it_gets() {
    let always = Hours::parse(r#"{"Monday":["Open 24 hours"]}"#);
    assert_eq!(always.opens_at(Weekday::Monday), Some(0));
}

#[test]
fn an_earlier_opening_sorts_before_a_later_one() {
    // The whole point of the number: an order that means something.
    let early = Hours::parse(r#"{"Monday":["07:00–15:00"]}"#);
    let late = Hours::parse(r#"{"Monday":["11:00–19:00"]}"#);
    let shut = Hours::parse(r#"{"Monday":["Zárva"]}"#);

    assert!(early.opens_at(Weekday::Monday) < late.opens_at(Weekday::Monday));
    // And a shut shop is not the first one to telephone.
    assert!(late.opens_at(Weekday::Monday).is_some());
    assert!(shut.opens_at(Weekday::Monday).is_none());
}

// ---- the whole week ----

#[test]
fn the_week_comes_back_in_week_order_not_alphabetically() {
    // Google sends them alphabetically, which puts Friday first.
    let hours = Hours::parse(A_HUNGARIAN_WEEK);
    let week = hours.week();
    assert_eq!(week.len(), 7);
    assert_eq!(week[0].0, Weekday::Monday);
    assert_eq!(week[5].0, Weekday::Saturday);
    assert_eq!(week[6].0, Weekday::Sunday);
    assert_eq!(week[6].1, "Zárva");
}

#[test]
fn a_business_open_only_some_days_lists_only_those() {
    let partial = Hours::parse(r#"{"Tuesday":["10:00–16:00"],"Thursday":["10:00–16:00"]}"#);
    let week = partial.week();
    assert_eq!(week.len(), 2);
    assert_eq!(week[0].0, Weekday::Tuesday);
    assert_eq!(week[1].0, Weekday::Thursday);
}

#[test]
fn a_business_with_no_hours_has_an_empty_week_rather_than_seven_blanks() {
    assert!(Hours::parse("{}").week().is_empty());
}

#[test]
fn the_week_comes_back_with_names_the_page_can_print() {
    let hours = Hours::parse(r#"{"Monday":["9:00–17:00"],"Saturday":["Zárva"]}"#);
    let week = hours.week_named();

    // Monday first, Saturday after it — week order, not the order Google sent.
    assert_eq!(week[0].0, "Mon");
    assert_eq!(week[0].1, "9:00–17:00");
    assert_eq!(week[1].0, "Sat");
    assert_eq!(week[1].1, "Zárva");
}

#[test]
fn a_business_we_know_nothing_about_has_an_empty_week() {
    // Fast mode sends `{}`, and seven "unknown" lines on a row is noise.
    assert!(Hours::parse("{}").week_named().is_empty());
}

#[test]
fn the_whole_week_of_opening_times_comes_back_monday_first() {
    let hours = Hours::parse(r#"{"Monday":["08:00–17:00"],"Sunday":["10:30–14:00"]}"#);
    let week = hours.opens_each_day();

    assert_eq!(week[0], Some(8 * 60));
    assert_eq!(week[6], Some(10 * 60 + 30));
    // Nothing was said about the middle of the week, so nothing is claimed.
    assert_eq!(week[3], None);
}

#[test]
fn a_day_they_are_shut_has_no_opening_time() {
    // Which is what puts it at the end of the order rather than at the front.
    let hours = Hours::parse(r#"{"Monday":["Zárva"]}"#);
    assert_eq!(hours.opens_each_day()[0], None);
}

#[test]
fn every_day_of_a_full_week_is_read() {
    let hours = Hours::parse(A_HUNGARIAN_WEEK);
    let week = hours.opens_each_day();
    assert!(
        week.iter().filter(|opens| opens.is_some()).count() >= 5,
        "a full week came back mostly unknown: {week:?}"
    );
}

// ---- what day it is in Budapest ----

/// A moment in UTC, for the timezone tests.
fn utc(y: i32, m: u8, d: u8, h: u8, min: u8) -> time::OffsetDateTime {
    let month = time::Month::try_from(m).expect("month");
    let date = time::Date::from_calendar_date(y, month, d).expect("date");
    let time_of_day = time::Time::from_hms(h, min, 0).expect("time");
    time::PrimitiveDateTime::new(date, time_of_day).assume_utc()
}

#[test]
fn late_evening_in_london_is_already_tomorrow_in_budapest() {
    // This is the bug: 23:30 UTC on the 18th is 01:30 on the 19th in Budapest.
    // Stamping it with the 18th would make a business look older than it is, and
    // "new since the last search" would quietly start lying.
    assert_eq!(date_here(utc(2026, 9, 18, 23, 30)), "2026-09-19");
    assert_eq!(date_here(utc(2026, 9, 18, 21, 59)), "2026-09-18");
}

#[test]
fn the_offset_follows_summer_time_rather_than_being_fixed() {
    // Hungary is +2 in September and +1 in January. A hard-coded offset would be
    // wrong for roughly half the year, which is why the timezone database is here.
    assert_eq!(date_here(utc(2026, 9, 18, 22, 30)), "2026-09-19");
    assert_eq!(date_here(utc(2026, 1, 18, 22, 30)), "2026-01-18");
    assert_eq!(date_here(utc(2026, 1, 18, 23, 30)), "2026-01-19");
}

#[test]
fn midday_is_the_same_day_either_way() {
    assert_eq!(date_here(utc(2026, 9, 18, 12, 0)), "2026-09-18");
}

// ---- moments, for measuring how long something takes ----

#[test]
fn a_moment_has_a_time_on_the_end() {
    // The bug this fixes: a date alone means every search measured as nought seconds,
    // so the page could never say how long a run would take.
    let said = moment(utc(2026, 9, 19, 18, 41));
    assert_eq!(said, "2026-09-19 18:41:00");
    assert!(said.contains(':'), "no time in `{said}`");
}

#[test]
fn two_moments_an_hour_apart_are_different_strings() {
    assert_ne!(
        moment(utc(2026, 9, 19, 18, 0)),
        moment(utc(2026, 9, 19, 19, 0))
    );
}

#[test]
fn a_moment_is_written_the_way_a_database_reads_a_time() {
    // `YYYY-MM-DD HH:MM:SS` — the shape SQLite's own date maths understands. Getting
    // this wrong would silently give no answer rather than a wrong one.
    let said = moment(utc(2026, 1, 2, 3, 4));
    assert_eq!(said, "2026-01-02 03:04:00");
    assert_eq!(said.len(), 19);
}

#[test]
fn a_moment_is_utc_rather_than_budapest() {
    // These are only ever subtracted from each other. A clock that jumps an hour twice
    // a year would make one search of the night look an hour long.
    assert_eq!(moment(utc(2026, 9, 19, 23, 30)), "2026-09-19 23:30:00");
    // The Budapest date for that same moment is already tomorrow, which is exactly
    // the difference being avoided here.
    assert_eq!(date_here(utc(2026, 9, 19, 23, 30)), "2026-09-20");
}

// ---- the whole week, for the places that have one cell ----

#[test]
fn the_week_reads_as_one_line() {
    let hours = Hours::parse(r#"{"Monday":["08:00–17:00"],"Saturday":["Zárva"]}"#);
    let said = hours.week_line().expect("there is a week");
    assert_eq!(said, "Mon 08:00–17:00 · Sat closed");
}

#[test]
fn closed_says_closed_whatever_language_it_arrived_in() {
    // Google answers in the language it was asked in, and we ask in Hungarian.
    assert_eq!(plain_hours("Zárva"), "closed");
    assert_eq!(plain_hours("closed"), "closed");
    assert_eq!(plain_hours(" ZÁRVA "), "closed");
}

#[test]
fn unusual_hours_are_passed_through_as_written() {
    // Our guess at what a shop meant is worse than the shop's own words.
    assert_eq!(
        plain_hours("9:00–12:00, 14:00–18:00"),
        "9:00–12:00, 14:00–18:00"
    );
}

#[test]
fn a_business_with_no_hours_has_no_week_line() {
    // Fast mode sends none, and a blank must stay blank rather than read as closed.
    assert_eq!(Hours::parse("{}").week_line(), None);
}

#[test]
fn a_column_per_day_keeps_the_days_apart() {
    let hours = Hours::parse(r#"{"Monday":["08:00–17:00"],"Sunday":["Zárva"]}"#);
    let days = hours.day_lines();
    assert_eq!(days[0].as_deref(), Some("08:00–17:00"));
    assert_eq!(days[6].as_deref(), Some("closed"));
    // Nothing was said about Wednesday, and nothing is claimed.
    assert_eq!(days[2], None);
}

// ---- the clock in Budapest, for the times you wrote down yourself ----

#[test]
fn a_ring_back_time_is_read_in_budapest_rather_than_utc() {
    // The point of this one: a call set for two o'clock is two o'clock where the
    // shop is. Comparing it against UTC would call it overdue two hours early in
    // summer, and it is compared as plain text — so the shape matters too.
    assert_eq!(clock_here(utc(2026, 9, 20, 12, 0)), "2026-09-20 14:00");
    assert_eq!(clock_here(utc(2026, 1, 20, 12, 0)), "2026-01-20 13:00");
    // Late evening in London is already tomorrow there.
    assert_eq!(clock_here(utc(2026, 9, 20, 23, 30)), "2026-09-21 01:30");
}

#[test]
fn the_clock_is_written_the_way_the_ring_back_times_are_stored() {
    let said = clock_here(utc(2026, 1, 2, 3, 4));
    assert_eq!(said, "2026-01-02 04:04");
    assert_eq!(said.len(), 16);
}

// ---- reading both ends of a line, and moving them ----

#[test]
fn both_ends_of_a_span_are_read() {
    assert_eq!(span_minutes("08:00–17:00"), Some((8 * 60, 17 * 60)));
    assert_eq!(span_minutes("8 am–5 pm"), Some((8 * 60, 17 * 60)));
    assert_eq!(span_minutes("10 am–11 pm"), Some((10 * 60, 23 * 60)));
    assert_eq!(
        span_minutes("9:30–18:15"),
        Some((9 * 60 + 30, 18 * 60 + 15))
    );
}

#[test]
fn noon_and_midnight_are_not_twelve_plus_twelve() {
    assert_eq!(span_minutes("12 pm–12 am"), Some((12 * 60, 0)));
    assert_eq!(span_minutes("12 am–6 am"), Some((0, 6 * 60)));
}

#[test]
fn a_day_with_no_span_in_it_is_not_a_span() {
    // Closed, open all day, or a line we cannot read. Guessing one end of it would
    // be inventing hours.
    assert_eq!(span_minutes("Closed"), None);
    assert_eq!(span_minutes("Zárva"), None);
    assert_eq!(span_minutes("Open 24 hours"), None);
    assert_eq!(span_minutes("by appointment"), None);
    assert_eq!(span_minutes(""), None);
}

#[test]
fn a_time_is_written_the_same_way_every_time() {
    // A column of "8 am" beside "10:30 pm" is a column you have to read twice.
    assert_eq!(said_at(8 * 60), "08:00");
    assert_eq!(said_at(23 * 60 + 5), "23:05");
    assert_eq!(said_at(0), "00:00");
}

#[test]
fn a_dubai_shop_reads_in_your_own_hours() {
    // The whole point. Two hours behind them in summer.
    assert_eq!(shift_line("10 am–11 pm", -120), "08:00–21:00");
    // And three in winter, when Hungary is off summer time and the Gulf never was.
    assert_eq!(shift_line("10 am–11 pm", -180), "07:00–20:00");
}

#[test]
fn your_own_city_is_not_moved_at_all() {
    // Nought minutes costs nothing and must not reformat what Google wrote.
    assert_eq!(shift_line("Closed", 0), "Closed");
    assert_eq!(shift_line("by appointment", 0), "by appointment");
    // A span it can read is tidied into one shape, which is worth having.
    assert_eq!(shift_line("8 am–5 pm", 0), "08:00–17:00");
}

#[test]
fn what_we_cannot_read_comes_back_exactly_as_it_was() {
    assert_eq!(shift_line("Closed", -120), "Closed");
    assert_eq!(shift_line("Zárva", -120), "Zárva");
    assert_eq!(shift_line("Open 24 hours", -120), "Open 24 hours");
    assert_eq!(shift_line("  by appointment  ", -120), "by appointment");
}

#[test]
fn a_span_that_crosses_midnight_says_so_rather_than_moving_day() {
    // A shop open till two in the morning your time is still Monday's shop. Moving
    // it to Tuesday's row would be a lie about which day to ring.
    assert_eq!(shift_line("18:00–22:00", 240), "22:00–02:00 (+1)");
    // And the other way: shifted back past midnight, it opened yesterday.
    assert_eq!(shift_line("01:00–09:00", -120), "23:00–07:00 (+1)");
}

#[test]
fn a_whole_week_moves_together() {
    let week = Hours::parse(
        r#"{"Monday":["10 am–11 pm"],"Tuesday":["Closed"],"Wednesday":["9:00–17:30"]}"#,
    );
    let ours = week.shifted(-120);
    assert_eq!(
        ours.line_for(Weekday::Monday).as_deref(),
        Some("08:00–21:00")
    );
    assert_eq!(ours.line_for(Weekday::Tuesday).as_deref(), Some("Closed"));
    assert_eq!(
        ours.line_for(Weekday::Wednesday).as_deref(),
        Some("07:00–15:30")
    );
}

#[test]
fn a_day_that_shuts_for_lunch_keeps_both_of_its_lines() {
    let week = Hours::parse(r#"{"Monday":["9:00–12:00","14:00–18:00"]}"#);
    let ours = week.shifted(-60);
    assert_eq!(
        ours.line_for(Weekday::Monday).as_deref(),
        Some("08:00–11:00, 13:00–17:00")
    );
}

#[test]
fn shifting_by_nothing_still_tidies_the_spans_into_one_shape() {
    // A local row reading "9:00–17:00" beside a converted one reading "07:00–20:00"
    // is a column you have to read twice.
    let week = Hours::parse(r#"{"Monday":["9:00-17:00"],"Tuesday":["Zárva"]}"#);
    let same = week.shifted(0);
    assert_eq!(
        same.line_for(Weekday::Monday).as_deref(),
        Some("09:00–17:00")
    );
    // And leaves alone what it cannot read.
    assert_eq!(same.line_for(Weekday::Tuesday).as_deref(), Some("Zárva"));
}

#[test]
fn dubai_is_two_or_three_hours_ahead_of_you_and_never_anything_else() {
    // Hungary keeps summer time and the Gulf does not, so the gap is 120 minutes in
    // summer and 180 in winter. Both are checked against dates whose answer is known.
    let july = utc(2026, 7, 1, 12, 0);
    let january = utc(2026, 1, 15, 12, 0);
    assert_eq!(ahead_at("Asia/Dubai", july), 120);
    assert_eq!(ahead_at("Asia/Dubai", january), 180);
}

#[test]
fn your_own_zone_is_no_distance_from_itself() {
    assert_eq!(ahead_at(FIRST_ZONE, utc(2026, 7, 1, 12, 0)), 0);
    assert_eq!(ahead_at(FIRST_ZONE, utc(2026, 1, 15, 12, 0)), 0);
}

#[test]
fn a_clock_we_cannot_resolve_moves_nothing() {
    // Shifting by a guess would be worse than not shifting.
    assert_eq!(ahead_at("Asia/Dubay", utc(2026, 7, 1, 12, 0)), 0);
    assert_eq!(ahead_at("", utc(2026, 7, 1, 12, 0)), 0);
}
