//! Measure every lead's distance from its own city's centre.
//!
//! # Why this exists
//!
//! Distance was worked out once, when a business was first seen, from one fixed street
//! in Zugló — whichever area the search was in. That read fine while there was one
//! city. With two it puts every Dubai lead four thousand kilometres away, and the
//! "within" slider and "nearest first" stop meaning anything outside Budapest.
//!
//! From now on a search measures from the centre of its own city. This goes back over
//! the rows already stored and does the same to them.
//!
//! **Nothing on today's screen should move.** Budapest's centre is the street the old
//! number was measured from, so every Budapest row comes out identical. That is the
//! point: the column means one thing everywhere afterwards rather than two things
//! depending on when it was written.
//!
//! A one-off, run by hand:
//!
//! ```text
//! cargo run -p webleads-server --bin remeasure
//! ```

use std::path::PathBuf;

use webleads_store::Store;

/// The same database the server uses.
const DB_PATH: &str = "data/webleads.db";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Four levels: crates/server → crates → the workspace → src → the repository.
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .map(PathBuf::from)
        .expect("this crate is four levels inside the repository");

    let db = repo.join(DB_PATH);
    if !db.exists() {
        anyhow::bail!("no database at {}", db.display());
    }
    let store = Store::open(&db).await?;

    println!("\x1b[1;35m=== measuring every lead from its own city ===\x1b[0m");
    for city in store.cities().await? {
        let counted = store.remeasure_city(city.id, &city.city).await?;
        println!(
            "{:>7} leads  \x1b[2mfrom the middle of\x1b[0m {} \x1b[2m({}, {})\x1b[0m",
            counted.considered, city.city.name, city.city.centre_lat, city.city.centre_lon
        );
        if counted.moved == 0 {
            println!("        \x1b[2mnot one number changed, which is what we expected\x1b[0m");
        } else {
            println!(
                "        \x1b[1;33m{} changed\x1b[0m — the furthest by {} m",
                counted.moved, counted.biggest_change_m
            );
        }
    }

    // Anything with no city cannot be measured against one, and guessing which town
    // it was in would be inventing data. Said plainly rather than left silent.
    let orphans = store.leads_with_no_city().await?;
    if orphans > 0 {
        println!(
            "\x1b[1;33m{orphans} leads belong to no city and were left exactly as they were\x1b[0m"
        );
    }

    println!("\x1b[1;35m=== done ===\x1b[0m");
    Ok(())
}
