//! Taking a picture of a broken website.
//!
//! # What this file is here to teach
//!
//! Driving a program that is not ours, and being honest when it is not there. The
//! browser is already installed beside us, so this shells out to it rather than
//! adding a library that would drive it over a socket.
//!
//! The picture is the point: "your website is down" is an argument on the telephone,
//! and a photograph of the error the owner's customers see is not an argument at all.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The browser we use, unless told otherwise.
///
/// Installed in the container already. `WEBLEADS_CHROME` overrides it for a machine
/// where it lives somewhere else.
pub const DEFAULT_BROWSER: &str = "/usr/bin/chromium-headless-shell";

/// How long one picture may take before we give up on it.
///
/// A dead site is exactly the kind that never answers, so this has to be short enough
/// that "do all of them" is not an afternoon.
pub const TIMEOUT: Duration = Duration::from_secs(25);

/// How big the picture is. A browser window, not a postage stamp.
pub const WINDOW: (u32, u32) = (1280, 900);

/// What went wrong, in words the page can show.
#[derive(Debug, thiserror::Error)]
pub enum ShotError {
    #[error("no browser here to take pictures with — looked for {0}")]
    NoBrowser(String),

    #[error("{0} did not answer within {} seconds", TIMEOUT.as_secs())]
    TimedOut(String),

    #[error("the browser could not photograph {0}")]
    Failed(String),

    #[error("the picture could not be written")]
    Writing(#[from] std::io::Error),
}

/// Where the browser lives.
pub fn browser() -> PathBuf {
    std::env::var("WEBLEADS_CHROME")
        .unwrap_or_else(|_| DEFAULT_BROWSER.to_owned())
        .into()
}

/// A file name that is safe to put on disk, from an id that is not.
///
/// A lead's id is Google's own number when we have one and `name|lat,lon` when we do
/// not — which holds slashes, spaces and Hungarian letters. Pure, so what lands in
/// the folder can be checked without writing anything.
pub fn safe_name(lead_id: &str) -> String {
    let cleaned: String = lead_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    // Two dashes in a row read as a mistake, and a name of pure dashes reads as a
    // bug. Neither is wrong on disk; both are wrong to look at.
    let tidy = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let tidy = if tidy.is_empty() {
        "unnamed".to_owned()
    } else {
        tidy
    };
    // Long ids exist. A file name has a ceiling, and a truncated one still points at
    // one business because the start of an id is what varies.
    format!("{}.png", &tidy[..tidy.len().min(80)])
}

/// What we ask the browser to do.
///
/// Pure and tested, because getting one of these wrong gives a blank picture rather
/// than an error, and a blank picture looks like a broken website.
pub fn arguments(url: &str, out: &Path) -> Vec<String> {
    vec![
        // Root inside a container has no sandbox to drop into.
        "--no-sandbox".to_owned(),
        "--disable-gpu".to_owned(),
        "--hide-scrollbars".to_owned(),
        // Let the page settle without waiting for a slow advert to finish.
        "--virtual-time-budget=6000".to_owned(),
        format!("--window-size={},{}", WINDOW.0, WINDOW.1),
        format!("--screenshot={}", out.display()),
        url.to_owned(),
    ]
}

/// Take one picture. Returns what the browser said about it, in words.
///
/// Never silently skips: a missing browser is an error you can read, not an empty
/// square you have to guess at.
pub async fn take(url: &str, out: &Path) -> Result<String, ShotError> {
    let browser = browser();
    if !browser.exists() {
        return Err(ShotError::NoBrowser(browser.display().to_string()));
    }
    if let Some(folder) = out.parent() {
        tokio::fs::create_dir_all(folder).await?;
    }

    let mut child = tokio::process::Command::new(&browser)
        .args(arguments(url, out))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let finished = tokio::time::timeout(TIMEOUT, child.wait()).await;
    let status = match finished {
        Ok(status) => status?,
        Err(_) => {
            // Left alone it would sit there for ever holding the folder open.
            let _ = child.kill().await;
            return Err(ShotError::TimedOut(url.to_owned()));
        }
    };

    if !status.success() || !out.exists() {
        return Err(ShotError::Failed(url.to_owned()));
    }

    let answered = describe_size(tokio::fs::metadata(out).await?.len());
    tokio::fs::write(out.with_extension("txt"), &answered).await?;
    Ok(answered)
}

/// What the picture itself says, from how big it is.
///
/// A browser that reaches nothing still writes a picture — a white rectangle, and a
/// very small file. Saying so is the difference between "their site shows an error"
/// and "nothing loaded at all", which are different things to say on the telephone.
///
/// Pure, so the boundary can be checked without a browser.
pub fn describe_size(bytes: u64) -> String {
    if bytes < 6_000 {
        "nothing loaded — a blank page".to_owned()
    } else {
        "the page as their customers see it".to_owned()
    }
}

#[cfg(test)]
#[path = "shot_tests.rs"]
mod tests;
