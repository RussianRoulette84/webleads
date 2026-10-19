//! The trades we search for, and how they are ranked.

use serde::{Deserialize, Serialize};

/// How promising a trade is. `S` is best.
///
/// Deriving `PartialOrd` and `Ord` on an enum orders the variants in the order they
/// are written, so `Tier::S < Tier::A` — which is what we want, since S sorts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    S,
    A,
    B,
    C,
}

impl Tier {
    /// Read a tier from the CSV's single letter.
    ///
    /// Returning `Option<Self>` rather than defaulting to something means a typo in
    /// the data becomes a visible `None` instead of a silent wrong answer.
    pub fn parse(letter: &str) -> Option<Self> {
        match letter.trim() {
            "S" => Some(Self::S),
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            _ => None,
        }
    }
}

/// One trade from `research/TARGETS.csv`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub rank: u8,
    pub tier: Tier,
    /// The English name. **The key**, not decoration: it is what every business on
    /// the list already records as its trade, so it has to keep working as one.
    pub english: String,
    /// The words that actually get searched, by language. A trade can have more than
    /// one in a language — *borbély* and *barbershop* are both real Hungarian
    /// searches and find different shops.
    ///
    /// A language with no words means **this trade is not searched there**. There is
    /// no pálinka distillery in Dubai and no abaya tailor in Budapest, and a list
    /// that pretended otherwise would send half of every run looking for nothing.
    ///
    /// `BTreeMap` rather than `HashMap` so the languages come out in a settled order,
    /// which makes what the page offers predictable rather than shuffled each start.
    pub words: std::collections::BTreeMap<String, Vec<String>>,
    /// 1–5: how often this trade turns up without a website.
    pub no_site_odds: u8,
    /// 1–5: what they can afford to pay.
    pub budget: u8,
    pub note: String,
}

impl Target {
    /// The search phrases this trade produces for a given place.
    ///
    /// `impl Iterator<Item = String>` says "you get something you can iterate, and I
    /// am not telling you what type" — no allocation of an intermediate `Vec` unless
    /// the caller asks for one. `move` hands the closure its own copy of `place` so
    /// the returned iterator does not borrow from this function's stack.
    pub fn phrases(&self, place: &str, lang: &str) -> impl Iterator<Item = String> + '_ {
        let place = place.trim().to_owned();
        self.words(lang).iter().map(move |word| {
            if place.is_empty() {
                word.clone()
            } else {
                format!("{word} {place}")
            }
        })
    }

    /// The words this trade is searched for in one language.
    ///
    /// Empty when it is not searched there at all, which is a real answer rather than
    /// a missing one.
    pub fn words(&self, lang: &str) -> &[String] {
        self.words
            .get(lang.trim().to_lowercase().as_str())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Is this trade searched for in this language?
    pub fn searched_in(&self, lang: &str) -> bool {
        !self.words(lang).is_empty()
    }

    /// Every language this trade has words for.
    pub fn languages(&self) -> impl Iterator<Item = &str> {
        self.words
            .iter()
            .filter(|(_, words)| !words.is_empty())
            .map(|(lang, _)| lang.as_str())
    }

    /// What this trade is called in one language, for reading.
    ///
    /// The first of its search words: *borbély* is the trade, *barbershop* is another
    /// way to find it. Falls back to the English name, which is the one thing every
    /// trade always has.
    pub fn name_in(&self, lang: &str) -> &str {
        self.words(lang)
            .iter()
            .map(String::as_str)
            .find(|word| !word.trim().is_empty())
            .unwrap_or(&self.english)
    }

    /// Rough worth: high odds of no website, and money to spend.
    pub fn promise(&self) -> u8 {
        self.no_site_odds + self.budget
    }
}

