use std::collections::HashMap;

use azure_core::auth::{AccessToken, TokenCredential};
use azure_identity::ImdsManagedIdentityCredential;

use anyhow::Context;
use chrono::{DateTime, Utc};
use rocket::{routes, serde::json::Json, Route};
use url::Url;

use serde::{Deserialize, Serialize};

/// Whether or not to fetch the event from an ADO API to verify it for security purposes.
///
/// Note that this app's identity will need permission to access the target ADO instance.
const SECURE_FETCH: bool = false;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Link {
    href: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Container {
    id: uuid::Uuid,
    base_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    text: String,
    html: String,
    markdown: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    id: uuid::Uuid,
    subscription_id: uuid::Uuid,
    notification_id: u64,
    event_type: String,
    publisher_id: String,
    message: Option<Message>,
    detailed_message: Option<Message>,
    resource: Option<serde_json::Value>,
    resource_version: Option<String>,
    resource_containers: HashMap<String, Container>,
    created_date: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFragment {
    id: uuid::Uuid,
    name: String,
    description: String,
    url: String,
    state: String,
    revision: u64,
    visibility: String,
    last_update_time: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionFragment {
    id: u64,
    name: String,
    /// API URL to query for more information about this pipeline definition
    url: String,
    uri: String,
    /// The folder path for this pipeline definition
    path: String,
    #[serde(rename = "type")]
    typ_: String,
    queue_status: String,
    revision: u64,
    project: ProjectFragment,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Build {
    /// Tags associated with a build
    tags: Vec<String>,
    /// Input parameters
    template_parameters: HashMap<String, serde_json::Value>,
    /// The run number
    id: u64,
    /// The API url for further information about the run itself
    url: Url,
    /// E.g: "20221202.1"
    build_number: String,
    /// The status string, e.g: "completed"
    status: String,
    /// Whether or not the run was successful. e.g: "succeeded"
    result: String,
    /// The time the run was request
    queue_time: DateTime<Utc>,
    /// The time the run actually started
    start_time: DateTime<Utc>,
    /// The time the run finished
    finish_time: DateTime<Utc>,
    /// Reason the build was initiated, e.g: "manual"
    reason: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildComplete {
    #[serde(rename = "_links")]
    links: HashMap<String, Link>,
    /// The fields that describe this build
    #[serde(flatten)]
    info: Build,
}

async fn build_complete(token: AccessToken, event: Event) -> anyhow::Result<()> {
    let event = serde_json::from_value::<BuildComplete>(
        event.resource.context("resource data not present")?,
    )
    .context("failed to decode resource")?;

    Ok(())
}

#[post("/ado", format = "json", data = "<event>")]
async fn ado(event: Json<Event>) -> Result<(), std::io::Error> {
    let f = || async {
        let creds = ImdsManagedIdentityCredential::default();
        let token = match creds.get_token("https://management.azure.com").await {
            Ok(t) => t.token,
            Err(e) => return Err(e).context("failed to query identity"),
        };

        let event = if SECURE_FETCH {
            // NOTE: For security purposes, we will want to pull the notification data from ADO directly:
            // https://learn.microsoft.com/en-us/rest/api/azure/devops/hooks/notifications/get?view=azure-devops-rest-7.1
            //
            // Hit this endpoint with `event.id` and `event.subscription_id`
            // N.B: `Uuid` is URL-safe, so simply including it in a URL will not allow for any unsafe attacker-controlled escaping.
            let url = format!("https://dev.azure.com/jusmoore/_apis/hooks/subscriptions/{}/notifications/{}?api-version=7.1-preview.1",
                            event.subscription_id, event.notification_id);

            let client = reqwest::Client::new();

            let resp = client
                .get(&url)
                .bearer_auth(token.secret())
                .send()
                .await
                .context("failed to fetch event")?;

            if resp.status() != 200 {
                anyhow::bail!("{}: code {}", url, resp.status().as_u16());
            }

            serde_json::from_str::<Event>(&resp.text().await.context("failed to download event")?)
                .context("failed to decode event")?
        } else {
            event.0
        };

        match event.event_type.as_str() {
            "build.complete" => build_complete(token, event).await,
            _ => Ok(()),
        }
    };

    // TODO: To assist in debugging, we could return the error message in the HTTP response.
    // ADO will capture the response and display it in its UI for later analysis.
    f().await.map_err(|e| std::io::Error::other(e))
}

pub fn routes() -> Vec<Route> {
    routes![ado]
}
