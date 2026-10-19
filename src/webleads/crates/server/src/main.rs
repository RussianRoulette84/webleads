//! The server binary.
//!
//! It opens the database, reads the list of trades, serves the WebAssembly app and
//! answers three routes: the call list, the trades, and an import that takes one of
//! the scraper's CSV files all the way through to stored leads.
//!
//! # What this file is here to teach
//!
//! - `#[tokio::main]` is a macro that wraps `main` in an async runtime. Without it
//!   there is nothing to `await` on, because Rust has no runtime of its own.
//! - `anyhow::Result` at the edge of the program: inside a library we use typed
//!   errors, but `main` only needs to say *what failed* to a human, so a single
//!   catch-all type is the honest choice here.
//! - Shared state built once and handed to every request, rather than globals.
//! - A guard value that must stay alive. `tracing-appender` returns one; drop it and
//!   the log writer shuts down. Holding it in a variable until the end of `main` is
//!   the whole mechanism, and it is a good first look at why Rust cares so much
//!   about when values die.

mod error;
mod machine;
mod page;
mod places;
mod routes;
mod scrape;
mod sheet;
mod shot;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{
    filter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

/// Where the built WebAssembly app lands, relative to the **workspace** root. It is
/// built from the crates, so it moves with them.
const WASM_DIR: &str = "crates/ui/dist";

// The rest hang off the **repository** root, two levels above the workspace. They are
// everything the program writes rather than everything it is built from.
const LOG_DIR: &str = "logs";
const DB_PATH: &str = "data/webleads.db";
/// The hundred trades. Hand-written, not generated — `.gitignore` keeps this one file
/// out of the rule that skips the rest of `data/`.
const TARGETS_CSV: &str = "data/TARGETS.csv";

/// Pictures of broken websites, served at `/shots`.
const SHOTS_DIR: &str = "data/leads";
/// Bound on 0.0.0.0, not 127.0.0.1: the published port forwards to the container's
/// external interface, and a server listening only on loopback would never see it.
const PORT: u16 = 8100;

/// What gets written when `RUST_LOG` says nothing.
///
/// `scraper=debug` is what puts the scraper's own requests and answers in the file.
const DEFAULT_LOGGING: &str = "info,tower_http=debug,scraper=debug";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Both anchors first, before anything that writes: the logs take their folder
    // from them, not from wherever the server happened to be started.
    let Roots { workspace, repo } = roots()?;
    let log_dir = repo.join(LOG_DIR);

    // `_guard` rather than `_`: naming it keeps it alive until main returns. Called
    // `_guard` with the underscore so clippy knows we are not ignoring it by mistake.
    let (writer, _guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(&log_dir, "backend.log"));

    // What the browser reports goes to its own file. Mixed in with the server's own
    // troubles it would be lost; `logs/frontend.log` sat empty from the day it was
    // made, so a page that misbehaved left no trace at all.
    let (frontend, _frontend_guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(&log_dir, "frontend.log"));

    // What the scraper does gets its own file too. `logs/scraper.log` has been empty
    // since the day it was made, so a run that searched a hundred and thirty-seven
    // words left nothing behind saying what it asked for or what came back.
    let (scraper, _scraper_guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(&log_dir, "scraper.log"));

