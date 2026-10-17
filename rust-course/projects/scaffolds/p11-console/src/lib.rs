//! Runnable first checkpoint: accessible local filter state, before network integration.
use leptos::prelude::*;
#[component]
fn App() -> impl IntoView {
    let (filter, set_filter) = signal(String::new());
    view! {<main><h1>"My local operations console"</h1><label for="filter">"Symbol filter"</label><input id="filter" prop:value=move||filter.get() on:input=move|ev|set_filter.set(event_target_value(&ev))/><p role="status">{move||format!("Current filter: {}",filter.get())}</p></main>}
}
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    leptos::mount::mount_to_body(App);
}
