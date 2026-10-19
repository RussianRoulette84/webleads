//! Whether a search can be started: is the server there, and is its scraper?
//!
//! Asked every few seconds, shown as a light under SCRAPE NOW, and the reason that
//! button is greyed out when the answer is no — with the command that fixes it.

use leptos::*;

use crate::api;

/// How often to ask, in milliseconds.
pub const HEALTH_TICK: u64 = 5_000;

/// What to type in the webleads folder to start everything.
pub const START_ALL: &str = "docker compose up -d";

/// What to type in the webleads folder to start only the scraper.
pub const START_SCRAPER: &str = "docker compose up -d scraper";

/// The state of the line to Google Maps, as the last check found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Not asked yet.
    Checking,
    /// The server and its scraper both answer.
    Online,
    /// The server answers; the scraper does not.
    ScraperOffline,
    /// Nothing answers at all.
    ServerOffline,
}

impl Link {
    /// What the light says. Pure.
    pub fn said(self) -> &'static str {
        match self {
            Self::Checking => "CHECKING THE SCRAPER",
            Self::Online => "SCRAPER ONLINE",
            Self::ScraperOffline => "SCRAPER OFFLINE",
            Self::ServerOffline => "WEBLEADS SERVER OFFLINE",
        }
    }

    /// The command that brings it back, if anything is down. Pure.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::ScraperOffline => Some(START_SCRAPER),
            Self::ServerOffline => Some(START_ALL),
            Self::Checking | Self::Online => None,
        }
    }

    /// Whether SCRAPE NOW can be pressed. Pure.
    pub fn can_scrape(self) -> bool {
        self == Self::Online
    }
}

/// Read a health answer as a link state. Pure.
pub fn link_of(health: &Result<api::Health, String>) -> Link {
    match health {
        Ok(health) if health.scraper => Link::Online,
        Ok(_) => Link::ScraperOffline,
        Err(_) => Link::ServerOffline,
    }
}

/// The light, its label, and — when something is down — the command to type.
#[component]
pub fn ScraperLight(link: ReadSignal<Link>) -> impl IntoView {
    let (copied, set_copied) = create_signal(false);

    let copy = move |command: &'static str| {
        let Some(window) = web_sys::window() else {
            return;
        };
        // `127.0.0.1` counts as a secure context, so this works on the page as it is.
        let _ = window.navigator().clipboard().write_text(command);
        set_copied.set(true);
        let _ = set_timeout_with_handle(
            move || set_copied.set(false),
            std::time::Duration::from_millis(1400),
        );
    };

    view! {
        <div class="wl-link">
            <span class=move || {
                if link.get() == Link::Online { "wl-link__light is-on" } else { "wl-link__light" }
            }></span>
            <span class="wl-link__label">{move || link.get().said()}</span>
            {move || link.get().command().map(|command| view! {
                <span class="is-dim wl-link__how">"start it in the webleads folder:"</span>
                <code class="wl-link__cmd">{command}</code>
                <button class="btn btn--quiet wl-link__copy" on:click=move |_| copy(command)>
                    {move || if copied.get() { "copied" } else { "copy" }}
                </button>
            })}
        </div>
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
