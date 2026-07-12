use leptos::*;
use serde::{Deserialize, Serialize};

// ── Shared types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushSubscriptionPayload {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

// ── Server functions ──────────────────────────────────────────────────────────

/// Returns the base64url-encoded VAPID public key for client subscription.
#[server(GetVapidPublicKey, "/api")]
pub async fn get_vapid_public_key() -> Result<String, ServerFnError> {
    std::env::var("VAPID_PUBLIC_KEY")
        .map_err(|_| ServerFnError::ServerError("VAPID_PUBLIC_KEY not configured".into()))
}

/// Stores a push subscription endpoint in the database.
#[server(SavePushSubscription, "/api")]
pub async fn save_push_subscription(
    payload: PushSubscriptionPayload,
) -> Result<(), ServerFnError> {
    let Some(pool) = use_context::<sqlx::SqlitePool>() else {
        return Err(ServerFnError::ServerError("no db pool".to_string()));
    };

    sqlx::query(
        "INSERT OR REPLACE INTO push_subscriptions (endpoint, p256dh, auth) VALUES (?, ?, ?)",
    )
    .bind(payload.endpoint)
    .bind(payload.p256dh)
    .bind(payload.auth)
    .execute(&pool)
    .await?;

    Ok(())
}

/// Sends a test push notification to all subscribers. Dev only.
#[server(TriggerTestPush, "/api")]
pub async fn trigger_test_push() -> Result<(), ServerFnError> {
    let Some(pool) = use_context::<sqlx::SqlitePool>() else {
        return Err(ServerFnError::ServerError("no db pool".to_string()));
    };
    send_push_notification(&pool, "Webby", "Test notification 🔔", None)
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
}

// ── Server-side send utility ──────────────────────────────────────────────────

/// Sends a push notification to all stored subscribers.
/// Call this from any server function when you have an event to notify about.
///
/// Requires VAPID_PRIVATE_KEY env var (PEM format).
#[cfg(feature = "ssr")]
pub async fn send_push_notification(
    pool: &sqlx::SqlitePool,
    title: &str,
    body: &str,
    url: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Cursor;
    use web_push::{
        ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
        WebPushClient, WebPushMessageBuilder,
    };

    let private_key_pem = std::env::var("VAPID_PRIVATE_KEY")?;

    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT endpoint, p256dh, auth FROM push_subscriptions",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let payload_json = serde_json::json!({
        "title": title,
        "body": body,
        "data": url.map(|u| serde_json::json!({ "url": u })),
    })
    .to_string();

    let client = IsahcWebPushClient::new()?;

    for (endpoint, p256dh, auth) in rows {
        let sub_info = SubscriptionInfo::new(&endpoint, &p256dh, &auth);

        let mut builder = WebPushMessageBuilder::new(&sub_info);
        builder.set_payload(ContentEncoding::Aes128Gcm, payload_json.as_bytes());
        builder.set_ttl(3600);

        let sig = VapidSignatureBuilder::from_pem(
            Cursor::new(private_key_pem.as_bytes()),
            &sub_info,
        )?
        .build()?;
        builder.set_vapid_signature(sig);

        // Ignore per-subscriber errors (stale subscriptions are expected).
        let _ = client.send(builder.build()?).await;
    }

    Ok(())
}

// ── Component ─────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum BellState {
    Idle,
    Pending,
    Subscribed,
    Denied,
    Error,
}

#[component]
pub fn NotificationBell() -> impl IntoView {
    let (state, set_state) = create_signal(BellState::Idle);

    let handle_click = move |_| {
        if state.get() == BellState::Pending || state.get() == BellState::Subscribed {
            return;
        }
        set_state.set(BellState::Pending);

        #[cfg(feature = "hydrate")]
        {
            let set_state = set_state.clone();
            leptos::spawn_local(async move {
                match subscribe_push().await {
                    Ok(()) => set_state.set(BellState::Subscribed),
                    Err(e) if e.contains("denied") => set_state.set(BellState::Denied),
                    Err(_) => set_state.set(BellState::Error),
                }
            });
        }
    };

    view! {
        <button
            class=move || {
                let mut c = String::from("notification-bell");
                match state.get() {
                    BellState::Subscribed => c.push_str(" notification-bell--active"),
                    BellState::Denied | BellState::Error => c.push_str(" notification-bell--denied"),
                    _ => {}
                }
                c
            }
            disabled=move || state.get() == BellState::Pending
            title=move || match state.get() {
                BellState::Subscribed => "Notifications enabled",
                BellState::Denied => "Notifications blocked — check browser settings",
                BellState::Error => "Subscription failed",
                BellState::Pending => "Requesting permission…",
                BellState::Idle => "Enable notifications",
            }
            on:click=handle_click
        >
            {move || match state.get() {
                BellState::Subscribed => "🔔",
                BellState::Denied | BellState::Error => "🔕",
                _ => "🔔",
            }}
        </button>
    }
}

