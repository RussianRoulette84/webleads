//! When a business is open, out of whatever Google felt like sending us.
//!
//! # What this file is here to teach
//!
//! Parsing somebody else's loose JSON without letting its looseness leak into the
//! rest of the program. Google answers in the language it was asked in, spaces its
//! times with characters you cannot see, and sometimes sends `{}`. All of that is
//! dealt with here, and what comes out is a plain, ordinary type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::Weekday;

/// What a business's week looks like.
///
/// A `BTreeMap` keyed by the day rather than seven fields: most businesses send all
/// seven, some send three, and a map says "whatever we were told" without inventing
/// the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hours {
    /// Day number (Monday = 0) to the lines for that day. A shop that shuts for
    /// lunch sends two lines, which is why this is a list.
    ///
    /// **The key is a `String`, not a number**, and that is not an accident. This
    /// type is flattened into a lead on the way to the browser, and `serde`'s
    /// `flatten` buffers the JSON first — which loses the knowledge that `"0"` was
    /// meant to be read back as a number. With numeric keys the whole call list
    /// failed to parse and the table simply did not appear.
    ///
    /// Single digits sort the same as strings or as numbers, so nothing is lost.
    days: BTreeMap<String, Vec<String>>,
}

impl Hours {
    /// Read whatever the scraper put in the `open_hours` column.
    ///
    /// Never fails. Hours are a nicety; refusing to show a business because we could
    /// not read its opening times would be the wrong trade entirely.
    pub fn parse(json: &str) -> Self {
        let json = json.trim();
        if json.is_empty() || json == "{}" || json == "null" {
            return Self::default();
        }

        let Ok(raw) = serde_json::from_str::<BTreeMap<String, Vec<String>>>(json) else {
            return Self::default();
        };

        let mut days = BTreeMap::new();
        for (name, lines) in raw {
            // A day we cannot place is dropped rather than guessed at. Showing
            // Tuesday's hours on Wednesday is worse than showing nothing.
            let Some(day) = read_day(&name) else { continue };
            let tidy: Vec<String> = lines
                .into_iter()
                .map(|line| tidy_line(&line))
                .filter(|line| !line.is_empty())
                .collect();
            if !tidy.is_empty() {
                days.insert(key_for(day), tidy);
            }
        }
        Self { days }
    }

    /// The lines for one day, if we were told any.
    pub fn on(&self, day: Weekday) -> Option<&[String]> {
        self.days.get(&key_for(day)).map(Vec::as_slice)
    }

    /// One day's hours as a single readable line.
    ///
    /// Two lines join with a comma — a shop that shuts for lunch reads
    /// "9:00–12:00, 14:00–18:00", which is how a person would say it.
    pub fn line_for(&self, day: Weekday) -> Option<String> {
        self.on(day).map(|lines| lines.join(", "))
    }

    /// What time they open on this day, in minutes past midnight.
    ///
    /// `None` when they are closed, or when we were told nothing, or when the line
    /// is in a shape we cannot read. All three sort to the end rather than to the
    /// front — a shop that is shut is not the first one to telephone.
    ///
    /// This exists so the database can *sort* by opening time. Ordering by the raw
    /// JSON looked like it worked and grouped by whichever day name came first
    /// alphabetically, which is no order at all.
    pub fn opens_at(&self, day: Weekday) -> Option<u16> {
        let line = self.on(day)?.first()?;
        opening_minutes(line)
    }

    /// What time they open on each day of the week, Monday first.
    ///
    /// Seven values rather than one for today: the answer is stored, and a value
    /// worked out for "today" is wrong by tomorrow morning. The database sorts by
    /// whichever of the seven is today's.
    pub fn opens_each_day(&self) -> [Option<u16>; 7] {
        [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ]
        .map(|day| self.opens_at(day))
    }

