pub mod app;
pub mod notifications;
#[cfg(feature = "runner")]
pub mod runner;
pub mod terminal;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use app::App;
    leptos::mount_to_body(App);
}
