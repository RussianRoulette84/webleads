//! A business worth telephoning, and the reason it qualified.

use serde::{Deserialize, Serialize};

/// Why a business is on the list — or why it is not.
///
/// This is an `enum`, and it is doing more work than a string would. A `String`
/// could hold `"no_site"`, `"nosite"` or `"banana"`; this can only ever hold one of
/// four things, and the compiler will stop us if we forget to handle one of them in
/// a `match`. That is the single most useful habit Rust teaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Google shows no website link at all.
    NoSite,
    /// The only link is Facebook, Instagram, LinkedIn and the like.
    SocialOnly,
    /// There is a link, but it errors, times out, or is a parked page.
    DeadSite,
    /// The website loads, but its certificate is broken — so every visitor gets a
    /// red security warning before they see the shop. A lead, and a good one: it is
    /// a real problem with a price on it.
    InsecureSite,
    /// The website loads and is real. Not a lead.
    LiveSite,
}

/// What we found out about a site's security certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tls {
    /// We never got far enough to look — no link, a social page, or nothing answered.
    Unknown,
    /// Fine. Nothing to sell here.
    Valid,
    /// Broken, and this is what is wrong with it — the sentence you say on the
    /// telephone. Expired, self-signed, issued for a different address.
    Broken(String),
}

impl Tls {
    /// Is there something here worth telephoning about?
    pub fn is_a_problem(&self) -> bool {
        matches!(self, Self::Broken(_))
    }

    /// What to show in the certificate column. Empty when there is nothing to say.
    pub fn describe(&self) -> &str {
        match self {
            Self::Unknown | Self::Valid => "",
            Self::Broken(detail) => detail,
        }
    }
}

impl Verdict {
    /// Does this verdict put the business on the call list?
    ///
    /// `matches!` is a small macro for "does this value look like one of these
    /// shapes" — shorter than a `match` when all we want back is a bool.
    pub fn is_lead(self) -> bool {
        matches!(
            self,
            Self::NoSite | Self::SocialOnly | Self::DeadSite | Self::InsecureSite
        )
    }

    /// The word a person reads on the screen.
    ///
    /// Separate from [`Verdict::as_str`] on purpose: that one is the word we store,
    /// and changing it would break every row already in the database. This one is
    /// for human eyes and can be reworded freely.
    pub fn describe(self) -> &'static str {
        match self {
            Self::NoSite => "no website",
            Self::SocialOnly => "social only",
            Self::DeadSite => "broken site",
            Self::InsecureSite => "certificate broken",
            Self::LiveSite => "has a site",
        }
    }

    /// Which of our colours this verdict wears.
    ///
    /// Three names, not five: the two certificate-shaped problems are the same
    /// conversation on the telephone, and a map with five colours on it is a map
    /// nobody reads. Named rather than a hex value, because the live page, the map
    /// and the standalone page each turn the name into their own colour — and all
    /// three have to agree about which reason is which.
    pub fn tone(self) -> &'static str {
        match self {
            // The best kind of lead, and the most common.
            Self::NoSite => "accent",
            // Has something, but not a website of their own.
            Self::SocialOnly | Self::InsecureSite => "warn",
            Self::DeadSite => "bad",
            // Never on the list — a working website is not a lead.
            Self::LiveSite => "dim",
        }
    }

    /// The word we store in the database and put in JSON.
    ///
    /// Returns `&'static str` — a string that lives in the program itself and needs
    /// no allocation, because these four words are baked into the binary.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoSite => "no_site",
            Self::SocialOnly => "social_only",
            Self::DeadSite => "dead_site",
            Self::InsecureSite => "insecure_site",
            Self::LiveSite => "live_site",
        }
    }
}

/// Reading a verdict back from the database.
///
/// Implementing the standard [`std::str::FromStr`] trait rather than a method called
/// `from_str` means `"no_site".parse::<Verdict>()` works, like it does for numbers.
impl std::str::FromStr for Verdict {
    type Err = UnknownVerdict;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "no_site" => Ok(Self::NoSite),
            "social_only" => Ok(Self::SocialOnly),
            "dead_site" => Ok(Self::DeadSite),
            "insecure_site" => Ok(Self::InsecureSite),
            "live_site" => Ok(Self::LiveSite),
            other => Err(UnknownVerdict(other.to_owned())),
        }
    }
}

/// What comes back when the database holds a word that is not a verdict.
#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a verdict")]
pub struct UnknownVerdict(pub String);

/// One business, as we care about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lead {
    /// How we tell two businesses apart. Google's own id when we have it; when fast
    /// mode strips it, the name plus a rounded position. See [`Lead::identity`].
    pub id: String,
    pub name: String,
    /// `Option<String>` is how Rust says "there might not be one". There is no
    /// `null` to forget about — the compiler makes us say what happens when it is
    /// absent, every time we read it.
    pub phone: Option<String>,
    /// Google's own category, when fast mode did not strip it.
    pub category: Option<String>,
    /// The trade **we** searched for, which is the one we can rely on.
    ///
    /// Google's category is whatever it felt like returning, and fast mode drops it
    /// altogether. The word we typed into the search is always known and always
    /// right, so it wins when we have it.
    pub trade: Option<String>,
    pub address: Option<String>,
    pub maps_url: Option<String>,
    /// Their own website, if they have one that works.
    pub website: Option<String>,
    /// A social page standing in for a website.
    pub social_url: Option<String>,
    pub verdict: Verdict,
    /// Why, in words, for the person making the call.
    pub reason: String,
    pub reviews: u32,
    pub rating: Option<f32>,
    /// Metres from the centre of the area, measured by us, not guessed by Google.
    pub distance_m: u32,
    /// Where they are, for the map. `None` when the scrape gave no usable position —
    /// such a business is simply not pinned rather than dropped from the list.
    #[serde(default)]
    pub position: Option<(f64, f64)>,
    /// What we found out about their security certificate.
    pub tls: Tls,
    /// When they are open, as far as Google told us. Empty for anything scraped in
    /// fast mode, which sends nothing at all.
    #[serde(default)]
    pub hours: crate::Hours,
}