    // Four places at once: the file for the record, the terminal for watching, and a
    // file each for what the browser and the scraper tell us.
    tracing_subscriber::registry()
        // `scraper=debug` on purpose: every request to the scraper and every answer
        // it gives goes into the file, always. The terminal on the page throws them
        // away unless the detail box is ticked, so the cost is disk rather than
        // noise — and when a search misbehaves the record is already there rather
        // than something you have to reproduce with the logging turned up.
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| DEFAULT_LOGGING.into()))
        .with(
            fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                // Everything except what the browser and the scraper sent — those
                // have their own files.
                .with_filter(filter::filter_fn(|meta| {
                    meta.target() != "frontend" && meta.target() != "scraper"
                })),
        )
        .with(
            fmt::layer()
                .with_writer(frontend)
                .with_ansi(false)
                .with_filter(filter::filter_fn(|meta| meta.target() == "frontend")),
        )
        .with(
            fmt::layer()
                .with_writer(scraper)
                .with_ansi(false)
                .with_filter(filter::filter_fn(|meta| meta.target() == "scraper")),
        )
        .with(fmt::layer())
        .init();

    // Old log files are pruned the same way the backups are — the pruning already
    // existed, it simply was not used here, so one file a day piled up forever.
    if let Err(err) = webleads_store::prune_old_files(&log_dir, ".log.", 14) {
        tracing::warn!("⚠️ WARNING: could not tidy the old logs: {err}");
    }

    let wasm = workspace.join(WASM_DIR);
    tracing::info!(path = %wasm.display(), "serving the WebAssembly build");

    if !wasm.exists() {
        // Say so loudly rather than serving 404s and letting someone wonder why.
        tracing::warn!("nothing built yet — run `trunk build` in crates/ui first");
    }

    // Open the database. The folder has to exist first; SQLite will make the file
    // but not the directory above it.
    let db_path = repo.join(DB_PATH);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not make {}", parent.display()))?;
    }
    let store = webleads_store::Store::open(&db_path)
        .await
        .with_context(|| format!("could not open {}", db_path.display()))?;
    tracing::info!(path = %db_path.display(), "database ready");

    // Your clock, as you last chose it, before anything reads the time.
    let zone = store.your_zone().await?;
    match webleads_domain::set_your_zone(&zone) {
        Ok(zone) => tracing::info!(zone, "your clock"),
        Err(err) => tracing::warn!(
            "⚠️ WARNING: keeping {}: {err}",
            webleads_domain::your_zone_name()
        ),
    }

    // The trades, read now and re-read whenever they are edited on the page.
    let targets_path = repo.join(TARGETS_CSV);
    let targets = state::read_targets(&targets_path)
        .with_context(|| format!("could not read {}", targets_path.display()))?;
    tracing::info!(count = targets.len(), "trades loaded");

    let shared = Arc::new(state::AppState {
        store,
        targets: std::sync::RwLock::new(targets),
        targets_path,
        home: state::home(),
        searching: std::sync::RwLock::new(state::whole_city()),
        shots_dir: repo.join(SHOTS_DIR),
        http: webleads_sitecheck::clients().context("could not build the HTTP clients")?,
        jobs: state::Jobs::default(),
        log_dir: log_dir.clone(),
        stopping: std::sync::Mutex::new(std::collections::HashSet::new()),
        holding: std::sync::Mutex::new(std::collections::HashSet::new()),
    });

    // A run half done when we last stopped is picked up and carried on. That is the
    // whole reason it lives in the database rather than in memory: ninety minutes of
    // searching should survive a restart, not be thrown away by one.
    // Where we were searching when we last ran. Kept in the database, so a restart
    // does not quietly move the search back to the whole city.
    match shared.store.chosen_area().await {
        Ok(Some(chosen)) => {
            tracing::info!(area = %chosen.area.label, "searching here");
            shared.search_in(chosen.area);
        }
        Ok(None) => tracing::warn!("⚠️ WARNING: there are no areas to search"),
        Err(err) => tracing::warn!("⚠️ WARNING: could not read the chosen area: {err}"),
    }

    match shared.store.unfinished_scrape().await {
        Ok(Some(run)) => {
            tracing::info!(
                run_id = run.id,
                done = run.words_done,
                total = run.words_total,
                "picking up a run that was left half done"
            );
            let carrying_on = shared.clone();
            tokio::spawn(async move {
                // **Its own** settings, written down when the run was made. Before
                // that was stored, a resumed run silently fell back to the defaults
                // whatever it had actually been asked to do.
                let options = match carrying_on.store.run_options(run.id).await {
                    Ok(Some(kept)) => kept,
                    _ => webleads_domain::ScrapeOptions::around(&carrying_on.searching()),
                };
                scrape::work_through(carrying_on.clone(), run.id, options).await;
                scrape::work_through_queue(carrying_on).await;
            });
        }
        // Nothing half done, but something may be lined up from before the restart.
        Ok(None) => {
            let taking = shared.clone();
            tokio::spawn(async move { scrape::work_through_queue(taking).await });
        }
        Err(err) => tracing::warn!("⚠️ WARNING: could not look for an unfinished run: {err}"),
    }

    // Our routes first; anything else falls through to the built web app. Order
    // matters — a fallback that came first would swallow /api/leads.
    let app = Router::new()
        .merge(routes::router(shared))
        // The pictures. Its own folder rather than the built page's, so a rebuild
        // of the page cannot sweep them away.
        .nest_service("/shots", ServeDir::new(repo.join(SHOTS_DIR)))
        .fallback_service(ServeDir::new(&wasm).append_index_html_on_directories(true))
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("could not take port {PORT}"))?;

    tracing::info!(%addr, "webleads is up");
    axum::serve(listener, app)
        .await
        .context("the server stopped unexpectedly")?;
    Ok(())
}

/// The two directories every path in the program is measured from.
///
/// There are two because the project keeps what it is *built from* and what it
/// *writes* in different places: the crates live in `src/webleads/`, everything
/// generated lives in `data/` and `logs/` two levels above that.
struct Roots {
    /// `src/webleads/` — the built page.
    workspace: PathBuf,
    /// The repository root — the database, the logs, the screenshots, the trades.
    repo: PathBuf,
}

/// Where the compiler put this crate.
///
/// A constant, not a decision, which is why no decision lives in it — every "how far
/// up" question belongs to the two functions below, each of which can be handed a
/// made-up path in a test. Note this is filled in **when the program is built**, not
/// when it runs: move the repository and the old binary will confidently keep looking
/// at the old place. Rebuild after moving.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The workspace root, given the server crate's own directory.
///
/// Pure: two steps up and an honest error if there is nowhere to go.
fn workspace_root_from(server_manifest_dir: &std::path::Path) -> anyhow::Result<PathBuf> {
    server_manifest_dir
        .parent() // crates/
        .and_then(std::path::Path::parent) // the workspace
        .map(std::path::Path::to_path_buf)
        .with_context(|| {
            format!(
                "{} is not two levels inside a workspace",
                server_manifest_dir.display()
            )
        })
}

/// The repository root, given the workspace root — two steps further up, because the
/// workspace sits at `src/webleads/`.
fn repo_root_from(workspace_root: &std::path::Path) -> anyhow::Result<PathBuf> {
    workspace_root
        .parent() // src/
        .and_then(std::path::Path::parent) // the repository
        .map(std::path::Path::to_path_buf)
        .with_context(|| {
            format!(
                "{} is not two levels inside a repository",
                workspace_root.display()
            )
        })
}

/// Both anchors, worked out once. The only part that cannot be unit tested, because
/// it is the one line that reads the compile-time constant.
fn roots() -> anyhow::Result<Roots> {
    let workspace = workspace_root_from(&manifest_dir())?;
    let repo = repo_root_from(&workspace)?;
    Ok(Roots { workspace, repo })
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
