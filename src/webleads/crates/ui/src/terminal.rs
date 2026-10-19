//! The window that says what is actually happening.
//!
//! # What this file is here to teach
//!
//! One place that three unrelated things can push lines into, without any of them
//! knowing about the others: the page's own requests, the server's log, and the
//! searching. A shared signal in Leptos's context is how that is done — everything
//! that writes reaches for the same handle, and the window simply draws whatever is
//! in it.
//!
//! **Every line is real.** The prompt is dressed as a local machine in
//! oh-my-zsh colours because that is the look we wanted; nothing underneath it is
//! invented. Nothing can be typed into it either — it is a window onto what happened,
//! not a shell on the host.

use std::collections::VecDeque;

use leptos::*;

/// How many lines the window keeps.
///
/// A ninety-minute search writes thousands. Past this the oldest go, because the
/// browser holding every line of an hour and a half is how a page starts to crawl.
const KEPT: usize = 600;

/// What colour a line is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Ordinary output.
    Plain,
    /// Something we asked for, or the state of the search.
    Note,
    /// It worked.
    Good,
    /// Worth reading.
    Warn,
    /// It went wrong.
    Bad,
    /// Quiet — timings, byte counts, the things you skim past.
    Dim,
    /// A command, typed at the prompt.
    Command,
}

impl Tone {
    /// The stylesheet's own classes, which the design sheet already defines.
    pub fn class(self) -> &'static str {
        match self {
            Self::Plain => "terminal__line",
            Self::Note => "terminal__line is-accent",
            Self::Good => "terminal__line is-ok",
            Self::Warn => "terminal__line is-warn",
            Self::Bad => "terminal__line is-bad",
            Self::Dim => "terminal__line is-dim",
            Self::Command => "terminal__line terminal__cmd",
        }
    }
}

/// One line in the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub tone: Tone,
    /// Drawn after a prompt, as though you had typed it.
    pub typed: bool,
}

/// Who the server runs as and where, as it reported it. The prompt shows these.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Machine {
    pub user: String,
    pub host: String,
    pub dir: String,
}

/// The window's scrollback, reachable from anywhere in the page.
#[derive(Debug, Clone, Copy)]
pub struct Session {
    lines: RwSignal<VecDeque<Line>>,
    /// The machine behind the prompt, once the server has said.
    machine: RwSignal<Option<Machine>>,
    /// Is the detail box ticked? Kept here rather than beside the checkbox so that
    /// the requests deep in the page can ask without being handed a signal each.
    detail: RwSignal<bool>,
}

impl Session {
    /// Say which machine the prompt belongs to.
    pub fn set_machine(&self, machine: Machine) {
        self.machine.set(Some(machine));
    }

    /// Put one line in the window.
    pub fn say(&self, text: impl Into<String>, tone: Tone) {
        self.push(Line {
            text: text.into(),
            tone,
            typed: false,
        });
    }

    /// Put a command in the window, as though it had been typed at the prompt.
    pub fn typed(&self, command: impl Into<String>) {
        self.push(Line {
            text: command.into(),
            tone: Tone::Command,
            typed: true,
        });
    }

    fn push(&self, line: Line) {
        self.lines.update(|kept| {
            kept.push_back(line);
            while kept.len() > KEPT {
                kept.pop_front();
            }
        });
    }

    /// Everything in it, oldest first.
    pub fn lines(&self) -> Vec<Line> {
        self.lines.get().into_iter().collect()
    }

    /// The detail box, to read and to set.
    pub fn detail(&self) -> RwSignal<bool> {
        self.detail
    }

    /// Put a line in only when the detail box is ticked.
    pub fn fine(&self, text: impl Into<String>) {
        if self.detail.get_untracked() {
            self.say(text, Tone::Dim);
        }
    }
}

/// Make the window's scrollback and hand it to the whole page.
///
/// Called once, from the top. Everything else reaches it with [`session`].
pub fn provide_session() -> Session {
    let session = Session {
        lines: create_rw_signal(VecDeque::new()),
        machine: create_rw_signal(None),
        detail: create_rw_signal(false),
    };
    provide_context(session);
    session
}

/// The window's scrollback, from anywhere below the top of the page.
///
/// `None` before the page has been built — in a test, for instance — so writing a
/// line is never a reason for anything to fall over.
pub fn session() -> Option<Session> {
    use_context::<Session>()
}

/// Put a line in the window, wherever you are, if there is a window.
pub fn say(text: impl Into<String>, tone: Tone) {
    if let Some(session) = session() {
        session.say(text, tone);
    }
}

/// Put a typed command in the window, wherever you are.
pub fn typed(command: impl Into<String>) {
    if let Some(session) = session() {
        session.typed(command);
    }
}

/// Put a line in the window only if the detail box is ticked.
///
/// This is how a request shows what came back without burying the window for
/// everybody who only wanted to know that the search is running.
pub fn fine(text: impl Into<String>) {
    if let Some(session) = session() {
        session.fine(text);
    }
}

