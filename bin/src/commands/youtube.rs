use std::{collections::HashMap, env, path::PathBuf, sync::Arc};

use crate::telegram::TelegramProxy;
use ambrogio_users::data::UserId;
use async_process::Command;
use async_trait::async_trait;
use redis::AsyncCommands;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use tokio::fs;
use url::Url;

use super::{InboundMessage, MessageHandler};

pub struct YoutubeDownloadHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    client: Client,
    redis: redis::aio::MultiplexedConnection,
    regex: Regex,
}

impl YoutubeDownloadHandler {
    pub fn new<Proxy>(
        telegram: Arc<Proxy>,
        redis: redis::aio::MultiplexedConnection,
        client: &Client,
    ) -> Self
    where
        Proxy: TelegramProxy + Send + Sync + 'static,
    {
        Self {
            telegram,
            client: client.clone(),
            redis,
            regex: Regex::new(r"(?i)^(video|audio)?\s+[^\s]+").unwrap(),
        }
    }

    async fn download_video(&self, id: UserId, video_id: String) -> Result<(), String> {
        tokio::spawn({
            let telegram = self.telegram.clone();
            let client = self.client.clone();
            let mut redis = self.redis.clone();
            async move {
                let output = format!("{}.mp4", video_id);
                let path = env::current_dir().unwrap().join("storage").join(&output);
                let key: String = format!("video:{video_id}");

                let url = match redis.get::<String, Option<String>>(key.clone()).await {
                    Ok(Some(url)) => {
                        tracing::info!("Hit found for key {key}: {url}");
                        url
                    }
                    _ => {
                        tracing::info!("No value for key {key}, downloading into {output}.");

                        if !matches!(fs::try_exists(&path).await, Ok(true)) {
                            let download = Command::new("yt-dlp")
                                .arg("-f")
                                .arg("bestvideo[vcodec^=avc]+bestaudio[ext=m4a]/best[ext=mp4]/best")
                                .arg(&video_id)
                                .arg("-o")
                                .arg(path.clone().to_str().unwrap())
                                .output()
                                .await;

                            if let Err(e) = download {
                                tracing::error!("Unable to download {video_id}: {e}");
                                telegram
                                    .send_text_to_user(
                                        format!("Unable to download {video_id}: {e}"),
                                        id,
                                    )
                                    .await
                                    .unwrap();
                                return;
                            }
                        }

                        match upload_file(client, path.clone()).await {
                            Ok(url) => url.to_string(),
                            Err(e) => {
                                tracing::error!("Unable to upload {video_id}: {e}");
                                telegram
                                    .send_text_to_user(
                                        format!("Unable to upload {video_id}: {e}"),
                                        id,
                                    )
                                    .await
                                    .unwrap();
                                return;
                            }
                        }
                    }
                };

                let _ = redis
                    .set::<String, String, String>(key.clone(), url.clone())
                    .await;
                let _ = redis.expire::<String, String>(key, 7 * 24 * 60 * 60).await;
                let _ = telegram
                    .send_text_to_user(format!("🎥 `{video_id}`: {url}"), id)
                    .await;
            }
        });

        Ok(())
    }

