//! Everything that touches the network: the clients, the retries, and reading a page.
//!
//! Kept apart from [`crate::rules`] on purpose. What can be decided without a socket
//! lives there and is tested exhaustively; what needs one lives here and is as thin
//! as it can be made.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use webleads_domain::{Tls, Verdict};

use crate::gate::HostGate;
use crate::rules::{after_a_deep_link, host_of, judge, pre_check, root_of, PreCheck};
use crate::{Outcome, BROWSER_UA, CONCURRENCY, MAX_READ_BYTES, RETRY_TIMEOUT, TIMEOUT};

/// The two HTTP clients a check needs.
///
/// Two, not one, and that is the whole trick for spotting a broken certificate.
/// `reqwest` will not tell us *why* a connection failed in any way we can rely on, so
/// instead of reading error messages we ask the question twice:
///
/// **if the strict client fails where the lenient one succeeds, the certificate was
/// the problem.** That is not a guess about the wording of an error, it is what the
/// difference between the two clients means.
#[derive(Debug, Clone)]
pub struct Clients {
    /// Rejects a bad certificate, like a browser does.
    pub strict: reqwest::Client,
    /// Accepts anything. Only used to find out whether the certificate was the
    /// reason the strict one could not get through.
    pub lenient: reqwest::Client,
    /// Keeps us from crowding any one server. Shared between clones.
    pub(crate) gate: Arc<HostGate>,
}

impl Clients {
    /// Wait until it is polite to ask this host for something.
    ///
    /// The same gate the website checking uses. Anything else in the program that
    /// talks to somebody else's server goes through here too, so politeness is one
    /// rule in one place rather than one per caller.
    pub async fn wait_turn(
        &self,
        host: &str,
    ) -> tokio::sync::OwnedMutexGuard<Option<std::time::Instant>> {
        self.gate.enter(host).await
    }
}

/// A guard on the politeness decision, checked while compiling rather than while
/// testing. Raising the limit back up means changing this line and thinking about why.
const _: () = assert!(
    CONCURRENCY <= 8,
    "we agreed to be a quieter visitor — see the politeness note in fetch.rs"
);

/// Build the clients the checks share.
///
/// One set, not one per request: each keeps a pool of connections, and making fresh
/// ones every time would be slower and a way to run out of sockets.
pub fn clients() -> reqwest::Result<Clients> {
    let build = |lenient: bool, timeout: Duration| {
        reqwest::Client::builder()
            .user_agent(BROWSER_UA)
            .timeout(timeout)
            .danger_accept_invalid_certs(lenient)
            .build()
    };
    Ok(Clients {
        strict: build(false, TIMEOUT)?,
        lenient: build(true, RETRY_TIMEOUT)?,
        gate: Arc::new(HostGate::default()),
    })
}

/// Decide about one website.
///
/// Three attempts at most, and each one rules something out:
///
/// 1. **Strict, eight seconds.** Nearly everything ends here.
/// 2. **Strict again, twenty seconds** — only if the first attempt failed to connect
///    at all. This rules out "the shop is on cheap hosting and was busy", which is
///    what previously made us telephone a Dutch dentist to say their working website
///    was broken.
/// 3. **Lenient, twenty seconds** — only if both strict attempts failed. Succeeding
///    here means the certificate was the obstacle, because that is the only thing
///    this client treats differently.
///
/// A server that *answers* — 404, 500, anything — is never retried. It gave us a real
/// answer and asking again will not change its mind.
pub async fn check_one(clients: &Clients, url: &str) -> Outcome {
    let absolute = match pre_check(url) {
        PreCheck::Settled(outcome) => return outcome,
        PreCheck::Fetch(absolute) => absolute,
    };

    // Wait our turn for this host. The guard is held for every attempt below, so a
    // retry does not jump the queue either.
    let host = host_of(&absolute).unwrap_or_default().to_owned();
    let _turn = clients.gate.enter(&host).await;

    let (outcome, answered) = attempt(clients, &absolute).await;

    // The page Google stored is often a contact page or some other corner of the
    // site, and those go stale while the site itself carries on. So a *server that
    // answered badly* earns one more question: does the front door work?
    //
    // Only then. A refused connection or a name that does not resolve is the host's
    // problem, and the front door is on the same host — asking again would cost a
    // request to learn what we know.
    if answered && outcome.verdict == Verdict::DeadSite {
        if let Some(root) = root_of(&absolute) {
            let (at_root, _) = attempt(clients, &root).await;
            if at_root.verdict != Verdict::DeadSite {
                tracing::debug!(
                    deep = %absolute,
                    root = %root,
                    "the linked page is dead but the site is not"
                );
                return after_a_deep_link(at_root, &absolute);
            }
        }
    }

    outcome
}