/// What a request looks like as a command.
///
/// The page's own traffic, written the way you would have made it by hand — because
/// that is exactly what it is. Pure, so the shape is a test.
pub fn as_command(what: &str) -> String {
    // `what` arrives as "POST /api/runs/:id/stop" — the method and the address. The
    // address is what tells it apart from an ordinary sentence: "taking a picture"
    // also has a space in it, and `curl -Xtaking a picture` would be nonsense.
    match what.split_once(' ') {
        Some(("GET", path)) if path.starts_with('/') => format!("curl -s {path}"),
        Some((method, path)) if path.starts_with('/') => format!("curl -s -X{method} {path}"),
        _ => what.to_owned(),
    }
}

/// The most of a request or answer the window will show.
///
/// The list of leads is a quarter of a megabyte. Shown whole it would be the only
/// thing in the window, and none of it would be read.
const BODY_SHOWN: usize = 600;

/// A body short enough to read on one screen, saying so when it was cut.
///
/// Cut by letters rather than by bytes: Hungarian names are full of accents, and
/// half of one is rubbish the terminal cannot draw. Pure, and tested.
pub fn snip(body: &str) -> String {
    let tidy = body.trim();
    let length = tidy.chars().count();
    if length <= BODY_SHOWN {
        return tidy.to_owned();
    }
    let kept: String = tidy.chars().take(BODY_SHOWN).collect();
    format!("{kept}… (of {length} characters)")
}

/// What a server log line should be drawn in.
///
/// Read off the level the line carries, so a warning looks like a warning without
/// anything having to agree about it beforehand. Pure, and tested.
pub fn tone_of(line: &str) -> Tone {
    if line.contains("ERROR") || line.contains("‼️") {
        Tone::Bad
    } else if line.contains("WARN") || line.contains("⚠️") {
        Tone::Warn
    } else if line.contains("DEBUG") || line.contains("TRACE") {
        Tone::Dim
    } else if line.contains(" INFO") || line.contains("INFO ") {
        Tone::Plain
    } else {
        Tone::Dim
    }
}

/// Is the window showing the newest lines, or has it been scrolled up?
///
/// Pure, so "it stops following when you scroll up to read" is a test rather than
/// something you check by trying to read a moving screen. A little slack, because a
/// browser's own scrolling lands a pixel or two out.
pub fn near_the_bottom(at: i32, showing: i32, height: i32) -> bool {
    at + showing >= height - 40
}

/// The prompt, in oh-my-zsh colours: the server's real user, machine and folder. Just
/// the arrow until it has said — a made-up name would be the one thing on the screen
/// that is not real.
#[component]
fn Prompt(machine: RwSignal<Option<Machine>>) -> impl IntoView {
    view! {
        {move || machine.get().map(|m| view! {
            <span class="terminal__user">{m.user}</span>
            <span class="terminal__host">{format!("@{}", m.host)}</span>
            <span class="terminal__path">{format!(" {}", m.dir)}</span>
        })}
        <span class="terminal__sigil">" ➜ "</span>
    }
}

/// The window's title: who, on what, where. Pure, so the wording is a test.
pub fn title_said(machine: Option<&Machine>) -> String {
    match machine {
        Some(m) => format!("{}@{} ◆ {}", m.user, m.host, m.dir),
        None => "connecting…".to_owned(),
    }
}

/// The window itself.
#[component]
pub fn Terminal(session: Session) -> impl IntoView {
    let debug = session.detail();
    let body = create_node_ref::<html::Div>();

    // Follow the bottom as lines arrive — unless you have scrolled up to read
    // something, in which case being yanked back down is infuriating.
    create_effect(move |_| {
        let count = session.lines().len();
        let Some(element) = body.get() else {
            return;
        };
        let _ = count;
        let height = element.scroll_height();
        let showing = element.client_height();
        let at = element.scroll_top();
        if near_the_bottom(at, showing, height) {
            element.set_scroll_top(height);
        }
    });

    view! {
        <div class="terminal wl-term">
            <div class="terminal__bar">
                <span class="terminal__mark" aria-hidden="true"></span>
                <span class="terminal__title">
                    <b>{move || title_said(session.machine.get().as_ref())}</b>
                </span>
                <label class="wl-term__debug" title="Show every request and the fine detail">
                    <input
                        type="checkbox"
                        prop:checked=move || debug.get()
                        on:change=move |ev| debug.set(event_target_checked(&ev))
                    />
                    "debug"
                </label>
            </div>

            <div class="terminal__body" id="wl-term-body" node_ref=body>
                {move || session.lines().into_iter().map(|line| view! {
                    <span class=line.tone.class()>
                        {line.typed.then(|| view! { <Prompt machine=session.machine/> })}
                        {line.text}
                    </span>
                }).collect_view()}
                // The prompt, waiting for the next thing you press.
                <span class="terminal__line">
                    <Prompt machine=session.machine/>
                    <span class="terminal__cursor"></span>
                </span>
            </div>
        </div>
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