    /// The same week, with every line moved by some number of minutes.
    ///
    /// **How a foreign shop's hours read on your own clock.** Google sends them in
    /// the shop's local time, which is the one thing you cannot dial by: you need to
    /// know when *you* should ring. A Dubai shop open `10 am–11 pm` is open
    /// `08:00–21:00` as far as you are concerned.
    ///
    /// Nought minutes still tidies every span into one shape. A business in your own
    /// city reading `9:00–17:00` beside a converted one reading `07:00–20:00` is a
    /// column you have to read twice, and the conversion is the only reason the
    /// second one is written that way.
    pub fn shifted(&self, by_minutes: i32) -> Self {
        Self {
            days: self
                .days
                .iter()
                .map(|(day, lines)| {
                    (
                        day.clone(),
                        lines
                            .iter()
                            .map(|line| shift_line(line, by_minutes))
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    /// Every day we know about, in week order, as `(day, line)`.
    ///
    /// For showing the whole week — "closed today, opens Monday at eight" is what
    /// you need on a Saturday, and we already hold it.
    pub fn week(&self) -> Vec<(Weekday, String)> {
        const WEEK: [Weekday; 7] = [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ];
        WEEK.into_iter()
            .filter_map(|day| self.line_for(day).map(|line| (day, line)))
            .collect()
    }

    /// The whole week as short day names and their lines.
    ///
    /// The naming happens here rather than on the page: a WebAssembly page has no
    /// timezone database, and "Mon" is not worth adding one for.
    pub fn week_named(&self) -> Vec<(&'static str, String)> {
        self.week()
            .into_iter()
            .map(|(day, line)| (short_name(day), line))
            .collect()
    }

    /// Each day's hours, Monday first, in English.
    ///
    /// Seven of them, for a spreadsheet that wants a column per day. `None` for a day
    /// we were told nothing about — which is not the same as being closed, and must
    /// not be written as though it were.
    pub fn day_lines(&self) -> [Option<String>; 7] {
        [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ]
        .map(|day| self.line_for(day).map(|line| plain_hours(&line)))
    }

    /// The whole week on one line: `Mon 8:00–17:00 · Tue closed · …`
    ///
    /// For the places that have one cell rather than seven — the row, the CSV, the
    /// page you keep. `None` when we were told nothing at all, so a blank stays
    /// blank rather than becoming a lie.
    pub fn week_line(&self) -> Option<String> {
        let week = self.week_named();
        if week.is_empty() {
            return None;
        }
        Some(
            week.into_iter()
                .map(|(day, line)| format!("{day} {}", plain_hours(&line)))
                .collect::<Vec<_>>()
                .join(" · "),
        )
    }

    /// Do we know anything at all about this business's week?
    ///
    /// Fast mode sends `{}`, so this is false for anything scraped that way.
    pub fn known(&self) -> bool {
        !self.days.is_empty()
    }

    /// How many days we were told about.
    pub fn days_known(&self) -> usize {
        self.days.len()
    }
}

/// Google's own word for a day with no trading, in English.
///
/// It answers in the language it was asked in, so a Hungarian search comes back
/// `Zárva`. Anything we do not recognise is passed through untouched — a shop with
/// unusual hours should read as it was written, not as our guess at it.
pub fn plain_hours(line: &str) -> String {
    let lowered = line.trim().to_lowercase();
    if lowered == "zárva" || lowered == "zarva" || lowered == "closed" {
        return "closed".to_owned();
    }
    if lowered.contains("24") && lowered.contains("óra") {
        return "open all day".to_owned();
    }
    line.trim().to_owned()
}

/// A day in three letters, for a table cell that has no room for "Wednesday".
fn short_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

/// How a day is keyed in the map. Monday is "0".
fn key_for(day: Weekday) -> String {
    day.number_days_from_monday().to_string()
}

/// Is this a day with no trading at all?
///
/// Google answers in the language it was asked in, so a Hungarian search comes back
/// `Zárva`.
fn says_closed(lowered: &str) -> bool {
    lowered.is_empty() || lowered.contains("zárva") || lowered.contains("closed")
}

/// Is this a day that never shuts?
fn says_all_day(lowered: &str) -> bool {
    lowered.contains("24") && (lowered.contains("hour") || lowered.contains("óra"))
}

/// Read one clock time out of a piece of a line: `08:00`, `8 am`, `5 pm`.
///
/// Minutes past midnight, or `None` when there is no time in there. Pure and used by
/// both edges of a line, so the opening and the closing are read by the same rules.
fn clock_minutes(piece: &str) -> Option<u16> {
    let piece = piece.trim().to_lowercase();
    let afternoon = piece.contains("pm");
    let morning = piece.contains("am");
    let digits = piece
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .trim_start_matches(|c: char| !c.is_ascii_digit());

    let (hours, minutes) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<u16>().ok()?, m.parse::<u16>().ok()?),
        None => (digits.parse::<u16>().ok()?, 0),
    };
    if hours > 23 || minutes > 59 {
        return None;
    }

    // "12 pm" is noon and "12 am" is midnight, neither of which is twelve plus
    // twelve.
    let hours = match (afternoon, morning, hours) {
        (true, _, 12) => 12,
        (true, _, h) => h + 12,
        (_, true, 12) => 0,
        (_, _, h) => h,
    };
    if hours > 23 {
        return None;
    }
    Some(hours * 60 + minutes)
}

/// Split a line of hours on whatever dash Google used.
///
/// An en dash usually, an em dash or a plain hyphen sometimes. `None` when there is
/// no dash at all, which means there is no closing time to find.
fn split_on_dash(line: &str) -> Option<(&str, &str)> {
    line.split_once(['–', '—', '-'])
}

/// Read the opening time out of a line of hours.
///
/// Handles what Google actually sends, in both languages we have seen:
/// `08:00–17:00`, `8 am–5 pm`, `Zárva`, `Closed`. Anything else gives `None` rather
/// than a guess — a wrong opening time would send you to a locked door.
fn opening_minutes(line: &str) -> Option<u16> {
    let lowered = line.trim().to_lowercase();
    if says_closed(&lowered) {
        return None;
    }
    // Open around the clock is as early as it gets.
    if says_all_day(&lowered) {
        return Some(0);
    }
    // The part before the dash is the opening time.
    let opens = split_on_dash(&lowered).map_or(lowered.as_str(), |(first, _)| first);
    clock_minutes(opens)
}

/// Both ends of a line of hours, in minutes past midnight.
///
/// `None` for a day that is closed, open all day, or written in a way we cannot read.
/// A line with no dash in it has no closing time and so is not a span — better to
/// leave it alone than to invent one end of it.
pub fn span_minutes(line: &str) -> Option<(u16, u16)> {
    let lowered = line.trim().to_lowercase();
    if says_closed(&lowered) || says_all_day(&lowered) {
        return None;
    }
    let (opens, shuts) = split_on_dash(&lowered)?;
    Some((clock_minutes(opens)?, clock_minutes(shuts)?))
}

/// One clock time written the way we show it: `08:00`.
///
/// Always twenty-four hour and always two digits, because a column of `8 am` beside
/// `10:30 pm` is a column you have to read twice.
pub fn said_at(minutes: u16) -> String {
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

/// The same line of hours, moved by some number of minutes.
///
/// **This is how a Dubai shop's hours read in your own time.** Their `10 am–11 pm`
/// becomes `08:00–21:00` when you are two hours behind them, which is the only form
/// of it you can act on: you are the one dialling.
///
/// A day that is closed, open all day, or written in a way we cannot read comes back
/// exactly as it was — shifting what we do not understand would be inventing hours.
/// Nought minutes is not a special case and returns the same thing.
///
/// A span that crosses midnight once shifted says so rather than moving itself to
/// another day: `22:00–02:00 (+1)` is a shop that shuts at two in the morning your
/// time, and pretending otherwise would put it on Tuesday's row.
pub fn shift_line(line: &str, by_minutes: i32) -> String {
    let Some((opens, shuts)) = span_minutes(line) else {
        return line.trim().to_owned();
    };
    if by_minutes == 0 {
        return format!("{}–{}", said_at(opens), said_at(shuts));
    }

    let moved = |minutes: u16| -> (u16, i32) {
        let total = i32::from(minutes) + by_minutes;
        // `rem_euclid` so a shift back past midnight lands on the previous day
        // rather than on a negative number.
        let day = total.div_euclid(MINUTES_IN_A_DAY);
        (
            u16::try_from(total.rem_euclid(MINUTES_IN_A_DAY)).unwrap_or(0),
            day,
        )
    };
    let (open_at, open_day) = moved(opens);
    let (shut_at, shut_day) = moved(shuts);

    let said = format!("{}–{}", said_at(open_at), said_at(shut_at));
    // Which day it shuts on, relative to the day it opens. Said out loud so an
    // evening shop does not read as one that shuts before it opens.
    match shut_day - open_day {
        0 => said,
        n if n > 0 => format!("{said} (+{n})"),
        n => format!("{said} ({n})"),
    }
}

/// Minutes in a day, for the arithmetic above.
const MINUTES_IN_A_DAY: i32 = 24 * 60;

/// **Your** clock — the one you make the calls by.
///
/// Not the shop's. A ring-back you set for two o'clock means two o'clock where you
/// are sitting, whichever country the shop is in, and the dates stamped on a business
/// when it is first seen are your dates too. Opening hours are the one thing read in
/// the shop's own time, and those come from its city.
///
/// Budapest until you choose your own on the page; the server sets it as it starts
/// from what you chose last. The data is bundled into the binary rather than read
/// from the machine, so a changed container image cannot move every date stored.
static YOUR_ZONE: std::sync::RwLock<&'static time_tz::Tz> =
    std::sync::RwLock::new(time_tz::timezones::db::europe::BUDAPEST);

/// The clock a fresh install starts on.
pub const FIRST_ZONE: &str = "Europe/Budapest";

fn your_zone() -> &'static time_tz::Tz {
    // A poisoned lock still holds a zone the clock knows; there is nothing to lose.
    *YOUR_ZONE.read().unwrap_or_else(|held| held.into_inner())
}

/// The name of your clock, e.g. `Europe/Budapest`.
pub fn your_zone_name() -> &'static str {
    time_tz::TimeZone::name(your_zone())
}

/// A zone the clock knows, by its IANA name. Pure.
pub fn zone_named(name: &str) -> Option<&'static time_tz::Tz> {
    time_tz::timezones::get_by_name(name.trim())
}

/// Every zone the clock knows, by name, A to Z. For the page's chooser.
pub fn all_zones() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = time_tz::timezones::iter()
        .map(time_tz::TimeZone::name)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Make your clock this zone, from now on. Refuses a name the clock does not know.
pub fn set_your_zone(name: &str) -> Result<&'static str, String> {
    let zone = zone_named(name).ok_or_else(|| {
        format!(
            "`{}` is not a clock anybody keeps — it reads like Europe/Budapest",
            name.trim()
        )
    })?;
    *YOUR_ZONE.write().unwrap_or_else(|held| held.into_inner()) = zone;
    Ok(time_tz::TimeZone::name(zone))
}

