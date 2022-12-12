use axum::{
    async_trait,
    extract::{FromRequest, FromRef},
    http::{header::CONTENT_TYPE, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Json, RequestExt, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sigh::{PrivateKey, PublicKey, alg::{RsaSha256, Algorithm}, Key};
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod fetch;
pub use fetch::fetch;
mod send;
pub use send::send;
mod activitypub;
mod endpoint;

const ACTOR_ID: &str = "https://relay.fedi.buzz/actor";
const ACTOR_KEY: &str = "https://relay.fedi.buzz/actor#key";

#[derive(Debug, Clone)]
struct State {
    client: Arc<reqwest::Client>,
    private_key: PrivateKey,
    public_key: PublicKey,
}


impl FromRef<State> for Arc<reqwest::Client> {
    fn from_ref(state: &State) -> Arc<reqwest::Client> {
        state.client.clone()
    }
}

async fn actor(axum::extract::State(state): axum::extract::State<State>) -> Response {
    let id = ACTOR_ID.to_string();
    ([("content-type", "application/activity+json")],
     Json(activitypub::Actor {
         jsonld_context: json!([
             "https://www.w3.org/ns/activitystreams",
             "https://w3id.org/security/v1",
         ]),
         actor_type: "Service".to_string(),
         id: id.clone(),
         inbox: "https://relay.fedi.buzz/inbox".to_string(),
         // outbox: "https://relay.fedi.buzz/outbox".to_string(),
         public_key: activitypub::ActorPublicKey {
             id: ACTOR_KEY.to_string(),
             owner: Some(id.clone()),
             pem: state.public_key.to_pem().unwrap(),
         },
         preferredUsername: Some("buzzrelay".to_string()),
     })).into_response()
}

async fn handler(
    axum::extract::State(state): axum::extract::State<State>,
    endpoint: endpoint::Endpoint,
) -> Response {
    let action = match serde_json::from_value::<activitypub::Action<serde_json::Value>>(endpoint.payload.clone()) {
        Ok(action) => action,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            format!("Bad action: {:?}", e)
        ).into_response(),
    };
    dbg!(&action);
    
    if action.action_type == "Follow" {
        let private_key = state.private_key.clone();
        let client = state.client.clone();
        tokio::spawn(async move {
            let accept = activitypub::Action {
                action_type: "Accept".to_string(),
                actor: ACTOR_ID.to_string(),
                to: Some(endpoint.actor.id),
                object: Some(endpoint.payload),
            };
            dbg!(serde_json::to_string_pretty(&accept));
            send::send(
                client.as_ref(), &endpoint.actor.inbox,
                ACTOR_KEY,
                &private_key,
                accept,
            ).await
                .map_err(|e| tracing::error!("post: {}", e));
        });
        
        (StatusCode::CREATED,
         [("content-type", "application/activity+json")],
         "{}"
        ).into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Not a recognized request").into_response()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "buzzrelay=trace,tower_http=trace,axum=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (private_key, public_key) = RsaSha256.generate_keys().unwrap();

    let app = Router::new()
        .route("/actor", get(actor))
        .route("/relay", post(handler))
        .with_state(State {
            client: Arc::new(reqwest::Client::new()),
            private_key, public_key,
        });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}