//! The places you can search: keeping them, choosing one, and looking one up.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use webleads_domain::Area;
use webleads_store::InCity;

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

/// The areas, and which one the next search will use.
#[derive(Debug, Serialize)]
pub struct Areas {
    pub areas: Vec<webleads_store::SavedArea>,
    /// The number of the chosen one, so the page does not have to guess.
    pub chosen: Option<i64>,
}

/// A new area, from a place you looked up.
#[derive(Debug, Deserialize)]
pub struct NewArea {
    pub label: String,
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    /// Which city it belongs to. A name, because the usual answer is one we already
    /// keep — Zugló and Óbuda are both Budapest — and the page offers what the
    /// address lookup reported rather than making you pick from a list.
    #[serde(default)]
    pub city: Option<String>,
    /// Its country, for a city we have not seen before. Two letters, as the lookup
    /// reports them: it is what decides the clock and the search language.
    #[serde(default)]
    pub country_code: Option<String>,
}

/// An address to look up.
#[derive(Debug, Deserialize)]
pub struct Asking {
    #[serde(default)]
    pub q: String,
}

/// `GET /api/areas` — everywhere you can search.
pub async fn areas(State(state): State<SharedState>) -> ApiResult<Json<Areas>> {
    Ok(Json(Areas {
        areas: state.store.areas().await?,
        chosen: state.store.chosen_area().await?.map(|a| a.id),
    }))
}

/// `POST /api/areas` — keep a new area.
pub async fn add(
    State(state): State<SharedState>,
    Json(asked): Json<NewArea>,
) -> ApiResult<Json<webleads_store::SavedArea>> {
    let area = Area {
        label: asked.label.trim().to_owned(),
        address: asked.address.clone(),
        lat: asked.lat,
        lon: asked.lon,
        // Zero means "no fence", the same as everywhere else on the page.
        max_results: crate::state::MAX_RESULTS,
    };

    // Which city this area is in. A name we already keep is simply used; one we have
    // not seen becomes a city, in the same step as the area. An area with no city
    // named keeps none — it is still searchable, and guessing would file its leads
    // under the wrong town for ever.
    let id = match city_for(&state, &asked).await? {
        Settled::NoCity => state.store.add_area_in(&area, InCity::None).await?,
        Settled::Known(city) => state.store.add_area_in(&area, InCity::Known(city)).await?,
        Settled::New(city) => {
            let id = state
                .store
                .add_area_in(
                    &area,
                    InCity::New {
                        city: &city,
                        centre_label: &asked.address,
                    },
                )
                .await?;
            tracing::info!(city = %city.name, zone = %city.zone, lang = %city.lang, "new city");
            id
        }
    };

    state
        .store
        .area(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("the area was not written".into()))
}

/// The city a new area goes into, before anything is written.
enum Settled {
    NoCity,
    Known(i64),
    New(webleads_domain::City),
}

/// Which city this new area belongs to, worked out but not yet kept.
async fn city_for(state: &SharedState, asked: &NewArea) -> ApiResult<Settled> {
    let Some(name) = asked
        .city
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    else {
        return Ok(Settled::NoCity);
    };
    if let Some(known) = state.store.city_named(name).await? {
        return Ok(Settled::Known(known.id));
    }

    // A city we have not seen. Its clock is read off the map where it is, and its
    // search language follows its country.
    let code = asked
        .country_code
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            ApiError::BadRequest(format!("{name} is a new city, so it needs a country"))
        })?;
    let zone = crate::places::zone_at(asked.lat, asked.lon).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "no clock is kept at {}, {} — is that on land?",
            asked.lat, asked.lon
        ))
    })?;

    Ok(Settled::New(webleads_domain::City {
        name: name.to_owned(),
        country_code: code.to_lowercase(),
        zone,
        lang: webleads_domain::lang_for(code).to_owned(),
        // Its own centre, until anybody says otherwise: the first area added in a
        // city is the only thing we know about where it is.
        centre_lat: asked.lat,
        centre_lon: asked.lon,
    }))
}

/// `GET /api/cities` — every city, for the chips at the top of the filters.
pub async fn cities(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<webleads_store::SavedCity>>> {
    Ok(Json(state.store.cities().await?))
}

/// A new spot to measure a city from, as the address box found it.
#[derive(Debug, Deserialize)]
pub struct NewCentre {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

/// What moving a centre came to.
#[derive(Debug, Serialize)]
pub struct MovedCentre {
    pub city: webleads_store::SavedCity,
    /// How many leads came out with a different distance.
    pub moved: usize,
}

/// `POST /api/cities/:id/centre` — measure this city from somewhere else.
pub async fn move_centre(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Json(asked): Json<NewCentre>,
) -> ApiResult<Json<MovedCentre>> {
    let (city, counted) = state
        .store
        .move_centre(id, &asked.label, asked.lat, asked.lon)
        .await?;
    tracing::info!(
        city = %city.city.name,
        from = ?city.centre_label,
        moved = counted.moved,
        "centre moved"
    );
    Ok(Json(MovedCentre {
        city,
        moved: counted.moved,
    }))
}

/// `POST /api/areas/:id/choose` — search here next.
pub async fn choose(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<webleads_store::SavedArea>> {
    if !state.store.choose_area(id).await? {
        return Err(ApiError::NotFound(format!("there is no area {id}")));
    }
    let chosen = state
        .store
        .chosen_area()
        .await?
        .ok_or_else(|| ApiError::NotFound("the area disappeared".into()))?;
    // Kept in memory too, so every request does not have to ask the database where
    // it is searching.
    state.search_in(chosen.area.clone());
    Ok(Json(chosen))
}

/// `DELETE /api/areas/:id` — forget a place. The leads it found stay.
///
/// Any of them, including the one being searched: whatever is left becomes the place
/// we search, and a run already going carries its own area with it.
pub async fn remove(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<bool>> {
    if !state.store.remove_area(id).await? {
        return Err(ApiError::NotFound(format!("there is no area {id}")));
    }
    // Whatever stands in for it now becomes the place we search.
    if let Some(chosen) = state.store.chosen_area().await? {
        state.search_in(chosen.area);
    }
    Ok(Json(true))
}

/// `GET /api/places?q=` — turn a typed address into coordinates to choose from.
pub async fn look_up(
    State(state): State<SharedState>,
    Query(asking): Query<Asking>,
) -> ApiResult<Json<Vec<crate::places::Found>>> {
    // One request a second, which is what their rules ask. The same gate the website
    // checking uses, so politeness is one rule in one place rather than two.
    let _turn = state.http.wait_turn("nominatim.openstreetmap.org").await;
    tokio::time::sleep(crate::places::GAP).await;

    // Named in the language you search in, because the name is appended to every
    // search word: "Bécs" is no use to an English search of Vienna.
    let lang = state.store.search_lang().await?;
    let found = crate::places::look_up(&state.http.strict, &asking.q, &lang)
        .await
        .map_err(ApiError::BadRequest)?;
    Ok(Json(found))
}