/// Which day of the week is this, where you are.
///
/// Here rather than in the caller because every use wants the same answer, and
/// getting the timezone wrong would show Sunday's hours on Monday morning.
pub fn today_here() -> Weekday {
    time_tz::OffsetDateTimeExt::to_timezone(&time::OffsetDateTime::now_utc(), your_zone()).weekday()
}

/// Today's date where you are, as `2026-09-19`.
///
/// Here beside the weekday, so everything that asks what day it is asks one place.
pub fn today_here_date() -> String {
    date_here(time::OffsetDateTime::now_utc())
}

/// Your date for a given moment.
///
/// Split out so it can be tested: a function that reads the clock can never be given
/// midnight to look at, but this one can.
pub fn date_here(instant: time::OffsetDateTime) -> String {
    time_tz::OffsetDateTimeExt::to_timezone(&instant, your_zone())
        .date()
        .to_string()
}

/// Your date and time now, to the minute: `2026-09-20 14:30`.
///
/// **Yours, not UTC** — unlike [`moment_now`], which is only ever subtracted from
/// another one of itself. This one is compared against times *you* wrote down.
pub fn now_here() -> String {
    clock_here(time::OffsetDateTime::now_utc())
}

/// Your date and time for a given moment. Split out so it can be tested.
pub fn clock_here(instant: time::OffsetDateTime) -> String {
    let there = time_tz::OffsetDateTimeExt::to_timezone(&instant, your_zone());
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        there.year(),
        u8::from(there.month()),
        there.day(),
        there.hour(),
        there.minute()
    )
}

