//! Where a business has got to in the selling, and what you wrote down about it.
//!
//! # What this file is here to teach
//!
//! The state of a lead is the product. Everything else — the scraping, the map, the
//! pictures — only exists to put a row in front of you; this is the part that
//! remembers what you did about it. So the rules live here, in the crate that
//! depends on nothing, and the database, the server and the page all read them from
//! here rather than each keeping their own copy of the words.

use serde::{Deserialize, Serialize};

/// How many unanswered calls before we give up on a number.
///
/// Four is a judgement, not a law: it is about a fortnight of trying at different
/// times of day. Past it the number is treated as dead — see [`too_many_no_answers`].
pub const GIVE_UP_AFTER: i64 = 4;

/// What has happened with this business.
///
/// An enum for the same reason [`crate::Verdict`] is one: a `String` could hold
/// `"called"`, `"Called"` or `"banana"`, this can only ever hold one of seven things,
/// and the compiler stops us forgetting one in a `match`.
///
/// Seven because "called" tells you nothing. Nobody answered, ring them back on
/// Tuesday, they said they would ring you, and they told you to get lost are four
/// different mornings' work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Never telephoned. Everything starts here.
    #[default]
    NeedCall,
    /// Rung, nobody answered. How many times is kept on the contact.
    NotReached,
    /// Spoke to them, and they said to ring back. When is kept on the contact.
    FollowUp,
    /// Spoke to them, and they said *they* would ring you. When they said is kept
    /// on the contact, the same field a ring-back uses — the difference is whose
    /// move it is, not what is written down.
    TheyCall,
    /// They said yes. The only outcome that pays for any of this.
    Sold,
    /// They said no. Off the list unless you ask for them by name.
    NotInterested,
    /// Rung past giving up. Reached by counting, not by judgement.
    Dead,
}

impl Status {
    /// The word we store. Changing one of these would orphan every row already
    /// written, which is why it is kept apart from [`Status::describe`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedCall => "need_call",
            Self::NotReached => "not_reached",
            Self::FollowUp => "follow_up",
            Self::TheyCall => "they_call",
            Self::Sold => "sold",
            Self::NotInterested => "not_interested",
            Self::Dead => "dead",
        }
    }

    /// The word a person reads. Safe to reword whenever it reads better.
    pub fn describe(self) -> &'static str {
        match self {
            Self::NeedCall => "to call",
            Self::NotReached => "no answer",
            Self::FollowUp => "ring back",
            Self::TheyCall => "they'll call",
            Self::Sold => "sold",
            Self::NotInterested => "said no",
            Self::Dead => "line dead",
        }
    }

    /// Should this business still appear on the call list?
    ///
    /// Four states are work you still have: never rung, rung with no answer,
    /// promised a call back, and waiting on a call they promised. The other three
    /// are finished — sold, refused, or a number that never answers — and they only
    /// come back when you ask for them by name, which is the way back from a
    /// misclick.
    pub fn still_on_the_list(self) -> bool {
        matches!(
            self,
            Self::NeedCall | Self::NotReached | Self::FollowUp | Self::TheyCall
        )
    }

    /// The states that make up the call list, as the words we store.
    ///
    /// **The database reads its rule from here**, so the rule exists once: nothing
    /// writes these words into its own SQL, where a copy could quietly fall behind.
    pub fn words_on_the_list() -> Vec<&'static str> {
        Self::ALL
            .into_iter()
            .filter(|s| s.still_on_the_list())
            .map(Self::as_str)
            .collect()
    }

    /// How much work this state still represents, lowest first.
    ///
    /// What "sort by state" means. Sorting the stored words alphabetically would
    /// read dead, ring back, to call, said no, no answer, sold — an order with no
    /// opinion in it at all. This one puts the morning's work at the top: what you
    /// have not touched, then what you promised to ring back, then the call you are
    /// waiting on, then what has not answered, then everything already finished
    /// with.
    pub fn work_left(self) -> u8 {
        match self {
            Self::NeedCall => 0,
            Self::FollowUp => 1,
            Self::TheyCall => 2,
            Self::NotReached => 3,
            Self::Sold => 4,
            Self::NotInterested => 5,
            Self::Dead => 6,
        }
    }

    /// How far along this state is, for deciding what survives a merge.
    ///
    /// Sold beats everything — it happened. A refusal beats a promise to ring back,
    /// because it is the later and more final thing to have learned. And `Dead`
    /// loses to anything you have actually spoken to: a number that stopped
    /// answering does not undo a conversation.
    pub fn progress(self) -> u8 {
        match self {
            Self::NeedCall => 0,
            Self::Dead => 1,
            Self::NotReached => 2,
            Self::TheyCall => 3,
            Self::FollowUp => 4,
            Self::NotInterested => 5,
            Self::Sold => 6,
        }
    }

    /// Every status, in the order they make sense on screen.
    pub const ALL: [Status; 7] = [
        Status::NeedCall,
        Status::NotReached,
        Status::FollowUp,
        Status::TheyCall,
        Status::Sold,
        Status::NotInterested,
        Status::Dead,
    ];

    /// Does this state carry a date and time?
    ///
    /// Two of them do, and the difference between them is only whose move it is.
    /// Everything that shows or saves a time asks this rather than naming the two
    /// states again, so a third one would only have to be added here.
    pub fn carries_a_time(self) -> bool {
        matches!(self, Self::FollowUp | Self::TheyCall)
    }
}

