use axum::Router;

use crate::AppState;

pub mod ado;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().nest("/ado", ado::routes())
}