#[component]
pub fn TestPushButton() -> impl IntoView {
    let send = create_action(|_: &()| trigger_test_push());

    view! {
        <button
            class="btn btn--ghost"
            disabled=move || send.pending().get()
            on:click=move |_| { send.dispatch(()); }
        >
            {move || if send.pending().get() { "Sending…" } else { "Test push" }}
        </button>
    }
}

// ── Client-side subscription flow (WASM only) ─────────────────────────────────

#[cfg(feature = "hydrate")]
async fn subscribe_push() -> Result<(), String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Notification, PushSubscriptionOptionsInit};

    let vapid_key_b64 = get_vapid_public_key()
        .await
        .map_err(|e| e.to_string())?;

    let vapid_bytes = URL_SAFE_NO_PAD
        .decode(&vapid_key_b64)
        .map_err(|e| format!("invalid VAPID key: {e}"))?;

    let window = web_sys::window().ok_or("no window")?;

    // Ask for notification permission.
    let perm_promise =
        Notification::request_permission().map_err(|e| format!("permission: {e:?}"))?;
    let perm = JsFuture::from(perm_promise)
        .await
        .map_err(|e| format!("permission await: {e:?}"))?;
    if perm.as_string().as_deref() != Some("granted") {
        return Err("denied".into());
    }

    // Register the service worker (idempotent) then wait until it is active.
    let sw_container = window.navigator().service_worker();
    JsFuture::from(sw_container.register("/sw.js"))
        .await
        .map_err(|e| format!("SW register: {e:?}"))?;
    let ready_promise = sw_container.ready().map_err(|e| format!("SW ready: {e:?}"))?;
    let reg = JsFuture::from(ready_promise)
        .await
        .map_err(|e| format!("SW ready await: {e:?}"))?;
    let reg: web_sys::ServiceWorkerRegistration =
        reg.dyn_into().map_err(|_| "not a ServiceWorkerRegistration")?;

    // Build subscription options with the VAPID public key.
    let key_arr = js_sys::Uint8Array::from(vapid_bytes.as_slice());
    let opts = PushSubscriptionOptionsInit::new();
    opts.set_application_server_key_opt_u8_array(Some(&key_arr));
    opts.set_user_visible_only(true);

    // Subscribe.
    let push_manager = reg
        .push_manager()
        .map_err(|_| "PushManager unavailable")?;
    let sub = JsFuture::from(
        push_manager
            .subscribe_with_options(&opts)
            .map_err(|e| format!("subscribe: {e:?}"))?,
    )
    .await
    .map_err(|e| format!("subscribe await: {e:?}"))?;
    let sub: web_sys::PushSubscription =
        sub.dyn_into().map_err(|_| "not a PushSubscription")?;

    // Extract endpoint + keys from the subscription's JSON representation.
    let json_val = sub.to_json().map_err(|e| format!("to_json: {e:?}"))?;
    let json_str = js_sys::JSON::stringify(json_val.as_ref())
        .map_err(|_| "JSON.stringify failed")?
        .as_string()
        .ok_or("JSON.stringify returned non-string")?;

    #[derive(Deserialize)]
    struct SubJson {
        endpoint: String,
        keys: SubKeys,
    }
    #[derive(Deserialize)]
    struct SubKeys {
        p256dh: String,
        auth: String,
    }

    let parsed: SubJson =
        serde_json::from_str(&json_str).map_err(|e| format!("parse subscription JSON: {e}"))?;

    save_push_subscription(PushSubscriptionPayload {
        endpoint: parsed.endpoint,
        p256dh: parsed.keys.p256dh,
        auth: parsed.keys.auth,
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}
