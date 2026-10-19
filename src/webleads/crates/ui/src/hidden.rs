//! What you have hidden, and how to get it back.
//!
//! A panel rather than a page: hiding is something you do while working through the
//! list, and the way back has to be one press away or you stop trusting the button.

use leptos::*;

use crate::api;

#[component]
pub fn HiddenPanel(
    /// How many the list is currently keeping off the screen.
    count: Memo<i64>,
    /// Bumped whenever something is hidden or brought back, so the list reloads.
    on_change: WriteSignal<u32>,
    /// Bumped from outside when something new is hidden, so the panel reloads.
    reload: ReadSignal<u32>,
) -> impl IntoView {
    let (open, set_open) = create_signal(false);
    let rules = create_local_resource(
        move || (open.get(), reload.get()),
        |(open, _)| async move {
            if !open {
                return Ok(Vec::new());
            }
            api::hidden().await
        },
    );

    let bring_back = move |id: i64| {
        spawn_local(async move {
            crate::log::step("hidden", &format!("showing {id} again"));
            if api::unhide(id).await.is_ok() {
                on_change.update(|n| *n += 1);
                rules.refetch();
            }
        });
    };

    view! {
        {move || (count.get() > 0).then(|| view! {
            <button
                class="btn btn--quiet"
                style="font-size:15px;padding:4px 9px"
                on:click=move |_| set_open.update(|o| *o = !*o)
            >
                {move || if open.get() {
                    "hide the hidden".to_owned()
                } else {
                    format!("{} hidden", count.get())
                }}
            </button>
        })}

        {move || open.get().then(|| view! {
            <div class="panel" style="padding:12px;margin-bottom:14px">
                <div class="field__label">"Kept off the list"</div>
                <Suspense fallback=move || view! { <div class="empty"><span class="spinner"></span></div> }>
                    {move || rules.get().map(|answer| match answer {
                        Err(problem) => view! { <div class="alert is-bad">{problem}</div> }.into_view(),
                        Ok(rules) if rules.is_empty() => {
                            view! { <div class="is-dim" style="font-size:16px">"Nothing is hidden."</div> }
                                .into_view()
                        }
                        Ok(rules) => rules.into_iter().map(|rule| {
                            let id = rule.id;
                            view! {
                                <div style="display:flex;gap:10px;align-items:center;padding:4px 0">
                                    <span style="flex:1;font-size:16px">
                                        {rule.label.clone()}
                                        // A chain and a single shop read differently,
                                        // and which one you hid is the thing you want
                                        // to know before bringing it back.
                                        <span class="is-dim" style="margin-left:8px">
                                            {if rule.is_chain {
                                                format!("chain · {} on the list", rule.catches)
                                            } else {
                                                "one business".to_owned()
                                            }}
                                        </span>
                                    </span>
                                    <button
                                        class="btn btn--quiet"
                                        style="font-size:14px"
                                        on:click=move |_| bring_back(id)
                                    >
                                        "show again"
                                    </button>
                                </div>
                            }
                        }).collect_view(),
                    })}
                </Suspense>
            </div>
        })}
    }
}
