//! Getting the list out of the page and into a spreadsheet.

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use webleads_store::StoredLead;

use crate::error::{ApiError, ApiResult};
use crate::routes::leads::Paging;
use crate::state::SharedState;

/// The most rows one download will hold.
///
/// High enough that it is never reached in practice, low enough that a mistake cannot
/// ask the database to build a spreadsheet of everything at once.
const MAX_ROWS: i64 = 10_000;

/// `GET /api/export?status=&search=` — the current list as a spreadsheet.
///
/// Whatever the page is showing, with the filter and the search applied — not
/// everything, and not just the page you can see.
pub async fn export(
    State(state): State<SharedState>,
    Query(paging): Query<Paging>,
) -> ApiResult<Response> {
    // The filter and the search come from the page; the paging does not. You want
    // everything that matches, not the fifty on screen.
    let want = webleads_store::Wanted {
        limit: MAX_ROWS,
        offset: 0,
        ..paging.wanted()
    };

    let leads = state.store.leads(&want).await?;
    let body = to_csv(&leads).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let name = file_name(&want);

    tracing::info!(rows = leads.len(), "exported the call list");

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// `GET /api/export.xlsx?status=&search=` — the same list as a real spreadsheet.
///
/// The CSV stays: it opens anywhere and it is what a script reads. This one is for
/// working in — numbers sort as numbers, the headings stay put as you scroll, and it
/// carries the pitch line and your notes.
pub async fn export_xlsx(
    State(state): State<SharedState>,
    Query(paging): Query<Paging>,
) -> ApiResult<Response> {
    let want = webleads_store::Wanted {
        limit: MAX_ROWS,
        offset: 0,
        ..paging.wanted()
    };

    let leads = state.store.leads(&want).await?;
    let body = crate::sheet::to_xlsx(&leads).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let name = crate::sheet::file_name(&want);

    tracing::info!(
        rows = leads.len(),
        "exported the call list as a spreadsheet"
    );

    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_owned(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// `GET /api/export.html?status=&search=` — the list as one page you can keep.
///
/// Everything is inside the file: the leads, the style, the map and its pins. Opened
/// from a folder with our server switched off, it still searches, filters, sorts and
/// draws the map.
pub async fn export_page(
    State(state): State<SharedState>,
    Query(paging): Query<Paging>,
) -> ApiResult<Response> {
    let want = webleads_store::Wanted {
        limit: MAX_ROWS,
        offset: 0,
        ..paging.wanted()
    };

    let leads = state.store.leads(&want).await?;
    let heading = crate::page::Heading {
        area: state.searching().label.clone(),
        // The map opens on home, which is where you are reading the page from.
        centre: (state.home.lat, state.home.lon),
        made_on: webleads_domain::today_here_date(),
    };
    let body = crate::page::build(&leads, &heading);
    let name = file_name(&want).replace(".csv", ".html");

    tracing::info!(rows = leads.len(), "exported the call list as a page");

    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// Turn the list into a spreadsheet.
///
/// Pure, so the columns and their contents can be checked without a database or a
/// browser anywhere near it.
pub fn to_csv(leads: &[StoredLead]) -> Result<String, csv::Error> {
    let mut writer = csv::Writer::from_writer(Vec::new());

    writer.write_record([
        "Name",
        "Phone",
        "Why",
        "Detail",
        "Certificate",
        "Trade",
        "Address",
        "Reviews",
        "Distance (m)",
        "Open today",
        "Opening hours",
        "Status",
        "No answers",
        "Ring back",
        "Calls",
        "Last called",
        "Notes",
        "First seen",
        "Maps",
    ])?;

    for row in leads {
        let lead = &row.lead;
        writer.write_record([
            lead.name.as_str(),
            lead.phone.as_deref().unwrap_or(""),
            lead.verdict.describe(),
            lead.reason.as_str(),
            lead.tls.describe(),
            lead.shown_trade().unwrap_or(""),
            lead.address.as_deref().unwrap_or(""),
            &lead.reviews.to_string(),
            &lead.distance_m.to_string(),
            row.hours_today.as_deref().unwrap_or(""),
            // The whole week. A file read tomorrow morning is no use with only today
            // in it.
            &lead.hours.week_line().unwrap_or_default(),
            row.contact.status.describe(),
            &row.contact.not_reached_counter.to_string(),
            row.contact.follow_up_tstamp.as_deref().unwrap_or(""),
            &row.call_count.to_string(),
            row.contact.called_on.as_deref().unwrap_or(""),
            row.contact.note.as_str(),
            row.first_seen.as_str(),
            lead.maps_url.as_deref().unwrap_or(""),
        ])?;
    }

    let bytes = writer.into_inner().map_err(|e| e.into_error())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// What the downloaded file is called.
///
/// The filter is in the name, so three downloads in a row do not all land in your
/// downloads folder as `export (2).csv`.
pub fn file_name(want: &webleads_store::Wanted) -> String {
    let mut parts = vec!["webleads".to_owned()];
    if let Some(status) = want.status {
        parts.push(status.as_str().to_owned());
    }
    if want.search_pattern().is_some() {
        parts.push("search".to_owned());
    }
    format!("{}.csv", parts.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use webleads_domain::{Contact, Lead, Status, Tls, Verdict};

    fn a_row() -> StoredLead {
        StoredLead {
            lead: Lead {
                id: "a".into(),
                name: "Borzas Barber".into(),
                phone: Some("+36 1 234 5001".into()),
                category: Some("Barber shop".into()),
                trade: Some("Barbers".into()),
                hours: Default::default(),
                address: Some("Hősök tere 1".into()),
                maps_url: Some("https://maps.google.com/?cid=1".into()),
                website: None,
                social_url: None,
                verdict: Verdict::NoSite,
                reason: "no link on Google Maps".into(),
                reviews: 7,
                rating: Some(4.5),
                distance_m: 696,
                position: Some((47.5149, 19.0778)),
                tls: Tls::Unknown,
            },
            first_seen: "2026-09-19".into(),
            last_seen: "2026-09-19".into(),
            contact: Contact {
                status: Status::FollowUp,
                note: "ask for the owner".into(),
                called_on: Some("2026-09-19".into()),
                not_reached_counter: 1,
                follow_up_tstamp: Some("2026-09-21 14:30".into()),
            },
            call_count: 2,
            hours_today: Some("10:00–20:00".into()),
            hours_there: None,
            city_lang: None,
        }
    }

    #[test]
    fn the_spreadsheet_has_a_heading_row_and_one_row_per_business() {
        let csv = to_csv(&[a_row(), a_row()]).expect("writes");
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.starts_with("Name,Phone,Why"));
    }

    #[test]
    fn everything_worth_having_is_in_it() {
        let csv = to_csv(&[a_row()]).expect("writes");
        for expected in [
            "Borzas Barber",
            "+36 1 234 5001",
            "no website",
            "no link on Google Maps",
            // The trade we searched for wins over Google's guess.
            "Barbers",
            "696",
            "called",
            "ask for the owner",
            "2026-09-19",
        ] {
            assert!(csv.contains(expected), "`{expected}` is missing");
        }
    }

    #[test]
    fn a_business_with_nothing_filled_in_still_writes_a_row() {
        let mut bare = a_row();
        bare.lead.phone = None;
        bare.lead.category = None;
        bare.lead.address = None;
        bare.lead.maps_url = None;
        bare.contact.called_on = None;

        let csv = to_csv(&[bare]).expect("writes");
        assert_eq!(csv.lines().count(), 2);
        // Empty columns, not missing ones — a short row would shift everything along.
        // Nineteen columns, so eighteen commas.
        assert_eq!(csv.lines().nth(1).expect("row").matches(',').count(), 18);
    }

    #[test]
    fn a_note_with_a_comma_in_it_does_not_break_the_columns() {
        let mut awkward = a_row();
        awkward.contact.note = "call back, ask for Péter, not the boy".into();
        let csv = to_csv(&[awkward]).expect("writes");
        // Quoted rather than splitting the row into extra columns.
        assert!(csv.contains('"'), "the note was not quoted");
        assert_eq!(csv.lines().count(), 2);
    }

    #[test]
    fn an_empty_list_still_gives_a_usable_file() {
        let csv = to_csv(&[]).expect("writes");
        assert_eq!(csv.lines().count(), 1, "the headings should still be there");
    }

    #[test]
    fn the_file_is_named_after_what_you_filtered() {
        assert_eq!(
            file_name(&webleads_store::Wanted::default()),
            "webleads.csv"
        );
        assert_eq!(
            file_name(&webleads_store::Wanted {
                status: Some(Status::FollowUp),
                ..Default::default()
            }),
            "webleads-follow_up.csv"
        );
        assert_eq!(
            file_name(&webleads_store::Wanted {
                status: Some(Status::Sold),
                search: Some("borzas".into()),
                ..Default::default()
            }),
            "webleads-sold-search.csv"
        );
    }

    #[test]
    fn an_empty_search_does_not_end_up_in_the_name() {
        assert_eq!(
            file_name(&webleads_store::Wanted {
                search: Some("   ".into()),
                ..Default::default()
            }),
            "webleads.csv"
        );
    }
}
