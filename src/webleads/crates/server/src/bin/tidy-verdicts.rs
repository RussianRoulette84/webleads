//! Go back over every business we called broken, one at a time.
//!
//! # Why this exists
//!
//! Google Maps stores whatever address the owner pasted in, and that is often a page
//! inside the site rather than its front door. Those pages go stale while the site
//! carries on, so `…/kapcsolat.html` answers 404 and the business landed on the call
//! list as a lead — with a working website. A false lead is worse than a missing
//! one: you ring somebody to tell them their site is broken and it is not.
//!
//! The rules live in `sitecheck`: a server that *answers badly* to a page inside a
//! site earns one more question — does the front door work? — and a robot check in
//! front of a site means it is up. This asks every business we called broken again,
//! under today's rules.
//!
//! A one-off, run by hand:
//!
//! ```text
//! cargo run -p webleads-server --bin tidy-verdicts
//! cargo run -p webleads-server --bin tidy-verdicts -- --answered
//! ```
//!
//! `--answered` asks again only the ones whose server *answered* with an error — a
//! 403, a 503 — which is what a robot check in front of a working site looks like.
//! The unreachable ones are left alone: each costs half a minute of timing out.
//!
//! Nothing is deleted. A verdict is rewritten only when the site answers differently
//! from what we stored, and every business is named on its own line as it goes.

use std::path::PathBuf;

use webleads_sitecheck::rules::root_of;
use webleads_store::Store;

/// The same database the server uses.
const DB_PATH: &str = "data/webleads.db";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Four levels: crates/server → crates → the workspace → src → the repository.
    // The workspace sits at `src/webleads/`, and everything the program writes hangs
    // off the repository root above it.
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .map(PathBuf::from)
        .expect("this crate is four levels inside the repository");

    let db = repo.join(DB_PATH);
    // Say which one, and stop rather than quietly making an empty one somewhere else.
    if !db.exists() {
        anyhow::bail!("no database at {}", db.display());
    }
    let store = Store::open(&db).await?;
    let clients = webleads_sitecheck::clients()?;

    let only_answered = std::env::args().any(|a| a == "--answered");
    let suspects: Vec<_> = store
        .called_broken()
        .await?
        .into_iter()
        .filter(|s| worth_asking_again(&s.reason, only_answered))
        .collect();
    println!(
        "\x1b[1;35m=== {} businesses were called broken; asking each one again ===\x1b[0m",
        suspects.len()
    );

    let mut freed = 0_usize;
    let mut changed = 0_usize;
    for (at, suspect) in suspects.iter().enumerate() {
        let deep = root_of(&suspect.website).is_some();
        let outcome = webleads_sitecheck::check_one(&clients, &suspect.website).await;

        let counting = format!("{:>3}/{}", at + 1, suspects.len());
        if outcome.verdict == webleads_domain::Verdict::DeadSite {
            // Still broken. Left exactly as it is, including the reason, which may
            // well be a better one than we had.
            println!(
                "{counting} \x1b[2mstill broken\x1b[0m  {} — {}",
                suspect.name, outcome.reason
            );
            continue;
        }

        store
            .rewrite_verdict(
                &suspect.business_id,
                outcome.verdict,
                &outcome.reason,
                &outcome.tls,
            )
            .await?;
        changed += 1;
        if outcome.verdict == webleads_domain::Verdict::LiveSite {
            freed += 1;
        }

        // Green for the ones that leave the list, yellow for the ones that stay on it
        // under a different heading.
        let colour = if outcome.verdict == webleads_domain::Verdict::LiveSite {
            "\x1b[32m"
        } else {
            "\x1b[33m"
        };
        println!(
            "{counting} {colour}{}\x1b[0m  {} — {}{}",
            outcome.verdict.describe(),
            suspect.name,
            outcome.reason,
            if deep { " [link into the site]" } else { "" }
        );
    }

    println!(
        "\x1b[1;35m=== {changed} verdicts rewritten, {freed} of them off the call list ===\x1b[0m"
    );
    Ok(())
}

/// Is this one worth asking again? Everything, unless only the ones whose server
/// answered were asked for. Pure, so the choice is a test.
fn worth_asking_again(reason: &str, only_answered: bool) -> bool {
    !only_answered || reason.starts_with("server answers")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asked_for_everything_every_one_is_asked_again() {
        assert!(worth_asking_again("unreachable (no connection)", false));
        assert!(worth_asking_again("server answers 403", false));
    }

    #[test]
    fn asked_for_the_answered_ones_the_unreachable_are_left_alone() {
        assert!(worth_asking_again("server answers 403", true));
        assert!(worth_asking_again("server answers 503", true));
        assert!(!worth_asking_again("unreachable (no connection)", true));
        assert!(!worth_asking_again("timed out", true));
        assert!(!worth_asking_again("parked or placeholder page", true));
    }
}
