use serde::Deserialize;
use axum::extract::State;
use axum::Json;
use axum::{routing::post, Router};
use ngrok::prelude::*;
use teloxide::Bot;
use teloxide::requests::Requester;
use std::sync::Arc;
use teloxide::types::UserId;

#[derive(Deserialize, Debug)]
// See https://docs.docker.com/docker-hub/webhooks/#example-webhook-payload
struct DockerPush {
    pub push_data: DockerPushDetails,
}

#[derive(Deserialize, Debug)]
struct DockerPushDetails {
    pub tag: String,
}

async fn react_to_dockerhub_message(
    State((super_user_id, bot)): State<(UserId, Arc<Bot>)>,
    Json(DockerPush {push_data: DockerPushDetails { tag, .. }, ..}): Json<DockerPush>
) -> (axum::http::StatusCode, Json<()>) {
    tracing::info!("New ambrog.io version has been released: {:?}", tag);

    if tag.eq_ignore_ascii_case("latest") || tag.is_empty() {
        return (axum::http::StatusCode::NO_CONTENT, Json(()))
    }

    let _ = bot.send_message(super_user_id, format!("Signore, adesso può aggiornarmi alla versione {tag}")).await;
    
    (axum::http::StatusCode::CREATED, Json(()))
}

pub async fn run_embedded_web_listener(bot: Bot, super_user_id: UserId) -> Result<(), String> {
    let app = Router::new()
        .route("/ambrogio_updates", post(react_to_dockerhub_message))
        .with_state((super_user_id, Arc::new(bot)));

    let listener = ngrok::Session::builder()
        .authtoken_from_env()
        .connect()
        .await
        .map_err(|e| e.to_string())?
        .http_endpoint()
        .domain("badly-refined-roughy.ngrok-free.app")
        .listen()
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("Ingress URL: {:?}", listener.url());

    axum::Server::builder(listener)
        .serve(app.into_make_service())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