impl Lead {
    /// The canonical form of this business's telephone number.
    ///
    /// See [`normalise_phone`] for what it will and will not convert.
    ///
    /// The country has to be passed in: a lead does not know which one it is in, and
    /// the same digits mean different towns in different countries — `06` is a
    /// Hungarian trunk prefix and a Sharjah area code.
    pub fn phone_canonical(&self, country_code: &str) -> Option<String> {
        normalise_phone(self.phone.as_deref()?, country_code)
    }

    /// What trade to show for this business.
    ///
    /// Ours first, Google's second, nothing third. A blank is better than a guess —
    /// a wrong trade would send you into a call with the wrong opening line.
    pub fn shown_trade(&self) -> Option<&str> {
        self.trade
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| self.category.as_deref().filter(|c| !c.trim().is_empty()))
    }

    /// Build the identity used to spot the same business found by several searches.
    ///
    /// Two `&str` parameters rather than `String`: a borrowed string slice is a view
    /// into text someone else owns, so this does not take a copy and the caller keeps
    /// theirs. Taking `String` here would force every caller to hand over ownership
    /// of data they still need.
    ///
    /// Coordinates are rounded to four decimal places — about eleven metres — so the
    /// same shop reported with slightly different coordinates still collapses to one,
    /// while two shops in one building stay two.
    /// The telephone number reduced to what a `tel:` link accepts.
    ///
    /// People read `+36 1 234 5001`; a telephone dials `+3612345001`. Everything that
    /// is not a digit or a leading plus goes. Returns `None` when there is no number,
    /// or when what is there holds no digits at all — an empty `tel:` link is worse
    /// than no link, because it looks like it should work.
    pub fn dial_number(&self) -> Option<String> {
        let raw = self.phone.as_ref()?;
        let plus = raw.trim_start().starts_with('+');
        let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        Some(if plus { format!("+{digits}") } else { digits })
    }

    pub fn identity(google_id: Option<&str>, name: &str, lat: f64, lon: f64) -> String {
        match google_id {
            Some(id) if !id.trim().is_empty() => id.trim().to_owned(),
            _ => format!("{}|{:.4},{:.4}", name.trim().to_lowercase(), lat, lon),
        }
    }

    /// Ordering for the call list: never-reviewed first, then quietest, then nearest.
    ///
    /// Returning a tuple and letting the caller sort by it is the usual Rust way to
    /// express "sort by several things in order". Tuples compare left to right.
    pub fn sort_key(&self) -> (bool, u32, u32) {
        (self.reviews > 0, self.reviews, self.distance_m)
    }
}

/// Put a telephone number into one canonical shape, for the country it is in.
///
/// `+36 1 234 5001`, `06 1 234 5001` and `0036-1-234-5001` are the same shop written
/// three ways. Left as-is they look like three different businesses, which is why this
/// is the key two records are matched on.
///
/// **The country decides the rules.** In the UAE `06` is Sharjah, not a Hungarian
/// trunk prefix, and a Dubai number written `+971 4 …` must keep its plus or the
/// same shop written two ways stops matching itself.
///
/// A number already written with `+` keeps the country code it came with, whoever it
/// belongs to. One written for people at home has its trunk prefix taken off and the
/// country's own code put on. **Anything we do not recognise keeps its digits exactly
/// as they were** — guessing a country code onto an unfamiliar number would invent
/// data, and wrong data is worse than untidy data.
///
/// Returns `None` when there is no number in there at all.
pub fn normalise_phone(raw: &str, country_code: &str) -> Option<String> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }

    // A country code with nothing after it is not a telephone number, whoever it
    // belongs to. Five digits because the shortest real one is longer than that and
    // the longest country code is shorter — it separates `+36` from a number without
    // needing to know every country's code.
    const SHORTEST_NUMBER: usize = 5;

    // Written with a plus: it carries its own country code and we keep it whole.
    // Reading only our own out of it is how every foreign number lost its plus.
    if raw.trim_start().starts_with('+') {
        if digits.len() < SHORTEST_NUMBER {
            return None;
        }
        return Some(format!("+{digits}"));
    }

    // Written for people at home. Only the country we are actually in can say what
    // the leading noughts mean, so a country we have never heard of leaves the digits
    // untouched rather than being guessed at.
    let Some(country) = crate::city::country(country_code) else {
        return Some(digits);
    };

    // Longest first, which the country table is checked for: `0036` has to be tried
    // before `06`, or the `00` is read as the trunk prefix and the `36` swallowed
    // into the subscriber number.
    let national = country
        .trunks
        .iter()
        .find_map(|trunk| digits.strip_prefix(trunk));

    match national {
        // A prefix with nothing after it is not a telephone number.
        Some("") => None,
        Some(rest) => Some(format!("+{}{rest}", country.dials)),
        None => Some(digits),
    }
}

#[cfg(test)]
#[path = "lead_tests.rs"]
mod tests;
