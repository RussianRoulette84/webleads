//! Tests for [`crate::home`].

use super::*;

fn budapest(label: Option<&str>) -> CityCentre {
    CityCentre {
        id: 1,
        name: "Budapest".to_owned(),
        centre_lat: 47.5149,
        centre_lon: 19.0778,
        centre_label: label.map(str::to_owned),
    }
}

#[test]
fn a_named_starting_point_is_shown_by_name() {
    assert_eq!(
        starting_point(Some(&budapest(Some(" Hősök tere, Budapest ")))),
        "Hősök tere, Budapest"
    );
}

#[test]
fn an_unnamed_starting_point_is_the_centre_of_the_city() {
    for label in [None, Some(""), Some("   ")] {
        assert_eq!(
            starting_point(Some(&budapest(label))),
            "the centre of Budapest"
        );
    }
}

#[test]
fn no_city_has_no_starting_point_to_show() {
    assert_eq!(starting_point(None), "");
}

fn looked_up(label: &str, city: Option<&str>) -> Place {
    Place {
        label: label.to_owned(),
        lat: 47.5,
        lon: 19.05,
        city: city.map(str::to_owned),
        country: Some("Magyarország".to_owned()),
        country_code: Some("hu".to_owned()),
    }
}

#[test]
fn a_named_place_keeps_its_name_and_gains_its_town() {
    let place = looked_up(
        "Keleti pályaudvar, 2-4, Kerepesi út, VIII. kerület, Budapest, 1087, Magyarország",
        Some("Budapest"),
    );
    assert_eq!(short_centre(&place), "Keleti pályaudvar, Budapest");
}

#[test]
fn a_house_number_goes_after_its_street() {
    let place = looked_up(
        "12, Andrássy út, VI. kerület, Budapest, 1061, Magyarország",
        Some("Budapest"),
    );
    assert_eq!(short_centre(&place), "Andrássy út 12, Budapest");
}

#[test]
fn a_town_already_in_the_name_is_not_said_twice() {
    let place = looked_up(
        "Budapest, Közép-Magyarország, Magyarország",
        Some("Budapest"),
    );
    assert_eq!(short_centre(&place), "Budapest");
}

#[test]
fn a_place_with_no_town_is_just_its_name() {
    assert_eq!(
        short_centre(&looked_up("Hősök tere, Budapest", None)),
        "Hősök tere"
    );
}
