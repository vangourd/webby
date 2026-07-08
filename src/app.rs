use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/webby.css"/>
        <Title text="Webby"/>
        <Router>
            <main>
                <Routes>
                    <Route path="/" view=HomePage/>
                    <Route path="/*any" view=NotFound/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <h1>"Webby"</h1>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        let resp = use_context::<leptos_axum::ResponseOptions>();
        if let Some(resp) = resp {
            resp.set_status(axum::http::StatusCode::NOT_FOUND);
        }
    }

    view! {
        <h1>"404 — Not Found"</h1>
    }
}
