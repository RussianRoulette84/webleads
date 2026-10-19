//! Make one search out of each city's many.
//!
//! # Why this exists
//!
//! A city searched on six evenings reads as six searches, and each one holds only
//! what it happened to see first — so none of them is "the leads in Budapest", and
//! pressing any one of them shows a sixth of the city. Afterwards there is one
//! search per city, wearing the city's name and holding everything those evenings
//! found between them.
//!
//! A one-off, run by hand:
//!
//! ```text
//! cargo run -p webleads-server --bin gather-searches
//! ```
//!
//! Every business stays on the call list, each keeps the newest verdict recorded for
//! it, and each word keeps its best evening. The separate dates do not survive, so
//! the database is worth copying first.

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

    let before = store.past_scrapes(500).await?.len();
    let gathered = store.gather_searches().await?;

    println!("\x1b[1;35m=== gathering ===\x1b[0m");
    for city in &gathered {
        println!(
            "  \x1b[1;32m{}\x1b[0m — {} searches are now one (#{})",
            city.city, city.searches, city.kept
        );
    }
    if gathered.is_empty() {
        println!("  nothing to gather: no city has more than one finished search");
    }
    println!(
        "\x1b[2m{before} searches before, {} after\x1b[0m",
        store.past_scrapes(500).await?.len()
    );
    Ok(())
}