/// How far another place's clock is from yours, in minutes.
///
/// Positive when they are ahead: Dubai is +120 against Budapest in summer and +180
/// in winter, because Hungary keeps summer time and the Gulf does not. **That is why
/// this is worked out now rather than stored** — a number written down in September
/// is an hour wrong by November.
///
/// Nought for your own zone, and nought for a zone we cannot resolve: shifting by a
/// guess would be worse than not shifting at all.
pub fn minutes_ahead_of_you(zone: &str) -> i32 {
    ahead_at(zone, time::OffsetDateTime::now_utc())
}

/// The same, for a given moment. Split out so it can be tested against a date whose
/// answer we already know.
pub fn ahead_at(zone: &str, instant: time::OffsetDateTime) -> i32 {
    let Some(theirs) = time_tz::timezones::get_by_name(zone.trim()) else {
        return 0;
    };
    let there = time_tz::OffsetDateTimeExt::to_timezone(&instant, theirs)
        .offset()
        .whole_minutes();
    let here = time_tz::OffsetDateTimeExt::to_timezone(&instant, your_zone())
        .offset()
        .whole_minutes();
    i32::from(there) - i32::from(here)
}

/// A moment, for measuring how long something took.
///
/// **UTC, not Budapest.** This is only ever subtracted from another one of these, and
/// a clock that jumps an hour twice a year would make one search of the night look an
/// hour long and another look an hour short.
///
/// Written the way SQLite reads a time, so the database can do the subtraction itself.
pub fn moment_now() -> String {
    moment(time::OffsetDateTime::now_utc())
}

