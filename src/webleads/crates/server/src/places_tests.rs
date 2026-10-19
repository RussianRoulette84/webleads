//! Tests for [`crate::places`] — reading the address lookup's answer.

use super::*;

/// A real reply, trimmed to the fields we read. Kept verbatim so their shape is
/// checked rather than assumed — reading `lat` as a number rather than as text is
/// exactly the mistake that would silently return nothing.
const A_REAL_ANSWER: &str = r#"[
  {"place_id":1,"lat":"47.5142600","lon":"19.1064700",
   "display_name":"Zugló, Budapest, 1145, Magyarország","type":"suburb"},
  {"place_id":2,"lat":"47.5400000","lon":"19.0400000",
   "display_name":"Óbuda, Budapest, 1033, Magyarország","type":"suburb"}
]"#;

#[test]
fn a_real_answer_reads_as_places() {
    let found = read_answer(A_REAL_ANSWER);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].label, "Zugló, Budapest, 1145, Magyarország");
    assert!((found[0].lat - 47.51426).abs() < 0.000_01);
    assert!((found[0].lon - 19.10647).abs() < 0.000_01);
}

/// A real Dubai reply, asked for in English with the address details on. Verbatim
/// from their service, because the whole point of these fields is that they are
/// somebody else's shape and they come and go.
const A_REAL_DUBAI_ANSWER: &str = r#"[
  {"place_id":3,"lat":"25.1948428","lon":"55.2781950",
   "display_name":"Downtown Dubai, Dubai, Dubai Emirate, United Arab Emirates",
   "address":{"neighbourhood":"Downtown Dubai","city":"Dubai",
              "state":"Dubai Emirate","country":"United Arab Emirates",
              "country_code":"ae"}}
]"#;

#[test]
fn a_foreign_answer_carries_its_city_and_country() {
    // This is what lets adding an area offer the city rather than ask for it.
    let found = read_answer(A_REAL_DUBAI_ANSWER);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].city.as_deref(), Some("Dubai"));
    assert_eq!(found[0].country.as_deref(), Some("United Arab Emirates"));
    assert_eq!(found[0].country_code.as_deref(), Some("ae"));
}

#[test]
fn an_answer_without_the_details_still_reads_as_a_place() {
    // The older shape, with no `address` at all. It must not stop the lot being read
    // — a place with no city offered is still a place you can search.
    let found = read_answer(A_REAL_ANSWER);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].city, None);
    assert_eq!(found[0].country_code, None);
}

#[test]
fn a_town_or_a_village_counts_as_the_city() {
    // Their answer names the settlement by its size. Taking only `city` would offer
    // nothing for most of the countryside.
    let town = r#"[{"display_name":"A market town","lat":"47.1","lon":"19.1",
      "address":{"town":"Szentendre","country_code":"hu"}}]"#;
    assert_eq!(read_answer(town)[0].city.as_deref(), Some("Szentendre"));

    let village = r#"[{"display_name":"A hamlet","lat":"47.2","lon":"19.2",
      "address":{"village":"Nagymaros","country_code":"hu"}}]"#;
    assert_eq!(read_answer(village)[0].city.as_deref(), Some("Nagymaros"));

    // Nothing but a region: better to offer that than nothing.
    let region = r#"[{"display_name":"Somewhere","lat":"47.3","lon":"19.3",
      "address":{"state":"Dubai Emirate","country_code":"ae"}}]"#;
    assert_eq!(
        read_answer(region)[0].city.as_deref(),
        Some("Dubai Emirate")
    );
}

#[test]
fn a_blank_city_is_no_city_rather_than_an_empty_name() {
    let blank = r#"[{"display_name":"Somewhere","lat":"47.4","lon":"19.4",
      "address":{"city":"   ","town":"Vác","country_code":"hu"}}]"#;
    assert_eq!(read_answer(blank)[0].city.as_deref(), Some("Vác"));
}

