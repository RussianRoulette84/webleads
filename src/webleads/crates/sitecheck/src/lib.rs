//! Decides whether a business website is real, social-only, broken, or absent.
//!
//! This is the rule the whole product rests on. A business with a working website is
//! not a lead; a business with a Facebook page, a dead domain, a certificate that
//! frightens visitors, or nothing at all is.
//!
//! # How it is laid out
//!
//! - [`rules`] — everything decidable without a socket. Pure, and tested case by case.
//! - [`fetch`] — the clients, the retries, and reading the top of a page.
//!
//! # What this crate is here to teach
//!
//! `async`/`await` with real network calls, bounded concurrency with a
//! [`tokio::sync::Semaphore`], and why splitting a decision away from the I/O that
//! feeds it is what makes the decision testable at all.

#![forbid(unsafe_code)]

pub mod fetch;
pub mod gate;
pub mod rules;

use std::time::Duration;

use webleads_domain::{Tls, Verdict};

pub use fetch::{check_many, check_one, clients, Clients};
pub use gate::PER_HOST_GAP;
pub use rules::{judge, pre_check, PreCheck};

/// How long to wait for one website on the first try.
pub const TIMEOUT: Duration = Duration::from_secs(8);

/// How long to wait on the retry. A shop on cheap shared hosting can genuinely take
/// this long when somebody else on the same machine is busy — and calling that
/// business to say their website is broken would be embarrassing.
pub const RETRY_TIMEOUT: Duration = Duration::from_secs(20);

/// How many websites to check at the same time.
///
/// Eight. With the per-host queue in [`fetch`] and the server skipping anything
/// checked recently there are few requests to make, and a low number makes us a
/// quiet visitor.
pub const CONCURRENCY: usize = 8;

/// Some servers answer differently to something that does not look like a browser.
pub const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// A page shorter than this is a placeholder, whatever it claims.
const MIN_REAL_PAGE_BYTES: usize = 200;

/// Never read more than this from one page — we only need the top of it, and an
/// endless response should not be able to exhaust our memory.
const MAX_READ_BYTES: usize = 60_000;

/// What we decided about one website, and why in words an operator can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub verdict: Verdict,
    pub reason: String,
    /// What we learned about the certificate on the way.
    pub tls: Tls,
}

impl Outcome {
    fn new(verdict: Verdict, reason: impl Into<String>) -> Self {
        Self {
            verdict,
            reason: reason.into(),
            tls: Tls::Unknown,
        }
    }

    /// Attach what we found out about the certificate — and act on it.
    ///
    /// A site that loads perfectly well but has a broken certificate is **not** a
    /// working website as far as a customer is concerned: their browser shows a full
    /// page security warning before they ever see the shop. So it stops being
    /// `LiveSite` and becomes a lead.
    ///
    /// Only that one verdict changes. A site that was already dead, social, or
    /// missing does not become more interesting because of its certificate.
    fn with_tls(mut self, tls: Tls) -> Self {
        if self.verdict == Verdict::LiveSite && tls.is_a_problem() {
            self.verdict = Verdict::InsecureSite;
            self.reason = tls.describe().to_owned();
        }
        self.tls = tls;
        self
    }
}
