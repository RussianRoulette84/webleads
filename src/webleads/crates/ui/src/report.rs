//! Which words worked, and which came back with nothing.
//!
//! The one screen that tells you a Hungarian word is wrong rather than that a trade
//! does not exist in this district.

use leptos::*;

use crate::api::Word;

/// Every word and what it returned.
#[component]
pub fn HitReport(words: Vec<Word>) -> impl IntoView {
    let empty = words.iter().filter(|w| w.found == Some(0)).count();
    // Said in the console too, so it is in the same place as everything else that
    // happened during the run.
    if empty > 0 {
        crate::log::warn(
            "search",
            &format!("{empty} words returned nothing — usually the wording"),
        );
    }

    view! {
        <div style="margin-top:16px">
            <h4 class="heading" style="font-size:16px">"Which words worked"</h4>
            {(empty > 0).then(|| view! {
                <p class="is-warn" style="font-size:16px;margin:6px 0 10px">
                    {format!(
                        "{empty} word{} returned nothing — usually a word that needs fixing.",
                        if empty == 1 { "" } else { "s" }
                    )}
                </p>
            })}
            <div style="max-height:320px;overflow:auto">
            <table class="table">
                <thead>
                    <tr><th>"Word"</th><th>"Trade"</th><th style="text-align:right">"Found"</th></tr>
                </thead>
                <tbody>
                    {words.into_iter().map(|word| {
                        let (class, shown) = describe_hits(&word);
                        view! {
                            <tr>
                                <td style="font-size:16px">{word.phrase}</td>
                                <td class="is-dim" style="font-size:16px">
                                    {word.trade.unwrap_or_else(|| "—".into())}
                                </td>
                                <td style="text-align:right"><span class=class>{shown}</span></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
            </div>
        </div>
    }
}

/// How one word's result reads, and how loudly.
///
/// Pure, so the wording can be tested. Nought is called out because that is the
/// number worth acting on — it nearly always means a bad translation rather than a
/// trade nobody in Budapest practises.
pub fn describe_hits(word: &Word) -> (&'static str, String) {
    match (word.state.as_str(), word.found) {
        ("failed", _) => (
            "is-bad",
            word.note.clone().unwrap_or_else(|| "failed".into()),
        ),
        ("searching", _) => ("is-dim", "searching…".to_owned()),
        ("waiting", _) => ("is-dim", "waiting".to_owned()),
        (_, Some(0)) => ("is-warn", "nothing".to_owned()),
        (_, Some(n)) => ("", n.to_string()),
        (_, None) => ("is-dim", "—".to_owned()),
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