/// One moment, written out. Split from [`moment_now`] so it can be tested: a function
/// that reads the clock can never be handed a particular second to look at.
pub fn moment(instant: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        instant.year(),
        u8::from(instant.month()),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second()
    )
}

/// Read a day name, in whichever language Google answered in.
///
/// **Both languages on purpose.** Google replies in the language of the request, so a
/// Hungarian search gives Hungarian day names — and the same database holds rows from
/// both. Matching only one would silently blank half of them.
///
/// Accents are compared as written: `Hétfő` is what Google sends, and a Hungarian
/// keyboard is not involved here.
fn read_day(name: &str) -> Option<Weekday> {
    let name = name.trim().to_lowercase();
    Some(match name.as_str() {
        "monday" | "hétfő" | "hetfo" => Weekday::Monday,
        "tuesday" | "kedd" => Weekday::Tuesday,
        "wednesday" | "szerda" => Weekday::Wednesday,
        "thursday" | "csütörtök" | "csutortok" => Weekday::Thursday,
        "friday" | "péntek" | "pentek" => Weekday::Friday,
        "saturday" | "szombat" => Weekday::Saturday,
        "sunday" | "vasárnap" | "vasarnap" => Weekday::Sunday,
        _ => return None,
    })
}

/// Make one line of times readable.
///
/// Google separates "8" and "am" with a narrow no-break space — a character you
/// cannot see and cannot type, which survives into every export and every clipboard
/// paste looking like a mistake. This turns those into ordinary spaces.
fn tidy_line(line: &str) -> String {
    line.chars()
        .map(|c| match c {
            // Narrow no-break space, no-break space, thin space.
            '\u{202f}' | '\u{00a0}' | '\u{2009}' => ' ',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "hours_tests.rs"]
mod tests;
