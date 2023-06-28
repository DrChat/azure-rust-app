use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    body::StreamBody,
    extract::{DefaultBodyLimit, FromRef, Multipart, Path, State},
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Form, Router, Server,
};
use axum_template::{engine::Engine, RenderHtml};
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::{ClientBuilder, ContainerClient};
use futures::TryStreamExt;
use tera::Tera;
use thiserror::Error;
use tower_http::{services::ServeDir, trace::TraceLayer};

use anyhow::Context;
use azure_core::auth::TokenCredential;
use azure_identity::{AutoRefreshingTokenCredential, DefaultAzureCredential};
use serde::Deserialize;
use tracing::log::warn;

type AppEngine = Engine<Tera>;

#[derive(Debug, Error)]
#[error(transparent)]
pub struct Error(#[from] anyhow::Error);

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", self.0)).into_response()
    }
}

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

async fn index(
    State(creds): State<Arc<dyn TokenCredential>>,
    engine: AppEngine,
) -> impl IntoResponse {
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

async fn download(
    State(container_client): State<ContainerClient>,
    Path(path): Path<String>,
) -> Result<impl IntoResponse, Error> {
    let blob = container_client.blob_client(&path);

    let props = blob
        .get_properties()
        .await
        .context("failed to get blob properties")?;
    let body = blob.get().into_stream().map_ok(|r| r.data).try_flatten();

    // Return an image/webp response.
    Ok(Response::builder()
        .header("Content-Type", &props.blob.properties.content_type)
        .header(
            "Content-Length",
            &props.blob.properties.content_length.to_string(),
        )
        .body(StreamBody::new(body))
        .context("could not build response: {e}")?
        .into_response())
}

async fn upload(
    State(container_client): State<ContainerClient>,
    mut files: Multipart,
) -> Result<impl IntoResponse, Error> {
    while let Some(file) = files.next_field().await.unwrap() {
        let content_type = file.content_type().unwrap().to_string();
        let _name = file.name().unwrap().to_string();
        let filename = file.file_name().unwrap().to_string();

        let bytes = file.bytes().await.context("failed to get file bytes")?;
        let blob_client = container_client.blob_client(filename.as_str());

        let _r = blob_client
            .put_block_blob(bytes) // TODO: https://github.com/Azure/azure-sdk-for-rust/issues/1219
            .content_type(content_type)
            .await
            .context("failed to put block blob")?;
    }

    Ok("ok")
}

#[derive(Clone, FromRef)]
struct AppState {
    engine: AppEngine,
    creds: Arc<dyn TokenCredential>,
    container_client: ContainerClient,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let tera = Tera::new("templates/**/*").context("failed to initialize tera")?;

    // Meh. Kind of jank. Waiting for https://github.com/Azure/azure-sdk-for-rust/issues/1228
    //
    // Need to wrap the credential provider with an automatic refresh doodad so we don't
    // constantly request new credentials for every API request.
    let cred = Arc::new(AutoRefreshingTokenCredential::new(Arc::new(
        DefaultAzureCredential::default(),
    )));
    match cred.get_token("https://management.azure.com").await {
        Ok(_) => {}
        Err(e) => warn!("failed to authenticate: {e:?}"),
    }

    let account = std::env::var("STORAGE_ACCOUNT").expect("missing STORAGE_ACCOUNT");
    let container = std::env::var("STORAGE_CONTAINER").expect("missing STORAGE_CONTAINER");
    let container_client =
        ClientBuilder::new(account, StorageCredentials::token_credential(cred.clone()))
            .container_client(&container);

    let app = Router::new()
        .route("/", get(index))
        .route("/hello", post(hello))
        .route("/download/:file", get(download))
        .route("/upload", put(upload))
        .nest_service("/static", ServeDir::new("./static"))
        .layer(TraceLayer::new_for_http())
        .layer(DefaultBodyLimit::max(1024 * 1024 * 512))
        .with_state(AppState {
            engine: Engine::from(tera),
            creds: cred.clone(),
            container_client,
        });

    let addr = SocketAddr::from_str("0.0.0.0:8000").unwrap();
    tracing::info!("listening on {addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .context("failed to serve app")
}