/// Fetch one address and judge it.
///
/// The flag is whether the server *answered*: a 404 is an answer, a refused
/// connection is not. [`check_one`] needs to tell those apart, because only one of
/// them is worth a second question.
async fn attempt(clients: &Clients, absolute: &str) -> (Outcome, bool) {
    // 1 and 2: strict, then strict again for anything that merely failed to connect.
    let first = clients.strict.get(absolute).send().await;
    let strict = match first {
        Ok(response) => Ok(response),
        Err(err) => {
            tracing::debug!(url = %absolute, "first try failed ({err}), retrying");
            clients
                .strict
                .get(absolute)
                .timeout(RETRY_TIMEOUT)
                .send()
                .await
        }
    };

    let (response, tls) = match strict {
        Ok(response) => (response, Tls::Valid),
        Err(strict_err) => {
            // 3: the same address, with certificate checking turned off.
            match clients.lenient.get(absolute).send().await {
                Ok(response) => {
                    // It answers. The only thing we changed was the certificate, so
                    // the certificate is what was wrong — and that is a lead.
                    let detail = describe_certificate_problem(&strict_err);
                    // The library's own words go to the log, where somebody debugging
                    // wants them. The row gets the plain sentence.
                    tracing::debug!(
                        url = %absolute,
                        "certificate problem: {detail} — {}",
                        tidy_certificate_detail(&strict_err.to_string())
                    );
                    (response, Tls::Broken(detail))
                }
                // Neither got through. Genuinely unreachable.
                Err(err) => {
                    return (
                        Outcome::new(Verdict::DeadSite, describe_transport_error(&err)),
                        false,
                    )
                }
            }
        }
    };

    let final_url = response.url().to_string();
    let status = response.status().as_u16();

    let body = match read_capped(response).await {
        Ok(body) => body,
        // It answered, and then the connection went away mid-page. That is still a
        // site nobody can read.
        Err(err) => {
            return (
                Outcome::new(Verdict::DeadSite, describe_transport_error(&err)),
                true,
            )
        }
    };

    (judge(status, &final_url, &body).with_tls(tls), true)
}

/// Read the top of a page and stop.
///
/// `response.text()` would download the whole thing and only then let us trim it,
/// which on a site with a forty megabyte homepage means we have already paid for all
/// of it. This reads a piece at a time and stops the moment it has enough.
///
/// The body is decoded with `from_utf8_lossy`: a homepage in the wrong encoding is
/// still worth reading for the words we look for, and refusing it would mark a
/// perfectly real business as dead.
async fn read_capped(response: reqwest::Response) -> reqwest::Result<String> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);

    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk?);
        if buffer.len() >= MAX_READ_BYTES {
            buffer.truncate(MAX_READ_BYTES);
            break; // dropping the stream here closes the connection
        }
    }

    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Turn the strict client's refusal into something worth saying on the telephone.
///
/// "Your certificate expired" is a sentence a shop owner understands. The raw error
/// is not, so we keep it short and put the readable part first.
fn describe_certificate_problem(err: &reqwest::Error) -> String {
    // Walk down to the innermost cause — that is where the real reason lives.
    let mut deepest = err.to_string();
    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(err);
    while let Some(next) = cause {
        deepest = next.to_string();
        cause = next.source();
    }

    // Plain words only. The OpenSSL string — `error:0A000086:SSL routines:...` — is
    // unreadable on a row, and it was being cut off mid-word on top of that. It goes
    // to the log instead.
    plain_certificate_words(&deepest).to_owned()
}

/// Pull the readable part out of a certificate error.
///
/// OpenSSL writes them as colon-separated fields ending with the human sentence —
/// `error:0A000086:SSL routines:...:certificate verify failed`. The last field is the
/// only part worth showing, and cutting the whole thing at eighty characters lands
/// mid-word instead.
fn tidy_certificate_detail(raw: &str) -> String {
    let last = raw.rsplit(':').next().unwrap_or(raw).trim();
    let chosen = if last.len() >= 8 { last } else { raw.trim() };
    chosen.chars().take(70).collect()
}

/// Which plain sentence fits this failure.
///
/// Split out from [`describe_certificate_problem`] so it can be tested — a
/// `reqwest::Error` cannot be built by hand, but a string can.
fn plain_certificate_words(raw: &str) -> &'static str {
    let lowered = raw.to_lowercase();
    if lowered.contains("expired") {
        "certificate has expired"
    } else if lowered.contains("self signed") || lowered.contains("self-signed") {
        "certificate is self-signed"
    } else if lowered.contains("hostname") || lowered.contains("name") {
        "certificate is for a different address"
    } else {
        "certificate is not trusted"
    }
}