#[test]
fn a_country_code_is_kept_in_lower_case_and_only_if_it_is_two_letters() {
    // Our own tables are keyed by the two lower-case letters, so anything else is
    // worse than nothing: it would silently match no country at all.
    let shouting = r#"[{"display_name":"X","lat":"1.0","lon":"2.0",
      "address":{"country_code":"AE"}}]"#;
    assert_eq!(read_answer(shouting)[0].country_code.as_deref(), Some("ae"));

    let odd = r#"[{"display_name":"X","lat":"1.0","lon":"2.0",
      "address":{"country_code":"unknown"}}]"#;
    assert_eq!(read_answer(odd)[0].country_code, None);
}

#[test]
fn an_answer_with_nothing_in_it_is_no_places_rather_than_a_failure() {
    assert!(read_answer("[]").is_empty());
}

#[test]
fn something_that_is_not_their_answer_at_all_is_no_places() {
    // Their service answering with an error page must not take the server down.
    assert!(read_answer("<html>down for maintenance</html>").is_empty());
    assert!(read_answer("").is_empty());
}

#[test]
fn one_unreadable_match_does_not_lose_the_good_ones() {
    let mixed = r#"[
      {"display_name":"Fine","lat":"47.5","lon":"19.1"},
      {"display_name":"Broken","lat":"not a number","lon":"19.1"}
    ]"#;
    let found = read_answer(mixed);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].label, "Fine");
}

#[test]
fn a_blank_address_is_refused_before_we_ask_anybody() {
    // A blank lookup asks them for everything there is, which is how a tool gets
    // blocked.
    assert!(question("   ").is_err());
    assert!(question("").is_err());
    assert!(question("a").is_err());
}

#[test]
fn a_real_address_is_passed_through_trimmed() {
    assert_eq!(question("  Hősök tere  ").as_deref(), Ok("Hősök tere"));
}

#[test]
fn we_say_who_we_are_and_how_often_we_ask() {
    // Their two rules, both checked here so neither can be quietly dropped.
    assert!(AGENT.contains("webleads"));
    assert!(GAP >= std::time::Duration::from_secs(1));
}

#[test]
fn we_ask_about_the_whole_world_and_not_one_country() {
    // A country fence here would make everywhere but one country unsearchable:
    // "Dubai" would come back as a Dubai Supermarket on Rákóczi út. A test, because it
    // is one line and easy to add by accident.
    let asked = lookup_params("Downtown Dubai");
    assert!(
        !asked.iter().any(|(key, _)| *key == "countrycodes"),
        "the address lookup has been fenced to one country again"
    );
}

#[test]
fn a_hungarian_search_asks_for_hungarian_names_first() {
    // The name that comes back is appended to every search word, and asking in
    // English turned Zugló into "14th district", which Google Maps has never heard of.
    assert_eq!(readable_in("hu"), "hu,en");
}

#[test]
fn any_other_search_asks_for_english_names() {
    // A Hungarian name for Vienna — "Bécs" — finds nothing in an English search.
    assert_eq!(readable_in("en"), "en");
    assert_eq!(readable_in(""), "en");
}

#[test]
fn a_spot_on_the_map_knows_its_clock() {
    assert_eq!(zone_at(51.5074, -0.1278).as_deref(), Some("Europe/London"));
    assert_eq!(
        zone_at(47.5149, 19.0778).as_deref(),
        Some("Europe/Budapest")
    );
    assert_eq!(zone_at(25.1948, 55.2782).as_deref(), Some("Asia/Dubai"));
    assert_eq!(
        zone_at(40.7128, -74.0060).as_deref(),
        Some("America/New_York")
    );
}

#[test]
fn we_ask_for_the_city_and_the_country_of_each_match() {
    let asked = lookup_params("Downtown Dubai");
    assert!(asked.contains(&("addressdetails", "1".to_owned())));
    assert!(asked.contains(&("q", "Downtown Dubai".to_owned())));
    // As many as the screen offers, and no more.
    assert!(asked.contains(&("limit", MATCHES.to_string())));
}
