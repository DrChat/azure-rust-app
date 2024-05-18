use std::{net::SocketAddr, str::FromStr};

use axum::{
    extract::FromRef,
    response::IntoResponse,
    routing::{get, post},
    Form, Router, Server,
};
use axum_template::{engine::Engine, RenderHtml};
use tera::Tera;
use tower_http::{services::ServeDir, trace::TraceLayer};

use anyhow::Context;
use azure_core::auth::TokenCredential;
use azure_identity::ImdsManagedIdentityCredential;
use serde::Deserialize;

type AppEngine = Engine<Tera>;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Submit {
    name: String,
}

async fn hello(engine: AppEngine, Form(form): Form<Submit>) -> impl IntoResponse {
    RenderHtml(
        "hello.html.tera",
        engine,
        serde_json::json!({
            "name": form.name,
        }),
    )
}

async fn index(engine: AppEngine) -> impl IntoResponse {
    let creds = ImdsManagedIdentityCredential::default();
    let resp = creds.get_token("https://management.azure.com").await;

    let ident = match resp {
        Ok(_t) => format!("authenticated"),
        Err(e) => format!("unable to authenticate: {e:#}"),
    };

    RenderHtml(
        "index.html.tera",
        engine,
        serde_json::json!({
            "ident": ident
        }),
    )
}

#[derive(Clone, FromRef)]
struct AppState {
    engine: AppEngine,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let tera = Tera::new("templates/**/*").context("failed to initialize tera")?;

    let app = Router::new()
        .route("/", get(index))
        .route("/hello", post(hello))
        .nest_service("/static", ServeDir::new("./static"))
        .layer(TraceLayer::new_for_http())
        .with_state(AppState {
            engine: Engine::from(tera),
        });

    let addr = SocketAddr::from_str("0.0.0.0:8000").unwrap();
    tracing::info!("listening on {addr}");
    tracing::info!("connect to: http://127.0.0.1:{}", addr.port());

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .context("failed to serve app")
}
