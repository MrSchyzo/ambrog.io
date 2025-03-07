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
            regex: Regex::new(r"(?i)^(video|audio)(\s+[^\s]+)+").unwrap(),
        }
    }

    fn slugified_name(str: &str) -> String {
        let regex = Regex::new(r"\w|[-_ !.&]").expect("Invalid regex pattern");
        str.chars()
            .map(|x| {
                if regex.is_match(&x.to_string()) {
                    x
                } else {
                    '_'
                }
            })
            .collect()
    }

    async fn download_video(
        &self,
        id: UserId,
        video_id: String,
        target_name: Option<String>,
    ) -> Result<(), String> {
        let command = DownloadCommand::Video {
            id,
            video_id,
            target_name,
        };

        self.download_and_upload(command).await
    }

    async fn download_audio(
        &self,
        id: UserId,
        video_id: String,
        target_name: Option<String>,
    ) -> Result<(), String> {
        let command = DownloadCommand::Audio {
            id,
            video_id,
            target_name,
        };

        self.download_and_upload(command).await
    }

    async fn download_and_upload(&self, command: DownloadCommand) -> Result<(), String> {
        tokio::spawn({
            let telegram = self.telegram.clone();
            let client = self.client.clone();
            let mut redis = self.redis.clone();

            async move {
                let id = command.user_id();
                let video_id = command.video_id();
                let download_name = command.file_name(video_id);
                let key = command.caching_key();
                let download_path = env::current_dir()
                    .unwrap()
                    .join("storage")
                    .join(download_name.clone());
                let should_remember = command.target_name().is_none();

                let url = match (
                    redis.get::<String, Option<String>>(key.clone()).await,
                    should_remember,
                ) {
                    (Ok(Some(url)), true) => {
                        tracing::info!("Hit found for key {key}: {url}");
                        url
                    }
                    _ => {
                        match should_remember {
                            true => tracing::info!("No value for key {key}, downloading into {download_name}."),
                            false => tracing::info!("Caching is disabled for {key} when custom name is provided. Downloading into {download_name}.")
                        }

                        if !matches!(fs::try_exists(&download_path).await, Ok(true)) {
                            tracing::info!("Downloading {video_id} into {download_path:?}");
                            let download = command
                                .into_download_command(download_path.to_str().unwrap())
                                .output()
                                .await;

                            match download {
                                Err(e) => {
                                    tracing::error!("Unable to download {video_id}: {e}");
                                    telegram
                                        .send_text_to_user(
                                            format!("Unable to download {video_id}: {e}"),
                                            *id,
                                        )
                                        .await
                                        .unwrap();
                                    return;
                                }
                                Ok(std::process::Output {
                                    stdout,
                                    stderr,
                                    status,
                                }) => {
                                    if !status.success() {
                                        let out = String::from_utf8(stdout)
                                            .ok()
                                            .unwrap_or("N/A".to_owned());
                                        let err = String::from_utf8(stderr)
                                            .ok()
                                            .unwrap_or("N/A".to_owned());
                                        tracing::error!(
                                            "Output of {video_id}: {out}\n===============\n{err}"
                                        );
                                        telegram
                                            .send_text_to_user(
                                                format!("Download of {video_id} failed:\n\n{err}"),
                                                *id,
                                            )
                                            .await
                                            .unwrap();
                                        return;
                                    }
                                    tracing::info!("Downloaded {video_id} into {download_path:?}");
                                }
                            }
                        }

                        let title = match command.target_name() {
                            Some(s) => command.file_name(s),
                            None => {
                                let title = get_title(video_id).await;
                                command.file_name(&title)
                            }
                        };
                        match upload_file(client, download_path, &title).await {
                            Ok(url) => url.to_string(),
                            Err(e) => {
                                tracing::error!("Unable to upload {video_id}: {e}");
                                telegram
                                    .send_text_to_user(
                                        format!("Unable to upload {video_id}: {e}"),
                                        *id,
                                    )
                                    .await
                                    .unwrap();
                                return;
                            }
                        }
                    }
                };

                if !should_remember {
                    tracing::info!("Avoid remembering custom named uploaded file for video ID {video_id} and for url {url:?}");
                    let _ = telegram
                        .send_text_to_user(format!("Video {video_id}: {url}"), *id)
                        .await;
                    return;
                }

                let _ = redis
                    .set::<String, String, String>(key.clone(), url.clone())
                    .await;
                let _ = redis.expire::<String, String>(key, 7 * 24 * 60 * 60).await;
                let _ = telegram
                    .send_text_to_user(format!("Video {video_id}: {url}"), *id)
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
        let output_name = if pieces.len() > 2 {
            pieces[2..].join(" ").to_owned()
        } else {
            "".to_owned()
        };
        let slugified_name = Some(Self::slugified_name(&output_name)).filter(|s| !s.is_empty());

        let video_id = match Url::parse(video) {
            Ok(url) => url
                .query_pairs()
                .into_iter()
                .collect::<HashMap<_, _>>()
                .get("v")
                .map(|c| c.clone().into_owned())
                .or_else(|| {
                    url.path_segments()
                        .and_then(|p| p.last())
                        .map(|x| x.to_owned())
                })
                .unwrap_or(video.to_owned()),
            _ => video.to_owned(),
        };

        self.telegram
            .send_text_to_user(format!("Sto scaricando {} {}", command, video_id), id)
            .await?;

        match command.to_lowercase().as_str() {
            "audio" => return self.download_audio(id, video_id, slugified_name).await,
            _ => return self.download_video(id, video_id, slugified_name).await,
        };
    }
}

async fn upload_file(client: Client, path: PathBuf, desired_name: &str) -> Result<Url, String> {
    // See https://gofile.io/api
    tracing::info!("Uploading file {:?} to GoFile", path);
    let url = Url::parse("https://api.gofile.io/servers").map_err(|err| format!("{err}"))?;

    let request = client.get(url).build().map_err(|err| format!("{err}"))?;

    let server = client
        .execute(request)
        .await
        .map_err(|err| format!("{err}"))?
        .json::<GetServer>()
        .await
        .map_err(|err| format!("{err}"))?
        .data
        .servers
        .into_iter()
        .next()
        .ok_or("Unable to find any server from GoFile".to_owned())?
        .name;

    tracing::info!("Found GoFile server {}", server);

    let file = fs::read(path.clone())
        .await
        .map_err(|err| format!("{err}"))?;
    let file_part = reqwest::multipart::Part::bytes(file).file_name(desired_name.to_owned());
    let form = reqwest::multipart::Form::new().part("file", file_part);
    let upload_request = client
        .post(
            Url::parse(&format!("https://{server}.gofile.io/uploadFile"))
                .map_err(|e| e.to_string())?,
        )
        .multipart(form)
        .build()
        .map_err(|err| format!("{err}"))?;

    tracing::info!("Starting upload of {:?}: {:?}", path, upload_request);

    client
        .execute(upload_request)
        .await
        .map_err(|err| format!("{err}"))?
        .json::<UploadFile>()
        .await
        .map(|result| result.data.download_page)
        .map_err(|err| format!("{err}"))
}

async fn get_title(video_id: &str) -> String {
    let result = Command::new("yt-dlp")
        .arg("--skip-download")
        .arg("-J")
        .arg(video_id)
        .output()
        .await
        .map_err(|e| e.to_string())
        .and_then(|out| String::from_utf8(out.stdout).map_err(|e| e.to_string()))
        .and_then(|out| serde_json::from_str(&out).map_err(|e| e.to_string()))
        .map(|metadata: VideoMetadata| metadata.title)
        .map_err(|e| e.to_string());

    match result {
        Ok(title) => title,
        Err(e) => {
            tracing::warn!("Unable to fetch metadata for video {video_id}: {e}. Fallback to the video ID ({video_id})");
            video_id.to_owned()
        }
    }
}

#[derive(Deserialize)]
struct GetServer {
    pub data: Servers,
}

#[derive(Deserialize)]
struct VideoMetadata {
    pub title: String,
}

#[derive(Deserialize)]
struct Servers {
    pub servers: Vec<Server>,
}

#[derive(Deserialize)]
struct Server {
    pub name: String,
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

enum DownloadCommand {
    Video {
        id: UserId,
        video_id: String,
        target_name: Option<String>,
    },
    Audio {
        id: UserId,
        video_id: String,
        target_name: Option<String>,
    },
}

impl DownloadCommand {
    pub fn user_id(&self) -> &UserId {
        match self {
            DownloadCommand::Video { id, .. } | DownloadCommand::Audio { id, .. } => id,
        }
    }
    pub fn video_id(&self) -> &str {
        match self {
            DownloadCommand::Video { video_id, .. } | DownloadCommand::Audio { video_id, .. } => {
                video_id
            }
        }
    }
    pub fn file_name(&self, file_name: &str) -> String {
        match self {
            DownloadCommand::Video { .. } => format!("{}.mp4", file_name),
            DownloadCommand::Audio { .. } => format!("{}.mp3", file_name),
        }
    }
    pub fn target_name(&self) -> Option<&str> {
        match self {
            DownloadCommand::Video { target_name, .. }
            | DownloadCommand::Audio { target_name, .. } => {
                target_name.as_ref().map(|x| x.as_str())
            }
        }
    }
    pub fn caching_key(&self) -> String {
        match self {
            DownloadCommand::Video { video_id, .. } => format!("video:{}", video_id),
            DownloadCommand::Audio { video_id, .. } => format!("audio:{}", video_id),
        }
    }
    pub fn into_download_command(&self, download_path: &str) -> Command {
        match self {
            DownloadCommand::Video { video_id, .. } => {
                let mut cmd = Command::new("yt-dlp");
                cmd.arg("-f")
                    .arg("bestvideo[vcodec^=avc]+bestaudio[ext=m4a]/best[ext=mp4]/best")
                    .arg(video_id)
                    .arg("-o")
                    .arg(download_path);
                cmd
            }
            DownloadCommand::Audio { video_id, .. } => {
                let mut cmd = Command::new("yt-dlp");
                cmd.arg("-x")
                    .arg("--audio-format")
                    .arg("mp3")
                    .arg("--audio-quality")
                    .arg("0")
                    .arg(video_id)
                    .arg("-o")
                    .arg(download_path);
                cmd
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Write, path::PathBuf};

    use reqwest::Client;

    use super::upload_file;

    #[tokio::test]
    async fn test_upload_file_to_go_file() {
        let client = Client::builder().build().unwrap();
        let path = PathBuf::from(".delme");

        File::create(path.as_path())
            .unwrap()
            .write("test".as_bytes())
            .unwrap();

        let url = upload_file(client, path, ".delme").await.unwrap();
        println!("✅ Url = {url}");

        assert!(!url.as_str().is_empty());
    }
}
