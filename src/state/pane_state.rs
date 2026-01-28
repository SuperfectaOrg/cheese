mod ui;
mod commands;
mod state;
mod config;

use gtk4::prelude::*;
use gtk4::{glib, Application};
use cheese_core::security;
use std::process;

const APP_ID: &str = "org.ratos.cheese";

fn main() -> glib::ExitCode {
    if security::is_running_as_root() {
        eprintln!("ERROR: Cheese must not be run as root");
        eprintln!("Please run as a normal user");
        process::exit(1);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_| {
        load_css();
    });

    app.connect_activate(build_ui);

    app.run()
}

fn build_ui(app: &Application) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("cheese-worker")
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    let core = match cheese_core::CheeseCore::new() {
        Ok(core) => core,
        Err(e) => {
            eprintln!("Failed to initialize Cheese: {}", e);
            process::exit(1);
        }
    };

    let app_state = state::AppState::new(core, runtime);
    
    let window = ui::window::CheeseWindow::new(app, app_state);
    window.present();
}

fn load_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(include_str!("../resources/style.css"));

    gtk4::style_context_add_provider_for_display(
        &gdk4::Display::default().expect("Could not connect to display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
