//! Keeping us from crowding any one server.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long to leave between two requests to the same host.
pub const PER_HOST_GAP: Duration = Duration::from_millis(500);

/// How long a host is remembered after we last asked it for anything.
///
/// Long enough that a run never forgets mid-flight, short enough that a table of
/// thirty thousand hosts does not sit in memory for the life of the server.
pub const REMEMBER_HOST_FOR: Duration = Duration::from_secs(600);

/// One request at a time per host, with a pause between.
///
/// These are small shops on cheap shared hosting, and several of them may well sit on
/// the same server. Sixteen at once with no pause is how you get noticed, blocked, or
/// blamed for somebody's website falling over.
///
/// The outer `Mutex` is a plain one, held only long enough to look a host up. The
/// inner one is `tokio`'s async mutex, and it is held across the whole request —
/// which is exactly what makes the requests to one host queue up behind each other.
#[derive(Debug, Default)]
pub struct HostGate {
    hosts: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<std::time::Instant>>>>>,
}

impl HostGate {
    /// Wait until it is polite to ask this host for something.
    ///
    /// Returns the guard. Hold it for the whole request and drop it afterwards; while
    /// you have it, nothing else will touch that host.
    pub async fn enter(
        &self,
        host: &str,
    ) -> tokio::sync::OwnedMutexGuard<Option<std::time::Instant>> {
        let slot = {
            let mut hosts = self.hosts.lock().expect("host gate is never poisoned");
            // Drop anything nobody has asked about in a while. Done here rather than
            // on a timer: the only moment this table can grow is the moment we add to
            // it, so it is also the only moment worth tidying it.
            forget_idle(&mut hosts);
            Arc::clone(hosts.entry(host.to_owned()).or_default())
        };
        let mut guard = slot.lock_owned().await;
        if let Some(last) = *guard {
            let since = last.elapsed();
            if since < PER_HOST_GAP {
                tokio::time::sleep(PER_HOST_GAP - since).await;
            }
        }
        *guard = Some(std::time::Instant::now());
        guard
    }
}

/// Drop hosts nobody has asked about for a while.
///
/// A host still being used right now has its slot locked, and `try_lock` fails on it,
/// so an in-flight request can never be forgotten out from under itself.
fn forget_idle(hosts: &mut HashMap<String, Arc<tokio::sync::Mutex<Option<std::time::Instant>>>>) {
    hosts.retain(|_, slot| match slot.try_lock() {
        // Locked means somebody is talking to it. Keep it.
        Err(_) => true,
        Ok(last) => match *last {
            None => true,
            Some(when) => when.elapsed() < REMEMBER_HOST_FOR,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_requests_to_one_host_wait_for_each_other() {
        // The point of the gate: whatever else is running, one host sees one request
        // at a time, with a pause between them.
        let gate = Arc::new(HostGate::default());
        let started = std::time::Instant::now();

        let first = gate.enter("borzas.hu").await;
        drop(first);
        let _second = gate.enter("borzas.hu").await;

        assert!(
            started.elapsed() >= PER_HOST_GAP,
            "the second request did not wait: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn different_hosts_do_not_wait_for_each_other() {
        // Two different shops must not queue behind one another — that would make the
        // whole run as slow as one request at a time.
        let gate = Arc::new(HostGate::default());
        let started = std::time::Instant::now();

        let _a = gate.enter("borzas.hu").await;
        let _b = gate.enter("petraz.hu").await;
        let _c = gate.enter("fodraszat.hu").await;

        assert!(
            started.elapsed() < PER_HOST_GAP,
            "different hosts waited on each other: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn the_gate_holds_a_host_until_the_guard_is_dropped() {
        // While one request is in flight, a second to the same host must not get in.
        let gate = Arc::new(HostGate::default());
        let held = gate.enter("borzas.hu").await;

        let other = Arc::clone(&gate);
        let waiting = tokio::spawn(async move { other.enter("borzas.hu").await });

        // Give it a moment to try and fail to get in.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !waiting.is_finished(),
            "the second request got in too early"
        );

        drop(held);
        waiting
            .await
            .expect("the waiting request finishes once the first lets go");
    }

    #[test]
    fn a_host_nobody_has_asked_about_is_forgotten() {
        let mut hosts = HashMap::new();
        let long_ago = std::time::Instant::now() - REMEMBER_HOST_FOR - Duration::from_secs(1);
        hosts.insert(
            "old.hu".to_owned(),
            Arc::new(tokio::sync::Mutex::new(Some(long_ago))),
        );
        hosts.insert(
            "recent.hu".to_owned(),
            Arc::new(tokio::sync::Mutex::new(Some(std::time::Instant::now()))),
        );

        forget_idle(&mut hosts);
        assert!(!hosts.contains_key("old.hu"));
        assert!(hosts.contains_key("recent.hu"));
    }

    #[tokio::test]
    async fn a_host_being_talked_to_right_now_is_never_forgotten() {
        // Dropping it mid-request would let a second request through immediately —
        // exactly the crowding the gate exists to prevent.
        let mut hosts = HashMap::new();
        let slot = Arc::new(tokio::sync::Mutex::new(Some(
            std::time::Instant::now() - REMEMBER_HOST_FOR - Duration::from_secs(1),
        )));
        let _in_flight = Arc::clone(&slot).lock_owned().await;
        hosts.insert("busy.hu".to_owned(), slot);

        forget_idle(&mut hosts);
        assert!(hosts.contains_key("busy.hu"), "forgot a host mid-request");
    }

    #[test]
    fn a_host_we_have_not_asked_yet_is_kept() {
        let mut hosts = HashMap::new();
        hosts.insert("new.hu".to_owned(), Arc::new(tokio::sync::Mutex::new(None)));
        forget_idle(&mut hosts);
        assert!(hosts.contains_key("new.hu"));
    }

    #[tokio::test]
    async fn the_table_does_not_grow_without_end() {
        // The whole point: thirty thousand businesses must not leave thirty thousand
        // entries behind for the life of the server.
        let gate = HostGate::default();
        for n in 0..50 {
            let _turn = gate.enter(&format!("shop{n}.hu")).await;
        }
        let held = gate.hosts.lock().expect("not poisoned").len();
        assert_eq!(held, 50, "all still recent, so all still held");

        // Age them all, then ask about one more.
        {
            let hosts = gate.hosts.lock().expect("not poisoned");
            for slot in hosts.values() {
                let mut last = slot.try_lock().expect("nothing in flight");
                *last =
                    Some(std::time::Instant::now() - REMEMBER_HOST_FOR - Duration::from_secs(1));
            }
        }
        let _turn = gate.enter("one-more.hu").await;
        let after = gate.hosts.lock().expect("not poisoned").len();
        assert_eq!(after, 1, "the idle fifty should have gone");
    }
}
