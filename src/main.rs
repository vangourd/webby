#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use std::str::FromStr;
    use std::sync::Arc;
    use tower_http::services::ServeDir;
    use webby::app::App;
    use webby::terminal::relay::{
        list_runners_handler, runner_ws_handler, terminal_ws_handler, Registry, RunnerRegistry,
    };

    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://./webby.db".to_string());

    let opts = SqliteConnectOptions::from_str(&database_url)
        .expect("Invalid DATABASE_URL")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = sqlx::SqlitePool::connect_with(opts)
        .await
        .expect("Failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let conf = get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let site_root = leptos_options.site_root.clone().to_string();

    let registry: Registry = Arc::new(RunnerRegistry::new());

    let app = Router::new()
        .route("/ws/runner", axum::routing::get(runner_ws_handler))
        .route(
            "/ws/terminal/:runner_id",
            axum::routing::get(terminal_ws_handler),
        )
        .route("/api/runners", axum::routing::get(list_runners_handler))
        // Serve compiled JS/WASM/CSS before Leptos's /*any route can catch them
        .nest_service("/pkg", ServeDir::new(format!("{site_root}/pkg")))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let pool = pool.clone();
                let registry = registry.clone();
                move || {
                    provide_context(pool.clone());
                    provide_context(registry.clone());
                }
            },
            App,
        )
        // Serve public assets and return 404 for anything else
        .fallback_service(ServeDir::new(site_root))
        .layer(axum::Extension(registry))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