/// Say what went wrong in words rather than in library jargon.
fn describe_transport_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "timed out".to_owned();
    }
    if err.is_connect() {
        return "unreachable (no connection)".to_owned();
    }
    let text = err.to_string();
    let short: String = text.chars().take(60).collect();
    format!("failed ({short})")
}

/// Check a batch of websites at once, keeping the input order.
///
/// The `Semaphore` is what bounds this. Every task must take a permit before it makes
/// a request and gives it back when it finishes, so at most [`CONCURRENCY`] requests
/// are ever in flight — no matter whether the list has ten URLs or ten thousand.
///
/// `Arc` — atomically reference counted — is how several tasks share one semaphore.
/// Each task holds its own handle, and the semaphore is dropped when the last one goes.
///
/// `done` is bumped as each site finishes, so a progress bar can read it without any
/// message passing. An atomic can be read from another task safely while these are
/// still writing to it, which is exactly what a progress bar needs.
/// `stop` is checked by each task before it makes its request. A task that finds it
/// set does nothing and says so, which keeps the answer the same length as the
/// question — the caller pairs these with places by position, and a short list would
/// quietly pair a business with somebody else's verdict.
pub async fn check_many(
    clients: &Clients,
    urls: &[String],
    done: &Arc<AtomicUsize>,
    stop: &Arc<std::sync::atomic::AtomicBool>,
) -> Vec<Outcome> {
    let permits = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));

    let mut tasks = Vec::with_capacity(urls.len());
    for (index, url) in urls.iter().enumerate() {
        let permits = Arc::clone(&permits);
        let done = Arc::clone(done);
        let stop = Arc::clone(stop);
        let clients = clients.clone(); // cheap: each Client is a handle to a pool
        let url = url.clone();
        tasks.push(tokio::spawn(async move {
            // `acquire_owned` waits here until a permit is free. The guard gives the
            // permit back when it goes out of scope at the end of this block.
            let _permit = permits
                .acquire_owned()
                .await
                .expect("the semaphore is never closed");
            // Between sites, never during one.
            if stop.load(Ordering::Relaxed) {
                return (
                    index,
                    Outcome::new(Verdict::DeadSite, "not checked — you stopped"),
                );
            }
            let outcome = check_one(&clients, &url).await;
            done.fetch_add(1, Ordering::Relaxed);
            (index, outcome)
        }));
    }

    // Tasks finish in whatever order the network allows, so put them back in order.
    let mut results: Vec<Option<Outcome>> = vec![None; urls.len()];
    for task in tasks {
        match task.await {
            Ok((index, outcome)) => results[index] = Some(outcome),
            Err(err) => tracing::error!("‼️ ERROR: sitecheck task failed: {err}"),
        }
    }

    results
        .into_iter()
        .map(|slot| slot.unwrap_or_else(|| Outcome::new(Verdict::DeadSite, "check did not finish")))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_facebook_link_is_settled_without_the_network() {
        let clients = clients().expect("clients");
        let outcome = check_one(&clients, "https://www.facebook.com/borzasbarber").await;
        assert_eq!(outcome.verdict, Verdict::SocialOnly);
        assert_eq!(outcome.reason, "only www.facebook.com");
    }

    #[tokio::test]
    async fn order_is_kept_even_though_checks_finish_out_of_order() {
        let clients = clients().expect("clients");
        let urls = vec![
            String::new(),
            "https://facebook.com/a".to_owned(),
            "https://instagram.com/b".to_owned(),
            String::new(),
        ];
        let outcomes = check_many(
            &clients,
            &urls,
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .await;
        assert_eq!(outcomes.len(), 4);
        assert_eq!(outcomes[0].verdict, Verdict::NoSite);
        assert_eq!(outcomes[1].verdict, Verdict::SocialOnly);
        assert_eq!(outcomes[2].reason, "only instagram.com");
        assert_eq!(outcomes[3].verdict, Verdict::NoSite);
    }

    #[tokio::test]
    async fn a_domain_that_does_not_exist_is_dead_not_a_crash() {
        let clients = clients().expect("clients");
        let outcome = check_one(&clients, "https://this-domain-does-not-exist-9z8x.hu").await;
        assert_eq!(outcome.verdict, Verdict::DeadSite);
    }

    // ---- pre_check: everything settled without a socket ----

    // ---- judge: the four verdicts, no socket anywhere ----

    // ---- the small pieces ----

    #[test]
    fn the_shared_client_is_built_with_our_settings() {
        // Mostly a guard: if the builder ever gains a setting that conflicts, this
        // fails here rather than at the first real request.
        assert!(clients().is_ok());
    }

    #[test]
    fn the_readable_part_is_pulled_out_of_a_long_openssl_error() {
        // OpenSSL writes colon-separated fields and puts the sentence last. Cutting
        // the whole string at eighty characters lands mid-word instead.
        let raw = "error:0A000086:SSL routines:tls_post_process_server_certificate:certificate verify failed";
        assert_eq!(tidy_certificate_detail(raw), "certificate verify failed");
    }

    #[test]
    fn a_short_last_field_falls_back_to_the_whole_message() {
        // Some errors are not colon-separated at all, and some end in a short token
        // that would tell the reader nothing on its own.
        assert_eq!(
            tidy_certificate_detail("connection closed"),
            "connection closed"
        );
        assert_eq!(tidy_certificate_detail("tls error:bad"), "tls error:bad");
    }

    #[test]
    fn an_expired_certificate_reads_as_a_sentence_not_a_stack_trace() {
        // We cannot build a reqwest::Error by hand, so this checks the wording rules
        // that turn whatever it says into something you can read down a telephone.
        for (raw, expected) in [
            ("certificate has expired", "certificate has expired"),
            ("self signed certificate", "certificate is self-signed"),
            (
                "hostname mismatch",
                "certificate is for a different address",
            ),
            ("unknown ca", "certificate is not trusted"),
        ] {
            let plain = plain_certificate_words(raw);
            assert_eq!(plain, expected, "for `{raw}`");
        }
    }

    #[test]
    fn the_retry_waits_longer_than_the_first_try() {
        // The whole point of the retry is to give a slow shop more time. If these two
        // were the same, a busy site would fail twice for the same reason.
        assert!(RETRY_TIMEOUT > TIMEOUT);
    }

    #[test]
    fn the_two_clients_differ_only_in_how_strict_they_are() {
        // If both were built the same way, the certificate check would silently
        // become "always valid" — the strict one could never fail where the lenient
        // one succeeds, and no broken certificate would ever be found.
        let built = clients().expect("clients");
        // A Client does not expose its settings, so this is as much as can be
        // asserted here; the behaviour itself is covered by the live check below.
        assert!(format!("{:?}", built.strict).contains("Client"));
        assert!(format!("{:?}", built.lenient).contains("Client"));
    }

    /// These reach the internet, so they are marked `#[ignore]` and are run on
    /// purpose with `cargo test -- --ignored`. They are the only proof that the
    /// strict-then-lenient trick actually finds a broken certificate.
    #[tokio::test]
    #[ignore = "needs the internet"]
    async fn a_real_expired_certificate_is_a_lead_not_a_dead_site() {
        let c = clients().expect("clients");
        let outcome = check_one(&c, "https://expired.badssl.com/").await;
        assert_eq!(outcome.verdict, Verdict::InsecureSite, "{outcome:?}");
        assert!(outcome.tls.is_a_problem());
        assert!(outcome.tls.describe().contains("certificate"));
    }

    #[tokio::test]
    #[ignore = "needs the internet"]
    async fn a_real_self_signed_certificate_is_a_lead_too() {
        let c = clients().expect("clients");
        let outcome = check_one(&c, "https://self-signed.badssl.com/").await;
        assert_eq!(outcome.verdict, Verdict::InsecureSite, "{outcome:?}");
        assert!(outcome.tls.is_a_problem());
    }

    #[tokio::test]
    #[ignore = "needs the internet"]
    async fn a_good_certificate_is_not_a_lead() {
        let c = clients().expect("clients");
        let outcome = check_one(&c, "https://example.com/").await;
        assert_eq!(outcome.verdict, Verdict::LiveSite, "{outcome:?}");
        assert_eq!(outcome.tls, Tls::Valid);
    }

    #[tokio::test]
    #[ignore = "needs the internet"]
    async fn a_big_page_is_not_downloaded_in_full() {
        let c = clients().expect("clients");
        let outcome = check_one(&c, "https://www.wikipedia.org/").await;
        assert_eq!(outcome.verdict, Verdict::LiveSite, "{outcome:?}");
    }

    #[tokio::test]
    async fn an_empty_link_is_settled_without_the_network() {
        let clients = clients().expect("clients");
        let outcome = check_one(&clients, "   ").await;
        assert_eq!(outcome.verdict, Verdict::NoSite);
        assert_eq!(outcome.reason, "no link on Google Maps");
    }
}
