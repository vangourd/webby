use crate::notifications::{NotificationBell, TestPushButton};
use crate::terminal::protocol::RunnerInfo;
use leptos::*;
use leptos_meta::*;
use leptos_router::*;

// ── Server fns ────────────────────────────────────────────────────────────────

#[server(ListRunners, "/api")]
pub async fn list_runners_fn() -> Result<Vec<RunnerInfo>, ServerFnError> {
    use crate::terminal::relay::Registry;

    let Some(registry) = use_context::<Registry>() else {
        return Err(ServerFnError::ServerError("registry unavailable".into()));
    };

    Ok(registry.list().await)
}

// ── Root ──────────────────────────────────────────────────────────────────────

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/webby.css"/>
        <Link rel="preconnect" href="https://fonts.googleapis.com"/>
        <Link rel="preconnect" href="https://fonts.gstatic.com" crossorigin=""/>
        <Link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Cinzel:wght@400;600&family=Lora:ital,wght@0,400;0,500;1,400&family=Courier+Prime&display=swap"/>
        <Title text="Webby"/>
        <Router>
            <Routes>
                <Route path="/" view=Layout>
                    <Route path="" view=Home/>
                    <Route path="agents" view=AgentList/>
                    <Route path="agents/:id" view=AgentConsole/>
                </Route>
            </Routes>
        </Router>
    }
}

// ── Shell layout ─────────────────────────────────────────────────────────────

#[component]
fn Layout() -> impl IntoView {
    let (drawer_open, set_drawer_open) = create_signal(false);

    view! {
        <div class="app-shell">
            <header class="topbar">
                <A href="/" class="topbar__brand">"Webby"</A>
                <div class="topbar__actions">
                    <NotificationBell/>
                    <button class="hamburger" on:click=move |_| set_drawer_open.set(true)>"☰"</button>
                </div>
            </header>
            <main class="main-content">
                <Outlet/>
            </main>
            <div
                class=move || if drawer_open.get() {
                    "drawer-backdrop drawer-backdrop--open"
                } else {
                    "drawer-backdrop"
                }
                on:click=move |_| set_drawer_open.set(false)
            />
            <nav class=move || if drawer_open.get() { "drawer drawer--open" } else { "drawer" }>
                <div class="drawer__header">
                    <span class="drawer__title">"Menu"</span>
                    <button class="drawer__close" on:click=move |_| set_drawer_open.set(false)>"✕"</button>
                </div>
                <A href="/agents" class="drawer__item" on:click=move |_| set_drawer_open.set(false)>
                    "Agents"
                </A>
                <button class="drawer__item drawer__item--logout">"Logout"</button>
                <div class="drawer__footer">
                    <TestPushButton/>
                </div>
            </nav>
        </div>
    }
}

// ── Home ─────────────────────────────────────────────────────────────────────

#[component]
fn Home() -> impl IntoView {
    view! {
        <div class="empty-state">
            <h2>"Webby"</h2>
            <p>"Agent orchestration and terminal access."</p>
            <A href="/agents" class="btn btn--primary">"View Agents"</A>
        </div>
    }
}

// ── Agent list ────────────────────────────────────────────────────────────────

#[component]
fn AgentList() -> impl IntoView {
    let runners = create_resource(|| (), |_| list_runners_fn());

    view! {
        <div class="agent-list-page">
            <div class="page-header">
                <h2 class="page-title">"Agents"</h2>
                <button class="btn btn--ghost" on:click=move |_| runners.refetch()>"Refresh"</button>
            </div>
            <div class="runner-list">
                <Suspense fallback=move || view! { <p class="list-status">"Loading…"</p> }>
                    {move || runners.get().map(|res| match res {
                        Err(_) => view! {
                            <p class="list-status list-status--error">"Failed to load runners."</p>
                        }.into_view(),
                        Ok(list) if list.is_empty() => view! {
                            <div class="runner-empty">
                                <p>"No runners connected."</p>
                                <p class="runner-empty__hint">
                                    "Start one with: "
                                    <code>"webby-runner --server ws://localhost:8080 --name my-machine"</code>
                                </p>
                            </div>
                        }.into_view(),
                        Ok(list) => list.into_iter().map(|r| {
                            let href = format!("/agents/{}", r.runner_id);
                            let name = r.name.clone();
                            let id_short = format!("{}…", &r.runner_id[..8]);
                            view! {
                                <A href=href class="runner-card">
                                    <span class="agent-dot agent-dot--running"/>
                                    <div class="runner-card__info">
                                        <span class="runner-card__name">{name}</span>
                                        <span class="runner-card__id">{id_short}</span>
                                    </div>
                                    <span class="runner-card__arrow">"›"</span>
                                </A>
                            }
                        }).collect_view(),
                    })}
                </Suspense>
            </div>
        </div>
    }
}

// ── Agent console ─────────────────────────────────────────────────────────────

#[component]
fn AgentConsole() -> impl IntoView {
    let params = use_params_map();
    let runner_id = move || params.with(|p| p.get("id").cloned().unwrap_or_default());

    view! {
        <div class="console-page">
            <div class="console-header">
                <A href="/agents" class="btn btn--ghost btn--sm">"‹ Back"</A>
                <span class="agent-dot agent-dot--running"/>
                <h2 class="console-title">{runner_id}</h2>
            </div>
            <div class="console-terminal">
                <p class="console-placeholder">"[ terminal ]"</p>
            </div>
        </div>
    }
}
