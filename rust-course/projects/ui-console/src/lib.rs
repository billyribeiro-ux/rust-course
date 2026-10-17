use leptos::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use course_ui_model::*;

// The browser has one polling loop, one request at a time, and a 3-second timeout.
// The local fixture identity is visible client code: it is NOT a secret or production auth.
async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let controller = web_sys::AbortController::new().map_err(|_| "AbortController unavailable")?;
    let abort = controller.clone();
    let timer = gloo_timers::callback::Timeout::new(3_000, move || abort.abort());
    let response = gloo_net::http::Request::get(path)
        .header("Authorization", "Bearer demo-reader")
        .abort_signal(Some(&controller.signal()))
        .send()
        .await
        .map_err(|_| "Request failed or timed out. Check the local API.")?;
    if !response.ok() {
        return Err(format!("API returned HTTP {}", response.status()));
    }
    let result = response
        .json()
        .await
        .map_err(|_| "The response does not match the expected response schema.".into());
    drop(timer);
    result
}

#[component]
fn App() -> impl IntoView {
    let (snapshot, set_snapshot) = signal(Option::<Snapshot>::None);
    let (error, set_error) = signal(String::new());
    let (filter, set_filter) = signal(String::new());
    let (paused, set_paused) = signal(false);
    let (status, set_status) = signal("Connecting to the local API…".to_string());
    let (symbol, set_symbol) = signal("RUST".to_string());
    let (quantity, set_quantity) = signal("1".to_string());
    let (order_status, set_order_status) = signal(String::new());
    let (pending, set_pending) = signal(false);
    let (unresolved, set_unresolved) = signal(Option::<PendingOrder>::None);
    let (history, set_history) = signal(HistoryPage::default());
    let (jobs, set_jobs) = signal(Vec::<Job>::new());
    let (watch_input, set_watch_input) = signal("RUST".to_string());
    let (command_status, set_command_status) = signal(String::new());
    let (command_pending, set_command_pending) = signal(false);
    let (replay_submission, set_replay_submission) = signal(ReplaySubmission::default());
    // This flag outlives reactive slots, so an awaited response cannot read disposed state.
    // Relaxed is sufficient: it communicates only a boolean, not other shared data.
    let alive = Arc::new(AtomicBool::new(true));
    let cleanup_alive = Arc::clone(&alive);
    on_cleanup(move || cleanup_alive.store(false, Ordering::Relaxed));
    let poll_alive = Arc::clone(&alive);
    leptos::task::spawn_local(async move {
        while poll_alive.load(Ordering::Relaxed) {
            if !paused.get_untracked() {
                let result = get_json::<Snapshot>("/api/snapshot").await;
                if !poll_alive.load(Ordering::Relaxed) {
                    break;
                }
                match result {
                    Ok(data) => {
                        let after = data.events.saturating_sub(60);
                        let history_result = get_json::<HistoryPage>(&format!(
                            "/api/history?after={after}&limit=60"
                        ))
                        .await;
                        if !poll_alive.load(Ordering::Relaxed) {
                            break;
                        }
                        match history_result {
                            Ok(h) => set_history.set(h),
                            Err(e) => set_command_status.set(format!("History unavailable: {e}")),
                        }
                        let jobs_result = get_json::<Vec<Job>>("/api/jobs").await;
                        if !poll_alive.load(Ordering::Relaxed) {
                            break;
                        }
                        match jobs_result {
                            Ok(j) => set_jobs.set(j),
                            Err(e) => set_command_status.set(format!("Jobs unavailable: {e}")),
                        }
                        set_snapshot.set(Some(data));
                        set_error.set(String::new());
                        set_status.set(
                            if paused.get_untracked() {
                                "Updates paused"
                            } else {
                                "Connected · 2-second pause between completed refresh cycles"
                            }
                            .into(),
                        );
                    }
                    Err(e) => {
                        set_error.set(e);
                        set_status.set(
                            if paused.get_untracked() {
                                "Updates paused"
                            } else {
                                "Disconnected · retrying after the next interval"
                            }
                            .into(),
                        );
                    }
                }
            }
            gloo_timers::future::TimeoutFuture::new(2_000).await;
        }
    });
    let order_alive = Arc::clone(&alive);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if pending.get_untracked() {
            return;
        }
        let retrying = unresolved.get_untracked().is_some();
        let draft = if let Some(draft) = unresolved.get_untracked() {
            draft // Retrying a logical instruction preserves both ID and payload.
        } else {
            let n = match quantity.get_untracked().parse::<u64>() {
                Ok(n @ 1..=1000) => n,
                _ => {
                    set_order_status.set("Enter a whole quantity from 1 to 1000.".into());
                    return;
                }
            };
            let sym = symbol.get_untracked().trim().to_uppercase();
            if sym.is_empty() || sym.len() > 8 || !sym.bytes().all(|b| b.is_ascii_uppercase()) {
                set_order_status.set("Use a symbol of 1–8 uppercase letters.".into());
                return;
            }
            PendingOrder {
                id: format!("ui-{}", uuid::Uuid::new_v4()),
                symbol: sym,
                quantity: n,
            }
        };
        let order_id = draft.id.clone();
        let body = draft.body();
        set_unresolved.set(Some(draft));
        set_pending.set(true);
        let request_alive = Arc::clone(&order_alive);
        leptos::task::spawn_local(async move {
            if !request_alive.load(Ordering::Relaxed) {
                return;
            }
            let Ok(controller) = web_sys::AbortController::new() else {
                set_order_status.set("Browser cancellation support is unavailable.".into());
                set_pending.set(false);
                return;
            };
            let abort = controller.clone();
            let timer = gloo_timers::callback::Timeout::new(3_000, move || abort.abort());
            let result = match gloo_net::http::Request::post("/api/orders")
                .header("Authorization", "Bearer demo-operator")
                .abort_signal(Some(&controller.signal()))
                .json(&body)
            {
                Ok(request) => request.send().await.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            drop(timer);
            if !request_alive.load(Ordering::Relaxed) {
                return;
            }
            let outcome = write_outcome(result.as_ref().ok().map(|r| r.status()), retrying);
            let message = match outcome {
                WriteOutcome::Accepted => {
                    set_unresolved.set(None);
                    format!(
                        "Simulated order {order_id} accepted. The next snapshot updates holdings."
                    )
                }
                WriteOutcome::NotAccepted => {
                    set_unresolved.set(None);
                    format!(
                        "Order {order_id} was not accepted. Inspect validation, authorization and current state."
                    )
                }
                WriteOutcome::Unknown => format!(
                    "Unknown outcome for {order_id}. Check its exact ledger entry or retry this same instruction. Keep this page open; new orders stay disabled until reconciliation."
                ),
            };
            set_order_status.set(message);
            set_pending.set(false);
        });
    };
    let reconcile_alive = Arc::clone(&alive);
    let reconcile = move |_| {
        if pending.get_untracked() {
            return;
        }
        let Some(draft) = unresolved.get_untracked() else {
            return;
        };
        set_pending.set(true);
        let owner = Arc::clone(&reconcile_alive);
        leptos::task::spawn_local(async move {
            let result = get_json::<ExecutedOrder>(&format!("/api/orders/{}", draft.id)).await;
            if !owner.load(Ordering::Relaxed) {
                return;
            }
            match result {
                Ok(fill) if fill.order_id == draft.id && fill.symbol == draft.symbol && fill.side == "buy" && fill.quantity == draft.quantity as i64 => {
                    set_order_status.set(format!("Confirmed {} in the persistent ledger at {} per unit.",fill.order_id,money(fill.price_cents)));
                    set_unresolved.set(None);
                }
                Ok(_) => set_order_status.set("Ledger payload conflict: preserve the ID and request review; do not invent a new ID.".into()),
                Err(e) => set_order_status.set(format!("Still unresolved: {e}. Even a missing entry may race an in-flight write. Retry only the retained instruction.")),
            }
            set_pending.set(false);
        });
    };
    let watch_alive = Arc::clone(&alive);
    let update_watchlist = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let symbols = watch_input
            .get_untracked()
            .split(',')
            .map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if symbols.len() > 16
            || symbols
                .iter()
                .any(|s| s.len() > 8 || !s.bytes().all(|b| b.is_ascii_uppercase()))
        {
            set_command_status.set(
                "Use at most 16 symbols of 1–8 uppercase letters, separated by commas.".into(),
            );
            return;
        }
        command(
            "PUT",
            "/api/watchlist".into(),
            serde_json::json!({"symbols":symbols}),
            set_command_status,
            command_pending,
            set_command_pending,
            Arc::clone(&watch_alive),
        );
    };
    let replay_alive = Arc::clone(&alive);
    let schedule_replay = move |_| {
        if command_pending.get_untracked() {
            return;
        }
        let previous = snapshot
            .get_untracked()
            .and_then(|s| s.quotes.into_iter().find(|q| q.symbol == "LAB"))
            .map_or(0, |q| q.sequence);
        let mut submission = replay_submission.get_untracked();
        let body = match submission.prepare(|| format!("ui-{}", uuid::Uuid::new_v4()), previous) {
            Ok(body) => body,
            Err(e) => {
                set_command_status.set(e.into());
                return;
            }
        };
        set_replay_submission.set(submission);
        set_command_pending.set(true);
        let owner = Arc::clone(&replay_alive);
        leptos::task::spawn_local(async move {
            if !owner.load(Ordering::Relaxed) {
                return;
            }
            let result = send_mutation("POST", "/api/jobs/replay", &body).await;
            if !owner.load(Ordering::Relaxed) {
                return;
            }
            let mut submission = replay_submission.get_untracked();
            let outcome = submission.resolve(result.ok());
            set_replay_submission.set(submission);
            set_command_status.set(match outcome {
                WriteOutcome::Accepted => "Replay accepted. The next refresh shows its durable status.".into(),
                WriteOutcome::NotAccepted => "Replay was not accepted. Inspect validation, authorization and current state before submitting a new job.".into(),
                WriteOutcome::Unknown => "Replay outcome is unknown. Keep this page open and use Retry retained replay; it resends the exact ID and ticks, even if quotes have changed.".into(),
            });
            set_command_pending.set(false);
        });
    };
    let cancel_alive = Arc::clone(&alive);
    view! {
      <header>
        <span class="tag">"LOCAL LAB · SIMULATION ONLY"
        </span>
        <h1>"Operations console"
        </h1>
        <p>"Observe a service, trace a state change, and explain its failure boundary."
        </p>
      </header>
      <main>
        <section class="toolbar" aria-label="Connection controls">
          <p role="status">{move || status.get()}
          </p>
          <button on:click=move |_| {set_paused.update(|p|*p = !*p);set_status.set(if paused.get_untracked(){"Updates paused"}else{"Reconnecting…"}.into());}>{move || if paused.get(){"Resume updates"}else{"Pause updates"}}
          </button>
        </section>
        <Show when=move || !error.get().is_empty()>
          <p class="error" role="alert">{move || error.get()}
          </p>
        </Show>
        <Show when=move || snapshot.get().is_none()>
          <p>"No snapshot yet. Start the backend and synthetic feed using the lesson commands."
          </p>
        </Show>
        <div class="metrics">
          <article>
            <h2>"Simulated cash"
            </h2>
            <strong>{move || snapshot.get().map(|s|money(s.cash_cents)).unwrap_or_else(||"—".into())}
            </strong>
          </article>
          <article>
            <h2>"Recorded events"
            </h2>
            <strong>{move || snapshot.get().map(|s|s.events.to_string()).unwrap_or_else(||"—".into())}
            </strong>
          </article>
          <article>
            <h2>"Mode"
            </h2>
            <strong>{move || if snapshot.get().is_some_and(|s|s.simulated){"Paper only"}else{"Waiting"}}
            </strong>
          </article>
        </div>
        <section>
          <h2>"Latest quotes"
          </h2>
          <label for="quote-filter">"Filter symbols"
          </label>
          <input id="quote-filter" type="search" prop:value=move ||filter.get() on:input=move |ev|set_filter.set(event_target_value(&ev))/>
          <div class="table-scroll">
            <table>
              <caption>"Integer cents; sequence orders events within a symbol"
              </caption>
              <thead>
                <tr>
                  <th>"Symbol"
                  </th>
                  <th>"Price"
                  </th>
                  <th>"Sequence"
                  </th>
                </tr>
              </thead>
              <tbody>{move || snapshot.get().unwrap_or_default().quotes.into_iter().filter(|q|q.symbol.contains(&filter.get().to_uppercase())).map(|q|view!{
                <tr>
                  <td>{q.symbol}
                  </td>
                  <td>{money(q.price_cents)}
                  </td>
                  <td>{q.sequence}
                  </td>
                </tr>}).collect_view()}
              </tbody>
            </table>
          </div>
          <p>{move || if let Some(current) = snapshot.get() { if current.quotes.is_empty() { "No quotes. Run the synthetic feed; this is an empty state, not a $0 quote." } else if !current.quotes.iter().any(|q| q.symbol.contains(&filter.get().to_uppercase())) { "No symbols match this filter. Clear or change the filter." } else { "" } } else { "" }}
          </p>
        </section>
        <section>
          <h2>"Recorded LAB price history"
          </h2>
          <p>"Up to 60 recent event positions; unlike browser samples, points come from the persistent journal. The bounded history can expire older points."
          </p>
          <svg viewBox="0 0 600 130" role="img" aria-label="Recorded LAB price trend, with equivalent values below">
            <polyline fill="none" stroke="currentColor" stroke-width="3" points=move ||{let h=history.get().points.into_iter().filter(|p|p.symbol=="LAB").map(|p|p.price_cents).collect::<Vec<_>>();let min=h.iter().min().copied().unwrap_or(0);let max=h.iter().max().copied().unwrap_or(min+1);let range=(max-min).max(1) as f64;h.iter().enumerate().map(|(i,v)|format!("{},{}",i as f64*10.0,120.0-(*v-min) as f64/range*110.0)).collect::<Vec<_>>().join(" ")}/>
          </svg>
          <details>
            <summary>"History values and retention boundary"
            </summary>
            <p>{move ||{let h=history.get();match (h.oldest_position,h.latest_position) { (Some(oldest),Some(latest)) => format!("Oldest retained position: {oldest}; latest: {latest}"), _ => "No recorded ticks yet.".to_owned() }}}
            </p>
            <ul>{move ||history.get().points.into_iter().map(|p|view!{
              <li>{format!("#{} {} {} sequence {}",p.event_position,p.symbol,money(p.price_cents),p.sequence)}
              </li>}).collect_view()}
            </ul>
          </details>
        </section>
        <div class="columns">
          <section>
            <h2>"Simulated order"
            </h2>
            <p>"Local operator fixture. No broker, real funds or external account is connected."
            </p>
            <form on:submit=submit>
              <label for="symbol">"Symbol"
              </label>
              <input id="symbol" disabled=move ||unresolved.get().is_some() maxlength="8" prop:value=move ||symbol.get() on:input=move |ev|set_symbol.set(event_target_value(&ev))/>
              <label for="quantity">"Whole units to buy"
              </label>
              <input id="quantity" disabled=move ||unresolved.get().is_some() type="number" min="1" max="1000" prop:value=move ||quantity.get() on:input=move |ev|set_quantity.set(event_target_value(&ev))/>
              <button type="submit" disabled=move ||pending.get()>{move ||if pending.get(){"Submitting…"}else if unresolved.get().is_some(){"Retry retained instruction"}else{"Submit simulated buy"}}
              </button>
            </form>
            <Show when=move ||unresolved.get().is_some()>
              <button on:click=reconcile.clone() disabled=move ||pending.get()>"Check exact ledger entry"
              </button>
              <p>{move ||unresolved.get().map(|d|format!("Retained instruction: {} · buy {} {}. Record this ID before leaving the page.",d.id,d.quantity,d.symbol)).unwrap_or_default()}
              </p>
            </Show>
            <p role="status">{move ||order_status.get()}
            </p>
          </section>
          <section>
            <h2>"Positions and watchlist"
            </h2>
            <ul>{move ||snapshot.get().unwrap_or_default().positions.into_iter().map(|p|view!{
              <li>{p.symbol}": "{p.quantity}" units"
              </li>}).collect_view()}
            </ul>
            <p>{move ||format!("Watching: {}",snapshot.get().unwrap_or_default().watchlist.join(", "))}
            </p>
          </section>
        </div>
        <section>
          <h2>"Edit watchlist"
          </h2>
          <form on:submit=update_watchlist>
            <label for="watchlist">"Comma-separated symbols"
            </label>
            <input id="watchlist" maxlength="143" prop:value=move||watch_input.get() on:input=move|ev|set_watch_input.set(event_target_value(&ev))/>
            <button disabled=move||command_pending.get()>"Save watchlist"
            </button>
          </form>
        </section>
        <section>
          <h2>"Durable replay jobs"
          </h2>
          <p>"Queue five synthetic LAB ticks. The single worker persists its cursor. Cancellation is cooperative; completed ticks remain committed."
          </p>
          <button on:click=schedule_replay disabled=move||command_pending.get()>{move || if replay_submission.get().job_id().is_some(){"Retry retained replay"}else{"Queue synthetic replay"}}
          </button>
          <p>{move || replay_submission.get().job_id().map(|id| format!("Retained replay ID: {id}. Its exact ticks remain in this open page. Do not reload while the outcome is uncertain.")).unwrap_or_default()}</p>
          <p role="status">{move||command_status.get()}
          </p>
          <ul>{move ||{let owner=Arc::clone(&cancel_alive);jobs.get().into_iter().map(|j|{let id=j.job_id.clone();let active=matches!(j.status.as_str(),"queued"|"running");let owner=Arc::clone(&owner);view!{
            <li>
              <span>{format!("{} · {}/{} · {} {}",j.job_id,j.next,j.total,j.status,j.error.unwrap_or_default())}
              </span>
              <button disabled=move||command_pending.get()||!active on:click=move |_|command("POST",format!("/api/jobs/{id}/cancel"),serde_json::json!({}),set_command_status,command_pending,set_command_pending,Arc::clone(&owner))>"Cancel job"
              </button>
            </li>}}).collect_view()}}
          </ul>
        </section>
        <section>
          <h2>"Recent simulated order ledger"
          </h2>
          <p>"Inspect IDs after an ambiguous outcome before submitting another instruction. The snapshot retains at most 100 recent orders; exact lookup works beyond that window. Unresolved identity is retained in this open page, not across a reload. Copy its ID before leaving."
          </p>
          <div class="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>"Order ID"
                  </th>
                  <th>"Symbol"
                  </th>
                  <th>"Side"
                  </th>
                  <th>"Units"
                  </th>
                  <th>"Fill price"
                  </th>
                </tr>
              </thead>
              <tbody>{move||snapshot.get().unwrap_or_default().orders.into_iter().map(|o|view!{
                <tr>
                  <td>{o.order_id}
                  </td>
                  <td>{o.symbol}
                  </td>
                  <td>{o.side}
                  </td>
                  <td>{o.quantity}
                  </td>
                  <td>{money(o.price_cents)}
                  </td>
                </tr>}).collect_view()}
              </tbody>
            </table>
          </div>
        </section>
      </main>
      <footer>"This dashboard is a teaching client. Its fixture identities are public. Bind services to localhost."
      </footer>
    }
}
// The timeout limits one request; no mutation is retried automatically.
async fn send_mutation(method: &str, path: &str, body: &serde_json::Value) -> Result<u16, String> {
    let controller = web_sys::AbortController::new()
        .map_err(|_| "Browser cancellation unavailable".to_string())?;
    let abort = controller.clone();
    let timer = gloo_timers::callback::Timeout::new(3000, move || abort.abort());
    let request = if method == "PUT" {
        gloo_net::http::Request::put(path)
    } else {
        gloo_net::http::Request::post(path)
    };
    let result = match request
        .header("Authorization", "Bearer demo-operator")
        .abort_signal(Some(&controller.signal()))
        .json(body)
    {
        Ok(r) => r
            .send()
            .await
            .map(|r| r.status())
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    drop(timer);
    result
}
// One command request is admitted at a time; errors never trigger automatic mutation retries.
fn command(
    method: &'static str,
    path: String,
    body: serde_json::Value,
    status: WriteSignal<String>,
    pending: ReadSignal<bool>,
    set_pending: WriteSignal<bool>,
    alive: Arc<AtomicBool>,
) {
    if pending.get_untracked() {
        return;
    }
    set_pending.set(true);
    leptos::task::spawn_local(async move {
        if !alive.load(Ordering::Relaxed) {
            return;
        }
        let result = send_mutation(method, &path, &body).await;
        if !alive.load(Ordering::Relaxed) {
            return;
        }
        status.set(match result {Ok(200..=299)=>"Accepted. The next refresh confirms persistent state.".into(),Ok(code @ (408 | 409 | 500..=599))=>format!("Outcome unknown (HTTP {code}). Inspect durable state before retrying."),Ok(code)=>format!("Not accepted: HTTP {code}. Inspect validation and authorization."),Err(_)=>"Outcome unknown. Inspect jobs/watchlist before retrying; no automatic retry was sent.".into()});
        set_pending.set(false);
    });
}
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    leptos::mount::mount_to_body(App);
}
