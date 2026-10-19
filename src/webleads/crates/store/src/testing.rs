//! Helpers the tests in every module of this crate share.

use super::*;
use webleads_domain::{Tls, Verdict};

/// Start a search in the area the schema arrives with.
///
/// Use this rather than [`Store::begin_run`] wherever a test records businesses. A
/// bare run has no place, so what it records has no city — and without a city there
/// is no country, and without a country a telephone number written `06 1 234 5001`
/// cannot be read as Hungarian. The real program always searches somewhere.
pub(crate) async fn a_search(store: &Store) -> i64 {
    let area = store
        .areas()
        .await
        .expect("the schema arrives with one")
        .remove(0);
    store
        .begin_scrape(
            &area.area,
            &[("a test search".to_owned(), None)],
            &webleads_domain::ScrapeOptions::around(&area.area),
        )
        .await
        .expect("starts")
}

pub(crate) fn a_lead(id: &str, reviews: u32, distance_m: u32) -> Lead {
    Lead {
        id: id.to_owned(),
        name: format!("Shop {id}"),
        phone: Some("+36 1 111 1111".to_owned()),
        category: Some("Barber shop".to_owned()),
        trade: None,
        hours: Default::default(),
        address: Some("Hősök tere 1".to_owned()),
        maps_url: None,
        website: None,
        social_url: None,
        verdict: Verdict::NoSite,
        reason: "no link on Google Maps".to_owned(),
        reviews,
        rating: None,
        distance_m,
        position: Some((47.5149, 19.0778)),
        tls: Tls::Unknown,
    }
}

/// A lead paired with a position in Zugló, which is all these tests need.
pub(crate) fn found(lead: Lead) -> Found {
    Found {
        lead,
        lat: 47.5149,
        lon: 19.0778,
        freshly_checked: true,
        found_by: None,
        open_hours: None,
    }
}

/// Three businesses on the list, one of each state that stays on it.
pub(crate) async fn a_mixed_list() -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    // Each one distinct in every searchable field, or a search test cannot tell
    // whether it found the right business or simply all of them.
    let mut first = a_lead("fresh", 0, 10);
    first.name = "Borzas Barber".into();
    first.address = Some("Hősök tere 1".into());
    first.category = Some("Barber shop".into());

    let mut second = a_lead("rung", 1, 20);
    second.name = "Petra'z Hair".into();
    second.address = Some("Adria sétány 4".into());
    second.category = Some("Hair salon".into());

    let mut third = a_lead("refused", 2, 30);
    third.name = "Zuglói Fodrászat".into();
    third.address = Some("Kerepesi út 90".into());
    third.category = Some("Hairdresser".into());

    store
        .record(run, &[found(first), found(second), found(third)])
        .await
        .expect("records");
    store
        .set_status("rung", webleads_domain::Status::NotReached)
        .await
        .expect("sets");
    store
        .set_status("refused", webleads_domain::Status::NotInterested)
        .await
        .expect("sets");
    store
}
