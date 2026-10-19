//! What a lead, a target and a run *are* — and nothing about how they are fetched,
//! stored or displayed.
//!
//! # Why this crate exists
//!
//! Every other crate in the workspace depends on this one, and this one depends on
//! none of them. That is not a style preference, it is the whole architecture: if the
//! database is swapped, or axum is replaced, or the browser side is thrown away, the
//! meaning of "a business with a telephone and no website" does not move.
//!
//! # Reading order
//!
//! Start with [`lead`], then [`target`], then [`options`]. Each file explains the Rust
//! ideas it uses where they first appear rather than assuming them.

// `pub mod` makes a module part of this crate's public surface. Without `pub` the
// module would exist but nobody outside this crate could name it.
pub mod area;
pub mod city;
pub mod contact;
pub mod hidden;
pub mod hours;
pub mod lead;
pub mod options;
pub mod pitch;
pub mod rules;
pub mod sort;
pub mod target;

// Re-exporting means someone can write `webleads_domain::Lead` instead of
// `webleads_domain::lead::Lead`. The module structure is ours; the names people
// actually type should be short.
pub use area::Area;
pub use city::{
    check_centre, check_centre_reach, country, lang_for, languages_in_order, City, Country,
    COUNTRIES, LANGUAGES,
};
pub use contact::{
    after_no_answer, ring_back_for_input, ring_back_overdue, tidy_ring_back, too_many_no_answers,
    with_no_answers, Contact, Due, Status, UnknownStatus, GIVE_UP_AFTER,
};
pub use hidden::{chain_label, chain_pattern};
pub use hours::{
    ahead_at, all_zones, clock_here, date_here, minutes_ahead_of_you, moment, moment_now, now_here,
    plain_hours, said_at, set_your_zone, shift_line, span_minutes, today_here, today_here_date,
    your_zone_name, zone_named, Hours, FIRST_ZONE,
};
pub use lead::{normalise_phone, Lead, Tls, UnknownVerdict, Verdict};
pub use options::ScrapeOptions;
pub use rules::{
    is_social_host, looks_like_bot_check, looks_parked, BOT_CHECK_MARKERS, PARKED_MARKERS,
    SOCIAL_HOSTS,
};
pub use sort::{Sort, UnknownSort};
pub use target::{name_for, Target, Tier};