    async fn download_audio(&self, id: UserId, video_id: String) -> Result<(), String> {
        tokio::spawn({
            let telegram = self.telegram.clone();
            let client = self.client.clone();
            let mut redis = self.redis.clone();
            async move {
                let output = format!("{}.mp3", video_id);
                let path = env::current_dir().unwrap().join("storage").join(&output);
                let key: String = format!("audio:{video_id}");

                let url = match redis.get::<String, Option<String>>(key.clone()).await {
                    Ok(Some(url)) => {
                        tracing::info!("Hit found for key {key}: {url}");
                        url
                    }
                    _ => {
                        tracing::info!("No value for key {key}, downloading into {output}.");

                        if !matches!(fs::try_exists(&path).await, Ok(true)) {
                            let download = Command::new("yt-dlp")
                                .arg("-x")
                                .arg("--audio-format")
                                .arg("mp3")
                                .arg("--audio-quality")
                                .arg("0")
                                .arg(&video_id)
                                .arg("-o")
                                .arg(path.clone().to_str().unwrap())
                                .output()
                                .await;

                            if let Err(e) = download {
                                tracing::error!("Unable to download {video_id}: {e}");
                                telegram
                                    .send_text_to_user(
                                        format!("Unable to download {video_id}: {e}"),
                                        id,
                                    )
                                    .await
                                    .unwrap();
                                return;
                            }
                        }

                        match upload_file(client, path.clone()).await {
                            Ok(url) => url.to_string(),
                            Err(e) => {
                                tracing::error!("Unable to upload {video_id}: {e}");
                                telegram
                                    .send_text_to_user(
                                        format!("Unable to upload {video_id}: {e}"),
                                        id,
                                    )
                                    .await
                                    .unwrap();
                                return;
                            }
                        }
                    }
                };

                let _ = redis
                    .set::<String, String, String>(key.clone(), url.clone())
                    .await;
                let _ = redis.expire::<String, String>(key, 7 * 24 * 60 * 60).await;
                let _ = telegram
                    .send_text_to_user(format!("🎵 `{video_id}`: {url}"), id)
                    .await;
            }
        });

        Ok(())
    }
}

#[async_trait]
impl MessageHandler for YoutubeDownloadHandler {
    fn can_accept(&self, InboundMessage { text, .. }: &InboundMessage) -> bool {
        self.regex.is_match(text)
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let id = user.id();
        let pieces = text
            .split(' ')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>();
        let command = pieces[0];
        let video = pieces[1];

        let video_id = match Url::parse(video) {
            Ok(url) => url
                .query_pairs()
                .into_iter()
                .collect::<HashMap<_, _>>()
                .get("v")
                .map(|c| c.clone().into_owned())
                .unwrap_or(video.to_owned()),
            _ => video.to_owned(),
        };

        self.telegram
            .send_text_to_user(format!("Sto scaricando {} {}", command, video_id), id)
            .await?;

        match command {
            "audio" => return self.download_audio(id, video_id).await,
            _ => return self.download_video(id, video_id).await,
        };
    }
}

async fn upload_file(client: Client, path: PathBuf) -> Result<Url, String> {
    let url = Url::parse("https://api.gofile.io/getServer").map_err(|err| format!("{err}"))?;
    let file_name = path
        .file_name()
        .and_then(|x| x.to_str())
        .map(|x| x.to_owned())
        .unwrap_or("somefile".to_owned());

    let request = client.get(url).build().map_err(|err| format!("{err}"))?;

    let server = client
        .execute(request)
        .await
        .map_err(|err| format!("{err}"))?
        .json::<GetServer>()
        .await
        .map_err(|err| format!("{err}"))?
        .data
        .server;

    let file = fs::read(path.clone())
        .await
        .map_err(|err| format!("{err}"))?;
    let file_part = reqwest::multipart::Part::bytes(file).file_name(file_name);
    let form = reqwest::multipart::Form::new().part("file", file_part);
    let upload_request = client
        .post(Url::parse(&format!("https://{server}.gofile.io/uploadFile")).unwrap())
        .multipart(form)
        .build()
        .map_err(|err| format!("{err}"))?;

    client
        .execute(upload_request)
        .await
        .map_err(|err| format!("{err}"))?
        .json::<UploadFile>()
        .await
        .map(|result| result.data.download_page)
        .map_err(|err| format!("{err}"))
}

#[derive(Deserialize)]
struct GetServer {
    pub data: Server,
}

#[derive(Deserialize)]
struct Server {
    pub server: String,
}

#[derive(Deserialize)]
struct UploadFile {
    pub data: Upload,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Upload {
    pub download_page: Url,
}
