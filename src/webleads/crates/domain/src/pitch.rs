//! One line saying who you are about to ring, and why they are worth ringing.
//!
//! Not a script. You do the talking; this is the reminder in front of you while the
//! telephone is ringing, arranged so the useful part comes first.

use crate::{Lead, Tls, Verdict};

/// What separates the parts. A middle dot reads as a pause rather than as punctuation.
const BETWEEN: &str = " · ";

impl Lead {
    /// The facts about this business, arranged.
    ///
    /// Three parts, in this order, and any of them may be missing:
    ///
    /// 1. **The trade** — what they do, so you open with the right words.
    /// 2. **How busy they look** — reviews and rating, because that is what says
    ///    they can afford to pay you.
    /// 3. **Why they qualified**, in its sharpest form. This is the reason for the
    ///    call and it comes last because it is what you say last.
    ///
    /// Never empty. Every business on the list qualified for *some* reason, so
    /// there is always at least the third part to say.
    pub fn pitch(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if let Some(trade) = self.shown_trade() {
            parts.push(trade.to_owned());
        }
        if let Some(busy) = self.how_busy() {
            parts.push(busy);
        }
        parts.push(self.why_worth_ringing());

        parts.join(BETWEEN)
    }

    /// How busy they look, or `None` when nobody has ever reviewed them.
    ///
    /// A business with no reviews at all is not "0 reviews" — saying that out loud
    /// would be rude and it is not the point. It is simply left out, and the fact
    /// that they are quiet is already why they sort to the top of your list.
    fn how_busy(&self) -> Option<String> {
        if self.reviews == 0 {
            return None;
        }
        let reviews = format!(
            "{} review{}",
            self.reviews,
            if self.reviews == 1 { "" } else { "s" }
        );
        match self.rating {
            Some(stars) if stars > 0.0 => Some(format!("{reviews}, {stars:.1}★")),
            _ => Some(reviews),
        }
    }

    /// Why this business is on the call list, in the sharpest form.
    ///
    /// Sharper than [`Verdict::describe`] on purpose: that one labels a column, this
    /// one is the sentence you would actually say.
    fn why_worth_ringing(&self) -> String {
        // A broken certificate is the most specific thing we can offer, so it wins
        // wherever we found one — even on a business that also has another problem.
        if let Tls::Broken(detail) = &self.tls {
            return match detail.to_lowercase() {
                d if d.contains("expired") => {
                    "certificate expired — visitors see a security warning".to_owned()
                }
                d if d.contains("self-signed") => {
                    "certificate is self-signed — visitors see a security warning".to_owned()
                }
                _ => "certificate is broken — visitors see a security warning".to_owned(),
            };
        }

        match self.verdict {
            Verdict::NoSite => "no website at all".to_owned(),
            Verdict::SocialOnly => match self.social_host() {
                // "a Instagram page" is the sort of thing that makes a tool feel
                // unfinished, and it is one line to get right.
                Some(host) => format!("only {} {host} page", article_for(host)),
                None => "only a social page".to_owned(),
            },
            Verdict::DeadSite => match self.reason.to_lowercase() {
                r if r.contains("parked") => "website is a parked page".to_owned(),
                r if r.contains("timed out") => "website does not load".to_owned(),
                r if r.contains("empty") => "website is empty".to_owned(),
                _ => "website is down".to_owned(),
            },
            Verdict::InsecureSite => {
                "certificate is broken — visitors see a security warning".to_owned()
            }
            // Not a lead, so never shown — but a sentence beats an empty string if it
            // ever is.
            Verdict::LiveSite => "has a working website".to_owned(),
        }
    }

    /// Which social network their page is on, in a word.
    ///
    /// "only a Facebook page" is worth saying; "only a page" is not.
    fn social_host(&self) -> Option<&'static str> {
        let url = self.social_url.as_deref()?.to_lowercase();
        Some(match url {
            u if u.contains("facebook") || u.contains("fb.") => "Facebook",
            u if u.contains("instagram") => "Instagram",
            u if u.contains("tiktok") => "TikTok",
            u if u.contains("linkedin") => "LinkedIn",
            u if u.contains("linktr") => "Linktree",
            _ => return None,
        })
    }
}

/// "a" or "an", by how the next word sounds.
///
/// Only ever asked about the handful of names below, so the vowel rule is enough —
/// no need for the exceptions that make this hard in general.
fn article_for(word: &str) -> &'static str {
    match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

#[cfg(test)]
#[path = "pitch_tests.rs"]
mod tests;