/// What a trade is called in one language, given the English name we store.
///
/// The database keeps the English name — every one of the businesses already on the
/// list does — so the screen has to look the local one up. `None` for a word you
/// typed yourself, which is nobody's trade and is already in whatever language you
/// typed it in.
///
/// Pure, and tested: a lookup that quietly stops matching would show the whole list
/// in English again and nothing else would notice.
pub fn name_for(targets: &[Target], english: &str, lang: &str) -> Option<String> {
    let english = english.trim();
    if english.is_empty() {
        return None;
    }
    targets
        .iter()
        .find(|t| t.english.eq_ignore_ascii_case(english))
        .map(|t| t.name_in(lang).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn barber() -> Target {
        Target {
            rank: 92,
            tier: Tier::C,
            english: "Barbers".to_owned(),
            words: [
                (
                    "hu".to_owned(),
                    vec!["borbély".to_owned(), "barbershop".to_owned()],
                ),
                ("en".to_owned(), vec!["barber shop".to_owned()]),
            ]
            .into_iter()
            .collect(),
            no_site_odds: 5,
            budget: 1,
            note: "Famous for no site, no money".to_owned(),
        }
    }

    #[test]
    fn s_sorts_before_c() {
        let mut tiers = [Tier::C, Tier::S, Tier::B, Tier::A];
        tiers.sort();
        assert_eq!(tiers, [Tier::S, Tier::A, Tier::B, Tier::C]);
    }

    #[test]
    fn a_bad_letter_is_none_not_a_guess() {
        assert_eq!(Tier::parse("S"), Some(Tier::S));
        assert_eq!(Tier::parse("Q"), None);
    }

    #[test]
    fn a_trade_with_two_words_makes_two_searches() {
        let phrases: Vec<String> = barber().phrases("Budapest Zugló", "hu").collect();
        assert_eq!(
            phrases,
            vec!["borbély Budapest Zugló", "barbershop Budapest Zugló"]
        );
    }

    #[test]
    fn without_a_place_the_word_stands_alone() {
        let phrases: Vec<String> = barber().phrases("  ", "hu").collect();
        assert_eq!(phrases, vec!["borbély", "barbershop"]);
    }

    #[test]
    fn promise_adds_the_two_scores() {
        let mut t = barber();
        t.no_site_odds = 4;
        t.budget = 3;
        assert_eq!(t.promise(), 7);
    }

    #[test]
    fn a_trade_nobody_wants_scores_lowest_and_the_best_scores_highest() {
        let mut worst = barber();
        worst.no_site_odds = 1;
        worst.budget = 1;
        let mut best = barber();
        best.no_site_odds = 5;
        best.budget = 5;
        assert_eq!(worst.promise(), 2);
        assert_eq!(best.promise(), 10);
        assert!(best.promise() > worst.promise());
    }

    #[test]
    fn the_local_name_is_the_first_of_the_search_words() {
        // *borbély* is the trade; *barbershop* is another way to find it.
        assert_eq!(barber().name_in("hu"), "borbély");
    }

    #[test]
    fn a_blank_first_word_is_skipped_rather_than_shown() {
        let mut odd = barber();
        odd.words.insert(
            "hu".to_owned(),
            vec!["   ".to_owned(), "borbély".to_owned()],
        );
        assert_eq!(odd.name_in("hu"), "borbély");
    }

    #[test]
    fn a_trade_with_no_words_at_all_reads_in_english() {
        let mut broken = barber();
        broken.words.clear();
        assert_eq!(broken.name_in("hu"), "Barbers");
    }

    #[test]
    fn an_english_trade_name_finds_its_local_one() {
        let list = [barber()];
        assert_eq!(name_for(&list, "Barbers", "hu").as_deref(), Some("borbély"));
        // The stored value's capitals and spacing are not ours to rely on.
        assert_eq!(
            name_for(&list, "  barbers  ", "hu").as_deref(),
            Some("borbély")
        );
    }

    #[test]
    fn a_word_you_typed_yourself_has_no_local_name() {
        let list = [barber()];
        assert_eq!(name_for(&list, "zeneiskola", "hu"), None);
        assert_eq!(name_for(&list, "", "hu"), None);
        assert_eq!(name_for(&[], "Barbers", "hu"), None);
    }

    #[test]
    fn a_trade_is_searched_in_the_languages_it_has_words_for() {
        let barber = barber();
        assert!(barber.searched_in("hu"));
        assert!(barber.searched_in("en"));
        // Nothing pretends there is a Hungarian barber search in Arabic.
        assert!(!barber.searched_in("ar"));
        let langs: Vec<&str> = barber.languages().collect();
        assert_eq!(langs, vec!["en", "hu"], "settled order, not shuffled");
    }

    #[test]
    fn a_trade_with_no_words_in_a_language_is_not_searched_there() {
        // There is no pálinkáa distillery in Dubai and no abaya tailor in Budapest. A
        // list that pretended otherwise would send half of every run after nothing.
        let mut gulf_only = barber();
        gulf_only.words.remove("hu");
        assert!(!gulf_only.searched_in("hu"));
        assert!(gulf_only.phrases("Budapest", "hu").next().is_none());
        assert!(gulf_only.phrases("Dubai", "en").next().is_some());
    }

    #[test]
    fn the_language_asked_for_is_read_whatever_the_capitals() {
        assert_eq!(barber().name_in("HU"), "borbély");
        assert_eq!(barber().name_in("  hu  "), "borbély");
    }

    #[test]
    fn each_language_searches_its_own_words() {
        let hungarian: Vec<String> = barber().phrases("Budapest", "hu").collect();
        assert_eq!(hungarian, vec!["borbély Budapest", "barbershop Budapest"]);
        let english: Vec<String> = barber().phrases("Dubai", "en").collect();
        assert_eq!(english, vec!["barber shop Dubai"]);
    }

    #[test]
    fn the_same_trade_reads_differently_in_each_language() {
        let list = [barber()];
        assert_eq!(name_for(&list, "Barbers", "hu").as_deref(), Some("borbély"));
        assert_eq!(
            name_for(&list, "Barbers", "en").as_deref(),
            Some("barber shop")
        );
        // A language it has no words for falls back to the key, which is the one
        // name every trade always has.
        assert_eq!(name_for(&list, "Barbers", "ar").as_deref(), Some("Barbers"));
    }

    #[test]
    fn a_tier_letter_is_read_whatever_the_spacing() {
        assert_eq!(Tier::parse(" S "), Some(Tier::S));
        assert_eq!(Tier::parse("A"), Some(Tier::A));
        assert_eq!(Tier::parse("B"), Some(Tier::B));
        assert_eq!(Tier::parse("C"), Some(Tier::C));
    }

    #[test]
    fn a_lowercase_letter_is_not_a_tier() {
        // The file is written by us with capitals. Accepting lowercase would hide a
        // typo rather than surface it.
        assert_eq!(Tier::parse("s"), None);
        assert_eq!(Tier::parse(""), None);
        assert_eq!(Tier::parse("SS"), None);
        assert_eq!(Tier::parse("1"), None);
    }
}
