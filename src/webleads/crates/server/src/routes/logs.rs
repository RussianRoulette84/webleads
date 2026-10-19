//! The server's own log, handed to the page a few lines at a time.
//!
//! # What this file is here to teach
//!
//! Serving a file's *tail* without ever taking a path from the caller. The obvious
//! version takes a filename off the request, and that is how somebody reads
//! `../../.env`. Here the caller chooses between two words we wrote down, and the
//! path is built from a constant.

use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiResult;
use crate::state::SharedState;

/// The most lines one answer will carry.
///
/// A busy run writes a few dozen a second; past this the page is being handed more
/// than it can show and the rest is scrollback nobody will ever read.
const MOST_LINES: usize = 400;

/// And the most bytes we will read in one go, however far behind the page is.
const MOST_BYTES: u64 = 256 * 1024;

/// What a page that has just opened is shown of what happened before it.
///
/// **Nothing.** Old lines would scroll past as though they were happening now — a
/// warning from an hour ago reads as a search failing in front of you. A terminal
/// shows what happens next; the log file is there for what happened before.
const FIRST_LOOK: usize = 0;

/// Which log. A word from a list we wrote, never a path from the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Everything the web server itself says.
    Backend,
    /// Everything the searching says.
    Scraper,
}

impl Which {
    /// The name the files start with. `tracing-appender` adds `.2026-09-20` to it.
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Backend => "backend.log",
            Self::Scraper => "scraper.log",
        }
    }
}

/// Where each log was read up to last time.
#[derive(Debug, Default, Deserialize)]
pub struct Since {
    /// Byte offsets the page hands back. Absent means "I have just opened".
    pub backend: Option<u64>,
    pub scraper: Option<u64>,
    /// Include the noisy lines — every request the web server handles, and the
    /// word-by-word detail. Off unless you ask.
    #[serde(default)]
    pub debug: bool,
}

/// One log's new lines, and where to carry on from.
#[derive(Debug, Default, Serialize)]
pub struct Part {
    /// Hand this back next time.
    pub at: u64,
    pub lines: Vec<String>,
    /// True when the log ran ahead of us and we jumped to the end.
    #[serde(default)]
    pub skipped: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Tail {
    pub backend: Part,
    pub scraper: Part,
}

/// `GET /api/logs?backend=&scraper=&debug=` — what has been written since.
pub async fn tail(
    State(state): State<SharedState>,
    Query(since): Query<Since>,
) -> ApiResult<Json<Tail>> {
    Ok(Json(Tail {
        backend: read_since(&state.log_dir, Which::Backend, since.backend, since.debug),
        scraper: read_since(&state.log_dir, Which::Scraper, since.scraper, since.debug),
    }))
}

/// Read one log from where we left off.
///
/// Everything that can go wrong here — no such file yet, a rotated file, a log that
/// has been pruned from under us — comes back as "nothing new" rather than an error.
/// A page that stops showing leads because a log file is missing would be absurd.
fn read_since(dir: &Path, which: Which, after: Option<u64>, debug: bool) -> Part {
    use std::io::{Read, Seek, SeekFrom};

    let Some(path) = newest_log(dir, which) else {
        return Part::default();
    };
    let Ok(mut file) = std::fs::File::open(&path) else {
        return Part::default();
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);

    // A page that has just opened starts at the end: everything before it belongs to
    // a session that is over.
    let from = match after {
        None => size,
        // The file got smaller: it rotated, or it was pruned. Start again from the
        // top of what is there now rather than seeking past the end.
        Some(at) if at > size => 0,
        Some(at) => at,
    };

    // Far behind: skip to what we can carry and say so, rather than quietly showing
    // lines from an hour ago as though they were happening now.
    let skipped = size.saturating_sub(from) > MOST_BYTES;
    let from = if skipped { size - MOST_BYTES } else { from };

    if file.seek(SeekFrom::Start(from)).is_err() {
        return Part::default();
    }
    let mut text = String::new();
    // Lossy on purpose: a half-written line at the end of the file is a normal thing
    // to catch, and refusing the whole answer over it would be silly.
    let mut raw = Vec::new();
    if file.take(MOST_BYTES).read_to_end(&mut raw).is_err() {
        return Part::default();
    }
    text.push_str(&String::from_utf8_lossy(&raw));

    let mut lines = tidy(&text, debug);
    // The first look is where the page joins in, not a replay of what it missed.
    if after.is_none() && lines.len() > FIRST_LOOK {
        lines = lines.split_off(lines.len() - FIRST_LOOK);
    }
    if lines.len() > MOST_LINES {
        lines = lines.split_off(lines.len() - MOST_LINES);
    }

    Part {
        at: from + raw.len() as u64,
        lines,
        skipped,
    }
}

/// Today's file for this log.
///
/// `tracing-appender` rolls daily and names them `backend.log.2026-09-20`, so the one
/// being written is the last in alphabetical order. The bare `backend.log` left over
/// from before the rolling was set up is ignored unless it is all there is.
pub fn newest_log(dir: &Path, which: Which) -> Option<PathBuf> {
    let mut best: Option<String> = None;
    for entry in std::fs::read_dir(dir).ok()? {
        let Ok(name) = entry.map(|e| e.file_name().to_string_lossy().into_owned()) else {
            continue;
        };
        if !name.starts_with(which.prefix()) {
            continue;
        }
        let better = match &best {
            None => true,
            Some(kept) => name > *kept,
        };
        if better {
            best = Some(name);
        }
    }
    best.map(|name| dir.join(name))
}

/// Whole lines, with the terminal codes taken out and the noise optionally dropped.
///
/// Pure, so what reaches the page is a test rather than something you find out by
/// reading a screen full of `[2m[35m`.
pub fn tidy(text: &str, debug: bool) -> Vec<String> {
    text.lines()
        // The last line may be half-written; it will be whole next time round.
        .map(plain)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| debug || !is_noise(line))
        // Always, debug or not: this one would feed on itself.
        .filter(|line| !is_our_own_watching(line))
        .collect()
}

/// A log line with the colour codes taken out.
///
/// The file is written without them, but the same lines also go to a terminal that
/// has them, and a stray escape would be printed as `[2m` rather than seen.
pub fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // An escape runs to the letter that ends it — `\u{1b}[1;35m`.
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

/// Is this a line only somebody debugging wants?
///
/// The web server writes two of these for every request the page makes, which on a
/// page that polls is most of the log.
pub fn is_noise(line: &str) -> bool {
    line.contains("DEBUG") || line.contains("TRACE")
}

/// Is this line the terminal watching itself?
///
/// The page asks for the log every couple of seconds, and the web server writes two
/// lines about every request it handles. Left in, the window fills with the sound of
/// its own breathing within a minute — and every one of those lines makes the next
/// answer bigger.
pub fn is_our_own_watching(line: &str) -> bool {
    line.contains("uri=/api/logs")
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
