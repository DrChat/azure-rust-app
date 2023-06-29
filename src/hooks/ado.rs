use std::{collections::HashMap, sync::Arc};

use axum::{extract::State, routing::post, Json, Router};

use anyhow::Context;
use azure_core::auth::{AccessToken, TokenCredential};
use chrono::{DateTime, Utc};
use tracing::info;
use url::Url;

use serde::{Deserialize, Serialize};

use crate::{AppState, Error};

/// Whether or not to fetch the event from an ADO API to verify it for security purposes.
///
/// Note that this app's identity will need permission to access the target ADO instance.
const SECURE_FETCH: bool = true;

/// The organization this hook is intended to access. Hardcoded for now, but can be made a config variable.
const ADO_ORGANIZATION: &str = "https://dev.azure.com/jusmoore";
/// Globally unique resource identifier for Azure DevOps.
/// AAD tokens must target this GUID as the "aud" (audience) field.
const ADO_RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6975798";

mod events {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Link {
        pub href: Url,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Container {
        pub id: uuid::Uuid,
        pub base_url: Option<String>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Message {
        pub text: String,
        pub html: String,
        pub markdown: String,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotificationDetails {
        pub event_type: String,
        /// The event data. Note that despite the ADO documentation indicating otherwise,
        /// this field is _in fact optional_.
        pub event: Option<Event>,
    }

    /// https://learn.microsoft.com/en-us/rest/api/azure/devops/hooks/notifications/get?view=azure-devops-rest-7.0#notification
    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Notification {
        pub id: u64,
        pub subscription_id: uuid::Uuid,
        pub subscriber_id: uuid::Uuid,
        pub event_id: uuid::Uuid,
        pub status: String,
        pub result: String,
        pub created_date: DateTime<Utc>,
        pub modified_date: DateTime<Utc>,
        pub details: NotificationDetails,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Event {
        pub id: uuid::Uuid,
        pub subscription_id: Option<uuid::Uuid>,
        pub notification_id: Option<u64>,
        pub event_type: String,
        pub publisher_id: String,
        pub message: Option<Message>,
        pub detailed_message: Option<Message>,
        pub resource: Option<serde_json::Value>,
        pub resource_version: Option<String>,
        pub resource_containers: HashMap<String, Container>,
        pub created_date: DateTime<Utc>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ProjectFragment {
        pub id: uuid::Uuid,
        pub name: String,
        pub description: String,
        pub url: String,
        pub state: String,
        pub revision: u64,
        pub visibility: String,
        pub last_update_time: DateTime<Utc>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DefinitionFragment {
        pub id: u64,
        pub name: String,
        /// API URL to query for more information about this pipeline definition
        pub url: Url,
        pub uri: String,
        /// The folder path for this pipeline definition
        pub path: String,
        #[serde(rename = "type")]
        pub typ_: String,
        pub queue_status: String,
        pub revision: u64,
        pub project: ProjectFragment,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Build {
        /// Tags associated with a build
        #[serde(default)]
        pub tags: Vec<String>,
        /// Input parameters
        #[serde(default)]
        pub template_parameters: HashMap<String, serde_json::Value>,
        /// The run number
        pub id: u64,
        /// The API url for further information about the run itself
        pub url: Url,
        /// E.g: "20221202.1"
        pub build_number: String,
        /// The status string, e.g: "completed"
        pub status: String,
        /// Whether or not the run was successful. e.g: "succeeded"
        pub result: String,
        /// The time the run was request
        pub queue_time: DateTime<Utc>,
        /// The time the run actually started
        pub start_time: DateTime<Utc>,
        /// The time the run finished
        pub finish_time: DateTime<Utc>,
        /// Reason the build was initiated, e.g: "manual"
        pub reason: String,
        // ... more fields omitted.
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct BuildComplete {
        #[serde(rename = "_links")]
        pub links: HashMap<String, Link>,
        /// The fields that describe this build
        #[serde(flatten)]
        pub info: Build,
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn notification_test() {
            // N.B: This data was sourced from the webhook test from ADO
            const NOTIFICATIONS: &[&str] = &[
                r#"{"id":41,"subscriptionId":"00000000-0000-0000-0000-000000000000","subscriberId":"00000000-0000-0000-0000-000000000000","eventId":"d6ac459c-18b3-44ff-95b5-b5f03db672ea","status":"processing","result":"pending","createdDate":"2023-06-30T15:24:41.38Z","modifiedDate":"2023-06-30T15:24:41.39Z","details":{"eventType":"build.complete","event":{"id":"d6ac459c-18b3-44ff-95b5-b5f03db672ea","eventType":"build.complete","publisherId":"tfs","message":{"text":"Build 20150407.2 succeeded","html":"Build <a href=\"https://fabrikam-fiber-inc.visualstudio.com/web/build.aspx?pcguid=5023c10b-bef3-41c3-bf53-686c4e34ee9e&amp;builduri=vstfs%3a%2f%2f%2fBuild%2fBuild%2f4\">20150407.2</a> succeeded","markdown":"Build [20150407.2](https://fabrikam-fiber-inc.visualstudio.com/web/build.aspx?pcguid=5023c10b-bef3-41c3-bf53-686c4e34ee9e&builduri=vstfs%3a%2f%2f%2fBuild%2fBuild%2f4) succeeded"},"detailedMessage":{"text":"Build 20150407.2 succeeded","html":"Build <a href=\"https://fabrikam-fiber-inc.visualstudio.com/web/build.aspx?pcguid=5023c10b-bef3-41c3-bf53-686c4e34ee9e&amp;builduri=vstfs%3a%2f%2f%2fBuild%2fBuild%2f4\">20150407.2</a> succeeded","markdown":"Build [20150407.2](https://fabrikam-fiber-inc.visualstudio.com/web/build.aspx?pcguid=5023c10b-bef3-41c3-bf53-686c4e34ee9e&builduri=vstfs%3a%2f%2f%2fBuild%2fBuild%2f4) succeeded"},"resource":{"_links":{"web":{"href":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/71777fbc-1cf2-4bd1-9540-128c1c71f766/_apis/build/Builds/1"}},"id":1,"buildNumber":"20150407.2","status":"completed","result":"succeeded","queueTime":"2015-04-07T17:22:56.22Z","startTime":"2015-04-07T17:23:02.4977418Z","finishTime":"2015-04-07T17:24:20.763574Z","url":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/71777fbc-1cf2-4bd1-9540-128c1c71f766/_apis/build/Builds/1","definition":{"id":1,"name":"CustomerAddressModule","url":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/71777fbc-1cf2-4bd1-9540-128c1c71f766/_apis/build/Definitions/1","type":"build","queueStatus":"enabled","revision":2,"project":{"id":"71777fbc-1cf2-4bd1-9540-128c1c71f766","name":"Git","url":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/_apis/projects/71777fbc-1cf2-4bd1-9540-128c1c71f766","state":"wellFormed","visibility":"unchanged","lastUpdateTime":"0001-01-01T00:00:00"}},"uri":"vstfs:///Build/Build/1","sourceBranch":"refs/heads/master","sourceVersion":"600C52D2D5B655CAA111ABFD863E5A9BD304BB0E","queue":{"id":1,"name":"default","pool":null},"priority":"normal","reason":"batchedCI","requestedFor":{"displayName":"Normal Paulk","url":"https://fabrikam-fiber-inc.visualstudio.com/_apis/Identities/d6245f20-2af8-44f4-9451-8107cb2767db","id":"d6245f20-2af8-44f4-9451-8107cb2767db","uniqueName":"fabrikamfiber16@hotmail.com","imageUrl":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/_api/_common/identityImage?id=d6245f20-2af8-44f4-9451-8107cb2767db"},"requestedBy":{"displayName":"Jamal Hartnett","url":"https://fabrikam-fiber-inc.visualstudio.com/_apis/Identities/b873e41d-7ebf-4e56-a3ce-ec582975baf6","id":"b873e41d-7ebf-4e56-a3ce-ec582975baf6","uniqueName":"Jamal.Hartnett@Fabrikamcloud.com","imageUrl":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/_api/_common/identityImage?id=b873e41d-7ebf-4e56-a3ce-ec582975baf6","isContainer":true},"lastChangedDate":"2015-04-07T17:24:20.883Z","lastChangedBy":{"displayName":"[DefaultCollection]\\Project Collection Service Accounts","url":"https://fabrikam-fiber-inc.visualstudio.com/_apis/Identities/b873e41d-7ebf-4e56-a3ce-ec582975baf6","id":"b873e41d-7ebf-4e56-a3ce-ec582975baf6","uniqueName":"vstfs:///Framework/Generic/5023c10b-bef3-41c3-bf53-686c4e34ee9e\\Project Collection Service Accounts","imageUrl":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/_api/_common/identityImage?id=b873e41d-7ebf-4e56-a3ce-ec582975baf6","isContainer":true},"orchestrationPlan":{"planId":"b67fddb8-8036-47cf-a472-61aa7d9b53e8"},"logs":{"id":0,"type":"Container","url":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/71777fbc-1cf2-4bd1-9540-128c1c71f766/_apis/build/builds/1/logs"},"repository":{"id":"47de4095-dda2-4d32-a8df-9812ae598179","type":"TfsGit","name":"JamalHartnettUserBranch","url":"https://fabrikam-fiber-inc.visualstudio.com/DefaultCollection/_apis/_git/71777fbc-1cf2-4bd1-9540-128c1c71f766","clean":null,"checkoutSubmodules":false},"triggeredByBuild":null,"appendCommitMessageToRunName":true},"resourceVersion":"2.0","resourceContainers":{"collection":{"id":"c12d0eb8-e382-443b-9f9c-c52cba5014c2"},"account":{"id":"f844ec47-a9db-4511-8281-8b63f4eaf94e"},"project":{"id":"be9b3917-87e6-42a4-a549-2bc06a7a878f"}},"createdDate":"2023-06-30T15:24:41.3517333Z"},"publisherId":"tfs","consumerId":"webHooks","consumerActionId":"httpRequest","consumerInputs":{"url":"https://backend.azurewebsites.net/hooks/ado/build","resourceDetailsToSend":"none","messagesToSend":"none","detailedMessagesToSend":"none"},"publisherInputs":{"definitionName":"","buildStatus":"","projectId":"b1e35496-5821-4030-8a8c-1f79ca026f02"},"request":"Method: POST\nURI: https://backend.azurewebsites.net/hooks/ado/build\nHTTP Version: 1.1\nHeaders:\n{\n  Content-Type: application/json; charset=utf-8\n}\nContent:\n{\n  \"subscriptionId\": \"00000000-0000-0000-0000-000000000000\",\n  \"notificationId\": 41,\n  \"id\": \"d6ac459c-18b3-44ff-95b5-b5f03db672ea\",\n  \"eventType\": \"build.complete\",\n  \"publisherId\": \"tfs\",\n  \"message\": null,\n  \"detailedMessage\": null,\n  \"resource\": null,\n  \"resourceVersion\": null,\n  \"resourceContainers\": {\n    \"collection\": {\n      \"id\": \"c12d0eb8-e382-443b-9f9c-c52cba5014c2\"\n    },\n    \"account\": {\n      \"id\": \"f844ec47-a9db-4511-8281-8b63f4eaf94e\"\n    },\n    \"project\": {\n      \"id\": \"be9b3917-87e6-42a4-a549-2bc06a7a878f\"\n    }\n  },\n  \"createdDate\": \"2023-06-30T15:24:41.3517333Z\"\n}\n","queuedDate":"2023-06-30T15:24:41.367Z","requestAttempts":1}}"#,
                r#"{"id":2,"subscriptionId":"213e4167-f493-4941-9b4c-bbb092d0b159","subscriberId":"00000000-0000-0000-0000-000000000000","eventId":"d6ac459c-18b3-44ff-95b5-b5f03db672ea","status":"processing","result":"pending","createdDate":"2023-06-30T16:19:52.033Z","modifiedDate":"2023-06-30T16:19:52.043Z","details":{"eventType":"build.complete","publisherId":"tfs","consumerId":"webHooks","consumerActionId":"httpRequest","consumerInputs":{"url":"https://backend.azurewebsites.net/hooks/ado/build","resourceDetailsToSend":"none","messagesToSend":"none","detailedMessagesToSend":"none"},"publisherInputs":{"definitionName":"","buildStatus":"","projectId":"b1e35496-5821-4030-8a8c-1f79ca026f02","tfsSubscriptionId":"a54b1fe3-e57b-4743-b651-df508e0d53d3"},"queuedDate":"2023-06-30T16:19:52.033Z","requestAttempts":1}}"#,
            ];

            for notification in NOTIFICATIONS {
                serde_json::from_str::<Notification>(notification)
                    .expect("failed to decode notification data");
            }
        }
    }
}

/// Process a _verified_ `build.complete` notification from ADO.
async fn build_complete(_token: &AccessToken, event: events::Event) -> anyhow::Result<()> {
    let _event = serde_json::from_value::<events::BuildComplete>(
        event.resource.context("resource data not present")?,
    )
    .context("failed to decode resource")?;

    // TODO: If the build corresponds to a scheduled production pipeline:
    // 1) File a bug in ADO, or ping someone via comment if a bug is already filed
    // 2) Send a request to ADO to retry the pipeline run

    Ok(())
}

/// Verify that an event has indeed originated from our target ADO instance.
async fn verify(token: &AccessToken, event: &events::Event) -> Result<events::Event, Error> {
    let notification_id = event
        .notification_id
        .context("event had no notification id")?;

    // NOTE: For security purposes, we will want to verify the notification data from ADO directly:
    // https://learn.microsoft.com/en-us/rest/api/azure/devops/hooks/notifications/get?view=azure-devops-rest-7.1
    //
    // Hit this endpoint with `event.id` and `event.subscription_id`
    // N.B: `Uuid` is URL-safe, so simply including it in a URL will not allow for any unsafe attacker-controlled escaping.
    let url = format!(
        "{}/_apis/hooks/subscriptions/{}/notifications/{}?api-version=7.1-preview.1",
        ADO_ORGANIZATION,
        event
            .subscription_id
            .context("event had no subscription id")?,
        notification_id
    );

    let client = reqwest::Client::new();

    let resp = client
        .get(&url)
        .bearer_auth(token.secret())
        .send()
        .await
        .context("failed to fetch notification data")?;

    if resp.status() != 200 {
        return Err(anyhow::anyhow!("{}: code {}", url, resp.status().as_u16()).into());
    }

    let text = resp
        .text()
        .await
        .context("failed to download notification data")?;
    let notif = serde_json::from_str::<events::Notification>(&text)
        .context(format!("failed to decode notification data: {text}"))?;

    // Verify some basic fields to ensure that ADO returns the same value as what was contained
    // within the notification.
    // No, we can't really do any better than this because they do not seem to offer better options.
    if notif.event_id != event.id || notif.id != notification_id || notif.status != "processing" {
        Err(anyhow::anyhow!("failed to verify event").into())
    } else {
        Ok(event.clone())
    }
}

/// Hook that gets invoked solely on `build.complete` events from ADO.
async fn build(
    State(creds): State<Arc<dyn TokenCredential>>,
    Json(event): Json<events::Event>,
) -> Result<(), Error> {
    if let Ok(event) = serde_json::to_string(&event) {
        info!("received event: {event}");
    }

    let token = creds
        .get_token(ADO_RESOURCE)
        .await
        .context("failed to query identity")
        .map(|t| t.token)?;

    // If no data was specified in the event, or we're operating with secure fetch
    // mode, ping the ADO instance to fetch the details.
    let event = if event.resource.is_none() || SECURE_FETCH {
        verify(&token, &event)
            .await
            .context("failed to verify event")?
    } else {
        event
    };

    match event.event_type.as_str() {
        "build.complete" => Ok(build_complete(&token, event).await?),
        _ => Ok(()),
    }
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/build", post(build))
}

#[cfg(test)]
mod test {}
