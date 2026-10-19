//! The rules that decide what a website is, with no network anywhere.
//!
//! Every function here is pure: the same input always gives the same answer, and
//! nothing opens a socket. That is what makes the rule the whole product rests on
//! testable one case at a time.

use webleads_domain::{is_social_host, looks_like_bot_check, looks_parked, Verdict};

use crate::{Outcome, MIN_REAL_PAGE_BYTES};

/// Pull the host out of a URL without a URL parser.
///
/// Returns `None` when there is nothing that looks like a host. Written by hand
/// because the alternative is another dependency for four lines of string work.
pub(crate) fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .trim()
        .strip_prefix("https://")
        .or_else(|| url.trim().strip_prefix("http://"))
        .unwrap_or(url.trim());
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .rsplit('@') // drop any user:password@ part
        .next()?
        .split(':') // drop any :port
        .next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// The site's own front door, for an address that points at a page inside it.
///
/// `None` when the address *is* the front door — there is nothing else to try then.
///
/// **This is the fix for the false leads.** Google Maps often stores whatever page
/// the owner pasted in, which is frequently a contact or a sub-page. Those go stale
/// while the site itself carries on perfectly well, so `…/kapcsolat.html` answers 404
/// and we called a working business to tell them their website was broken.
///
/// Pure, so every shape of address is a test rather than a surprise.
pub fn root_of(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        // No scheme is how Google stores a bare domain; `pre_check` assumes https.
        None => ("https", trimmed),
    };
    // A host we can make sense of at all, with no port or credentials in it.
    host_of(trimmed)?;
    // The authority exactly as written, so a port survives the trip.
    let authority = rest.split(['/', '?', '#']).next()?;
    let tail = &rest[authority.len()..];
    // Nothing past the host, or a lone slash: this already is the front door.
    if tail.is_empty() || tail == "/" {
        return None;
    }
    Some(format!("{scheme}://{authority}/"))
}

/// The part of an address after the host, for saying which page was missing.
pub fn path_of(url: &str) -> String {
    let rest = url
        .trim()
        .split_once("://")
        .map_or(url.trim(), |(_, rest)| rest);
    match rest.find(['/', '?', '#']) {
        Some(at) => rest[at..].to_owned(),
        None => "/".to_owned(),
    }
}

/// What to record when the page Google linked to is missing but the site answers.
///
/// The verdict is the front door's — the business has a website, whatever state the
/// link is in — and the reason says both halves, because "loads fine" on a row whose
/// stored address 404s would look like a bug rather than an explanation.
pub fn after_a_deep_link(at_root: Outcome, deep_link: &str) -> Outcome {
    let path = path_of(deep_link);
    Outcome {
        reason: format!("site itself works; Google's link to {path} is dead"),
        ..at_root
    }
}

/// What [`pre_check`] concluded before anything touched the network.
///
/// An enum rather than `Option<Outcome>` because "we already know the answer" and
/// "go and fetch this address" are two different things, and a type that says so
/// cannot be misread at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreCheck {
    /// Settled without a request. Most of our leads end here.
    Settled(Outcome),
    /// Worth fetching. Carries the address with a scheme on the front.
    Fetch(String),
}

/// Everything that can be decided about a URL without asking the network.
///
/// Pure: same input, same answer, no sockets. That is what makes the rule testable.
pub fn pre_check(url: &str) -> PreCheck {
    let url = url.trim();
    if url.is_empty() {
        return PreCheck::Settled(Outcome::new(Verdict::NoSite, "no link on Google Maps"));
    }

    if let Some(host) = host_of(url) {
        if is_social_host(host) {
            return PreCheck::Settled(Outcome::new(Verdict::SocialOnly, format!("only {host}")));
        }
    }

    // Google Maps often stores a bare domain with no scheme.
    let absolute = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    PreCheck::Fetch(absolute)
}

