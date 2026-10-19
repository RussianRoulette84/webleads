//! Turning a business name into something that catches the whole chain.
//!
//! # What this file is here to teach
//!
//! Keeping a decision that affects what you *see* out of the database and in a place
//! where it can be tested. "Hide this chain" has to guess which part of a name is the
//! chain and which part is the branch, and a guess belongs somewhere you can check it
//! rather than buried in SQL.

/// How many words of a name make the chain.
///
/// "Best Barbers Zugló" and "Best Barbers Óbuda" are the same chain; two words is
/// what tells them apart from each other without also catching every barber in
/// Budapest.
const CHAIN_WORDS: usize = 2;

/// The pattern that catches every branch of one chain.
///
/// Lower case, because the database compares it against a lowered name — Hungarian
/// names arrive in whatever case Google felt like. The person's own `%` and `_` are
/// escaped so a name containing one searches for it rather than matching everything.
pub fn chain_pattern(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().take(CHAIN_WORDS).collect();
    let chain = words.join(" ").to_lowercase();
    let tidy = chain.trim_matches(|c: char| !c.is_alphanumeric());
    let escaped = tidy.replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

/// What the panel shows for a hidden chain — the words themselves, not the pattern.
///
/// A list reading `%best barbers%` tells you the machinery; `Best Barbers` tells you
/// what you hid.
pub fn chain_label(name: &str) -> String {
    name.split_whitespace()
        .take(CHAIN_WORDS)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "hidden_tests.rs"]
mod tests;
