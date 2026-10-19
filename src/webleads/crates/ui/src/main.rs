//! The browser side, in Rust, compiled to WebAssembly.
//!
//! # What this file is here to teach
//!
//! - `view!` is a macro that writes HTML-shaped Rust. It is checked at compile time,
//!   so a mistyped tag is a build error rather than a blank page.
//! - Components are plain functions with `#[component]` on them. They take their
//!   inputs as arguments, which is why a typo in a property is a compile error.
//! - A closure inside `view!` is how a value is read reactively. Wrapping something
//!   in `move ||` is what tells Leptos to re-run that bit when the value changes.
//!   (Leptos also allows calling a signal like `count()`, but only on the nightly
//!   compiler — on stable it is `.get()`.)

mod api;
mod api_runs;
mod areas;
mod cities;
mod clock;
mod coming;
mod console;
mod contact;
mod filters;
mod hidden;
mod home;
mod leads;
mod link;
mod log;
mod map;
mod picker;
mod queue;
mod report;
mod run;
mod scale;
mod searches;
mod settings;
mod status;
mod table;
mod tally;
mod terminal;
mod trades;

use leptos::*;

fn main() {
    // Panics in WebAssembly are otherwise silent. This sends them to the browser
    // console, which is the difference between a bug and a mystery.
    console_error_panic_hook::set_once();
    report_problems_to_the_server();

    leptos::mount_to_body(|| view! { <App/> });
}

/// Send anything that goes wrong in here to the server, so it lands in a file.
///
/// The browser was the one place we could not see. `logs/frontend.log` sat empty from
/// the day it was made, so a page that misbehaved left no trace and the only evidence
/// was you saying "it did something odd".
///
/// Two hooks, because they catch different things: `onerror` gets an exception that
/// reaches the top, `onunhandledrejection` gets an async one that nobody caught.
fn report_problems_to_the_server() {
    use wasm_bindgen::prelude::*;

    let Some(window) = web_sys::window() else {
        return;
    };

    let on_error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(|event: web_sys::ErrorEvent| {
        let at = format!("{}:{}", event.filename(), event.lineno());
        spawn_local(async move {
            api::report_problem(&event.message(), Some(&at)).await;
        });
    });
    // A listener rather than `window.onerror = ...`. The property form is called with
    // five separate arguments — message, source, line, column, error — and a handler
    // expecting one event object simply never fires. The listener form delivers a
    // proper `ErrorEvent`, which is what this reads.
    let _ = window.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref());
    // `forget` on purpose: this listener must outlive `main`, and there is nothing
    // later that could own it.
    on_error.forget();

    let on_rejected = Closure::<dyn FnMut(web_sys::PromiseRejectionEvent)>::new(
        |event: web_sys::PromiseRejectionEvent| {
            let reason = event
                .reason()
                .as_string()
                .unwrap_or_else(|| format!("{:?}", event.reason()));
            spawn_local(async move {
                api::report_problem(&reason, Some("a promise nobody caught")).await;
            });
        },
    );
    // No setter for this one, so it goes on as an ordinary listener.
    let _ = window.add_event_listener_with_callback(
        "unhandledrejection",
        on_rejected.as_ref().unchecked_ref(),
    );
    on_rejected.forget();
}

#[component]
fn App() -> impl IntoView {
    view! {
        // The artwork and the masthead, from the design sheet.
        <div class="hero">
            <img class="hero__svg" src="/img/hero-221b.svg" alt="" width="1600" height="300"/>
            // The legend under the artwork, off for now.
            // <div class="legend">
            //     "Two hundred and twenty-one B — and not a website behind any of these doors."
            // </div>
        </div>

        <header class="masthead" style="margin-bottom:26px">
            <div class="masthead__inner">
                <div class="masthead__rays"></div>
                <img class="masthead__mark" src="/img/mark-sherlock.svg" alt="" width="96"/>
                <h1>"WEBLEADS"</h1>
                <div class="sub">"businesses with a telephone BUT no website"</div>
                <div class="ded">
                    "Every shop within reach that somebody can ring and nobody can look up. \
                     Search a district, work the list, and keep what you have said to whom."
                </div>
            </div>
        </header>
        <leads::Leads/>
    }
}