/// Judge a page we have already fetched.
///
/// Takes the three things that actually matter — the status, where we ended up after
/// any redirects, and the top of the body — rather than a `Response`. That is the
/// whole trick: a `Response` can only come from a socket, but a number and two
/// strings can come from a test.
pub fn judge(status: u16, final_url: &str, body: &str) -> Outcome {
    // A site that redirects to Facebook is a Facebook page with a nicer address.
    if let Some(host) = host_of(final_url) {
        if is_social_host(host) {
            return Outcome::new(Verdict::SocialOnly, format!("redirects to {host}"));
        }
    }

    // A robot check is somebody protecting a site that is up. It answers 403 or 503
    // to anything that is not a person in a browser, and believing the status called
    // working websites broken and put them on the call list.
    if looks_like_bot_check(body) {
        return Outcome::new(
            Verdict::LiveSite,
            format!("up, behind a robot check ({status})"),
        );
    }

    if !(200..300).contains(&status) {
        return Outcome::new(Verdict::DeadSite, format!("server answers {status}"));
    }

    if looks_parked(body) {
        return Outcome::new(Verdict::DeadSite, "parked or placeholder page");
    }
    if body.trim().len() < MIN_REAL_PAGE_BYTES {
        return Outcome::new(Verdict::DeadSite, "page is effectively empty");
    }

    Outcome::new(Verdict::LiveSite, format!("loads fine ({status})"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use webleads_domain::Tls;

    #[test]
    fn a_page_inside_a_site_knows_where_its_front_door_is() {
        // The false lead this fixes: Google stored the contact page, the contact page
        // 404s, and the site itself is perfectly well.
        assert_eq!(
            root_of("https://www.viz-gaz-futesszereles.hu/kapcsolat.html").as_deref(),
            Some("https://www.viz-gaz-futesszereles.hu/")
        );
        assert_eq!(
            root_of("http://www.klima-javitas.hu/index.php?jav=szer").as_deref(),
            Some("http://www.klima-javitas.hu/")
        );
        // The scheme is kept, and so is a port.
        assert_eq!(
            root_of("http://example.hu:8080/a/b").as_deref(),
            Some("http://example.hu:8080/")
        );
        // No scheme is how Google stores a bare domain; https is what we would try.
        assert_eq!(
            root_of("example.hu/kapcsolat").as_deref(),
            Some("https://example.hu/")
        );
        // A query with no path still counts as somewhere other than the front door.
        assert_eq!(
            root_of("https://example.hu?oldal=ceginformacio").as_deref(),
            Some("https://example.hu/")
        );
    }

    #[test]
    fn a_front_door_has_nothing_else_to_try() {
        // These already are the root, so asking again would be the same request.
        assert_eq!(root_of("https://example.hu"), None);
        assert_eq!(root_of("https://example.hu/"), None);
        assert_eq!(root_of("example.hu"), None);
        assert_eq!(root_of("  https://example.hu/  "), None);
        assert_eq!(root_of(""), None);
    }

    #[test]
    fn the_missing_page_is_named_in_plain_words() {
        assert_eq!(
            path_of("https://example.hu/kapcsolat.html"),
            "/kapcsolat.html"
        );
        assert_eq!(
            path_of("http://example.hu/index.php?jav=x"),
            "/index.php?jav=x"
        );
        assert_eq!(path_of("https://example.hu?oldal=x"), "?oldal=x");
        assert_eq!(path_of("https://example.hu"), "/");
    }

    #[test]
    fn a_working_site_behind_a_dead_link_is_not_a_lead() {
        let at_root = Outcome::new(Verdict::LiveSite, "loads fine (200)");
        let said = after_a_deep_link(at_root, "https://example.hu/kapcsolat.html");
        // The verdict is the front door's: they have a website, so they are not a
        // lead, however stale Google's link is.
        assert_eq!(said.verdict, Verdict::LiveSite);
        // And the reason says both halves, so the row does not look like a bug.
        assert!(
            said.reason.contains("/kapcsolat.html"),
            "got {}",
            said.reason
        );
        assert!(said.reason.contains("works"), "got {}", said.reason);
    }

    #[test]
    fn a_front_door_that_is_only_a_facebook_page_is_still_a_lead() {
        // The link being dead does not make a social-only site into a real one.
        let at_root = Outcome::new(Verdict::SocialOnly, "redirects to facebook.com");
        let said = after_a_deep_link(at_root, "https://example.hu/kapcsolat.html");
        assert_eq!(said.verdict, Verdict::SocialOnly);
    }

    #[test]
    fn hosts_come_out_of_every_shape_of_url() {
        assert_eq!(host_of("https://facebook.com/borzas"), Some("facebook.com"));
        assert_eq!(
            host_of("http://www.example.hu:8080/x"),
            Some("www.example.hu")
        );
        assert_eq!(host_of("example.hu"), Some("example.hu"));
        assert_eq!(host_of("https://example.hu?a=1"), Some("example.hu"));
        assert_eq!(host_of(""), None);
    }

    /// A page long enough to count as real. The rule is 200 bytes.
    fn real_page() -> String {
        format!(
            "<html><body><h1>Fodrászat Zugló</h1><p>{}</p></body></html>",
            "nyitva 9-17. ".repeat(20)
        )
    }

    #[test]
    fn an_empty_link_is_settled_before_any_request() {
        for empty in ["", "   ", "\t\n"] {
            match pre_check(empty) {
                PreCheck::Settled(o) => {
                    assert_eq!(o.verdict, Verdict::NoSite);
                    assert_eq!(o.reason, "no link on Google Maps");
                }
                PreCheck::Fetch(url) => panic!("would have fetched {url}"),
            }
        }
    }

    #[test]
    fn a_social_link_is_settled_before_any_request() {
        for (url, host) in [
            ("https://facebook.com/borzas", "facebook.com"),
            ("https://www.instagram.com/borzas", "www.instagram.com"),
            ("tiktok.com/@borzas", "tiktok.com"),
        ] {
            match pre_check(url) {
                PreCheck::Settled(o) => {
                    assert_eq!(o.verdict, Verdict::SocialOnly);
                    assert_eq!(o.reason, format!("only {host}"));
                }
                PreCheck::Fetch(u) => panic!("would have fetched {u}"),
            }
        }
    }

    #[test]
    fn a_bare_domain_gets_a_scheme_put_on_the_front() {
        assert_eq!(
            pre_check("borzas.hu"),
            PreCheck::Fetch("https://borzas.hu".into())
        );
        // One that already has a scheme is left exactly as it was.
        assert_eq!(
            pre_check("http://borzas.hu"),
            PreCheck::Fetch("http://borzas.hu".into())
        );
        assert_eq!(
            pre_check("  https://borzas.hu  "),
            PreCheck::Fetch("https://borzas.hu".into())
        );
    }

    #[test]
    fn a_real_page_that_answers_200_is_a_live_site() {
        let o = judge(200, "https://borzas.hu/", &real_page());
        assert_eq!(o.verdict, Verdict::LiveSite);
        assert_eq!(o.reason, "loads fine (200)");
        // A live site is not a lead — this is the line the whole product rests on.
        assert!(!o.verdict.is_lead());
    }

    #[test]
    fn every_other_2xx_also_counts_as_alive() {
        for status in [200, 201, 202, 299] {
            assert_eq!(
                judge(status, "https://borzas.hu/", &real_page()).verdict,
                Verdict::LiveSite,
                "status {status}"
            );
        }
    }

    #[test]
    fn a_page_that_errors_is_a_dead_site_and_says_the_number() {
        for status in [400, 403, 404, 410, 500, 502, 503] {
            let o = judge(status, "https://borzas.hu/", &real_page());
            assert_eq!(o.verdict, Verdict::DeadSite, "status {status}");
            assert_eq!(o.reason, format!("server answers {status}"));
        }
    }

    #[test]
    fn a_redirect_is_not_treated_as_success() {
        // We follow redirects, so seeing a 301 here means it was never followed.
        assert_eq!(
            judge(301, "https://borzas.hu/", &real_page()).verdict,
            Verdict::DeadSite
        );
    }

    #[test]
    fn a_site_behind_a_robot_check_is_up_whatever_it_answers() {
        // What Cloudflare sends a program instead of the shop: a 403 or a 503 and its
        // challenge page. The site behind it is working, so it is not a lead.
        let challenge = "<html><head><title>Just a moment...</title></head><body>\
            <script>window._cf_chl_opt={cvId:'3'};</script>\
            <script src=\"/cdn-cgi/challenge-platform/h/b/orchestrate/chl_page/v1\"></script>\
            </body></html>";
        for status in [200, 403, 429, 503] {
            let o = judge(status, "https://euromotor.hu/", challenge);
            assert_eq!(o.verdict, Verdict::LiveSite, "status {status}");
            assert!(o.reason.contains("robot check"), "{}", o.reason);
        }
    }

    #[test]
    fn a_parked_page_is_dead_even_though_it_answers_200() {
        for text in [
            "<h1>This domain is for sale</h1>",
            "<title>Buy this domain</title>",
            "<h1>Welcome to nginx!</h1>",
            "Apache2 Ubuntu Default Page",
            "<p>Coming soon</p>",
        ] {
            let padded = format!("{text}{}", " ".repeat(400));
            let o = judge(200, "https://borzas.hu/", &padded);
            assert_eq!(o.verdict, Verdict::DeadSite, "{text}");
            assert_eq!(o.reason, "parked or placeholder page");
        }
    }

    #[test]
    fn a_page_with_almost_nothing_in_it_is_dead() {
        let o = judge(200, "https://borzas.hu/", "<html></html>");
        assert_eq!(o.verdict, Verdict::DeadSite);
        assert_eq!(o.reason, "page is effectively empty");
        assert_eq!(
            judge(200, "https://borzas.hu/", "").verdict,
            Verdict::DeadSite
        );
    }

    #[test]
    fn a_page_of_nothing_but_whitespace_is_empty_however_long_it_is() {
        // 400 spaces is over the byte threshold but still an empty page. Trimming
        // before measuring is what catches this.
        let o = judge(200, "https://borzas.hu/", &" ".repeat(400));
        assert_eq!(o.verdict, Verdict::DeadSite);
        assert_eq!(o.reason, "page is effectively empty");
    }

    #[test]
    fn landing_on_a_social_network_beats_every_other_verdict() {
        // Checked first on purpose: a shop whose domain forwards to Instagram has no
        // website, and that is more useful to know than the status it answered with.
        let o = judge(200, "https://www.instagram.com/borzas", &real_page());
        assert_eq!(o.verdict, Verdict::SocialOnly);
        assert_eq!(o.reason, "redirects to www.instagram.com");

        // Even a 404 on a social host reads as social rather than dead.
        assert_eq!(
            judge(404, "https://facebook.com/gone", "").verdict,
            Verdict::SocialOnly
        );
    }

    #[test]
    fn a_lookalike_domain_does_not_read_as_a_redirect_to_social() {
        let o = judge(200, "https://notfacebook.hu/", &real_page());
        assert_eq!(o.verdict, Verdict::LiveSite);
    }

    #[test]
    fn an_outcome_takes_a_borrowed_or_an_owned_reason() {
        let borrowed = Outcome::new(Verdict::NoSite, "no link");
        let owned = Outcome::new(Verdict::NoSite, String::from("no link"));
        assert_eq!(borrowed, owned);
    }

    #[test]
    fn a_working_site_with_a_broken_certificate_becomes_a_lead() {
        // The site loads. Without this rule it would be dropped as "has a website" —
        // but every visitor sees a security warning before they see the shop.
        let live = judge(200, "https://borzas.hu/", &real_page());
        assert_eq!(live.verdict, Verdict::LiveSite);
        assert_eq!(live.tls, Tls::Unknown);

        let flagged = live.with_tls(Tls::Broken("certificate has expired".into()));
        assert_eq!(flagged.verdict, Verdict::InsecureSite);
        assert!(flagged.verdict.is_lead());
        assert_eq!(flagged.reason, "certificate has expired");
    }

    #[test]
    fn a_good_certificate_leaves_a_working_site_alone() {
        let flagged = judge(200, "https://borzas.hu/", &real_page()).with_tls(Tls::Valid);
        assert_eq!(flagged.verdict, Verdict::LiveSite);
        assert!(!flagged.verdict.is_lead());
        assert_eq!(flagged.reason, "loads fine (200)");
    }

    #[test]
    fn a_certificate_does_not_make_an_already_dead_site_more_interesting() {
        // These are already leads for a better reason. Overwriting it with the
        // certificate would lose the sentence the caller actually needs.
        for (status, body) in [(404, ""), (200, "<html></html>")] {
            let before = judge(status, "https://borzas.hu/", body);
            let after = before.clone().with_tls(Tls::Broken("expired".into()));
            assert_eq!(after.verdict, before.verdict);
            assert_eq!(after.reason, before.reason);
            // The fact is still recorded, it just does not change the verdict.
            assert!(after.tls.is_a_problem());
        }
    }

    #[test]
    fn a_social_page_with_a_bad_certificate_is_still_a_social_page() {
        let before = judge(200, "https://facebook.com/borzas", &real_page());
        let after = before.with_tls(Tls::Broken("expired".into()));
        assert_eq!(after.verdict, Verdict::SocialOnly);
    }
}