impl std::str::FromStr for Status {
    type Err = UnknownStatus;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Status::ALL
            .into_iter()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| UnknownStatus(text.to_owned()))
    }
}

/// What comes back when the database holds a word that is not a status.
#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a status")]
pub struct UnknownStatus(pub String);

/// Have we rung this number too many times to keep trying?
pub fn too_many_no_answers(attempts: i64) -> bool {
    attempts > GIVE_UP_AFTER
}

/// What the state becomes when the count of unanswered calls changes.
///
/// The one rule, in one place, whether the count went up because you rang again or
/// because you corrected it by hand. It only ever moves between "no answer" and
/// "line dead" — a business you have sold to does not become dead because you fixed
/// a number beside it.
pub fn with_no_answers(status: Status, attempts: i64) -> Status {
    match status {
        Status::NotReached if too_many_no_answers(attempts) => Status::Dead,
        Status::Dead if !too_many_no_answers(attempts) => Status::NotReached,
        other => other,
    }
}

/// One more unanswered call: the new state, and the new count.
///
/// Pure, so "the fifth no-answer is the one that gives up" is a test rather than
/// something you find out by ringing a shop five times.
pub fn after_no_answer(attempts: i64) -> (Status, i64) {
    let attempts = attempts.max(0) + 1;
    (with_no_answers(Status::NotReached, attempts), attempts)
}

/// A ring-back time as we keep it: `2026-09-21 14:30`.
///
/// The browser's own date-and-time field hands over `2026-09-21T14:30`, sometimes
/// with seconds on the end. Stored with a space and to the minute, because that is
/// what SQLite compares as a time and what sorts correctly as plain text.
///
/// Anything that is not a time comes back as `None` rather than being stored and
/// confusing us later.
pub fn tidy_ring_back(typed: &str) -> Option<String> {
    let typed = typed.trim().replace('T', " ");
    let (date, clock) = typed.split_once(' ')?;
    let mut parts = clock.split(':');
    let hour: u8 = parts.next()?.parse().ok()?;
    let minute: u8 = parts.next()?.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let mut days = date.split('-');
    let year: i32 = days.next()?.parse().ok()?;
    let month: u8 = days.next()?.parse().ok()?;
    let day: u8 = days.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !(2000..=2999).contains(&year) {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"
    ))
}

/// A stored ring-back time in the shape the browser's field wants it.
pub fn ring_back_for_input(stored: &str) -> String {
    stored.trim().replace(' ', "T")
}

/// Was this ring-back meant to happen already?
///
/// Plain text comparison, which works because both are written the same way round —
/// year, month, day — and is why they are stored like that.
pub fn ring_back_overdue(stored: &str, now: &str) -> bool {
    stored.trim() <= now.trim()
}

/// Which of the promised calls to show.
///
/// A word rather than a date on the request: the page has no clock it can trust — a
/// WebAssembly build carries no timezone database — so it says *which* calls it wants
/// and the server works out where the line falls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Due {
    /// A call you meant to make already.
    Overdue,
    /// Anything up to the end of today.
    Today,
    /// Anything in the next seven days.
    Week,
}

impl Due {
    /// The word this goes by in a web address.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overdue => "overdue",
            Self::Today => "today",
            Self::Week => "week",
        }
    }

    /// What a person reads.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Overdue => "overdue",
            Self::Today => "by tonight",
            Self::Week => "this week",
        }
    }

    /// Every one, in the order they widen.
    pub const ALL: [Due; 3] = [Due::Overdue, Due::Today, Due::Week];

    /// The latest ring-back time this still counts as due, by the clock now.
    ///
    /// The clock stays in this crate, beside the timezone data, rather than every
    /// caller reaching for one of its own.
    pub fn cut_off_now(self) -> String {
        self.cut_off(time::OffsetDateTime::now_utc())
    }

    /// The latest ring-back time this still counts as due.
    ///
    /// Budapest, to the minute, in the shape the times are stored — so the comparison
    /// is plain text and the database can do it.
    pub fn cut_off(self, instant: time::OffsetDateTime) -> String {
        match self {
            Self::Overdue => crate::hours::clock_here(instant),
            Self::Today => format!("{} 23:59", crate::hours::date_here(instant)),
            Self::Week => format!(
                "{} 23:59",
                crate::hours::date_here(instant + time::Duration::days(7))
            ),
        }
    }
}

impl std::str::FromStr for Due {
    type Err = UnknownStatus;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Due::ALL
            .into_iter()
            .find(|d| d.as_str() == text)
            .ok_or_else(|| UnknownStatus(text.to_owned()))
    }
}

/// Everything we have recorded about dealing with one business.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Contact {
    pub status: Status,
    /// Whatever you scribbled — "call back after five", "ask for the owner".
    pub note: String,
    /// The day you telephoned, if you have. `None` until then.
    pub called_on: Option<String>,
    /// How many times you rang and nobody answered.
    #[serde(default)]
    pub not_reached_counter: i64,
    /// When you said you would ring back, as `2026-09-21 14:30`.
    #[serde(default)]
    pub follow_up_tstamp: Option<String>,
}

impl Contact {
    /// Is this business still worth putting in front of you?
    pub fn still_on_the_list(&self) -> bool {
        self.status.still_on_the_list()
    }
}

#[cfg(test)]
#[path = "contact_tests.rs"]
mod tests;
