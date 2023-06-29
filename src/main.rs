#![feature(io_error_other)]
use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{extract::FromRef, http::StatusCode, response::IntoResponse, Router, Server};
use tower_http::trace::TraceLayer;

use anyhow::Context;
use azure_core::auth::TokenCredential;
use azure_identity::{AutoRefreshingTokenCredential, DefaultAzureCredential};

use thiserror::Error;
use tracing::{info, warn};

mod hooks;

#[derive(Error)]
#[error(transparent)]
struct Error(#[from] anyhow::Error);

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redirect to anyhow::Error implementation
        self.0.fmt(f)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{:?}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", self.0)).into_response()
    }
}

#[derive(Clone, FromRef)]
struct AppState {
    creds: Arc<dyn TokenCredential>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false) // Clean up log lines in Azure interface
        .init();

    // Meh. Kind of jank. Waiting for https://github.com/Azure/azure-sdk-for-rust/issues/1228
    //
    // Need to wrap the credential provider with an automatic refresh doodad so we don't
    // constantly request new credentials for every API request.
    let cred = Arc::new(AutoRefreshingTokenCredential::new(Arc::new(
        DefaultAzureCredential::default(),
    )));
    match cred.get_token("https://management.azure.com").await {
        Ok(_) => {}
        // This is not fatal because the managed identity service takes some
        // time to start up after app startup, so we must tolerate errors here.
        Err(e) => warn!("failed to authenticate: {e:?}"),
    }

    let app = Router::new()
        .nest("/hooks", hooks::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(AppState {
            creds: cred.clone(),
        });

    let addr = SocketAddr::from_str("0.0.0.0:8000").unwrap();
    info!("listening on {addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .context("failed to serve app")
}
