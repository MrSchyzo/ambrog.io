use std::{collections::HashMap, env, sync::Arc};

use crate::telegram::TelegramProxy;
use ambrogio_users::data::UserId;
use async_process::Command;
use async_trait::async_trait;
use regex::Regex;
use tokio::fs::remove_file;
use url::Url;

use super::{InboundMessage, MessageHandler};

pub struct YoutubeDownloadHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    regex: Regex,
}

impl YoutubeDownloadHandler {
    pub fn new<Proxy>(telegram: Arc<Proxy>) -> Self
    where
        Proxy: TelegramProxy + Send + Sync + 'static,
    {
        Self {
            telegram,
            regex: Regex::new(r"(?i)^(video|audio)?\s+[^\s]+").unwrap(),
        }
    }

    async fn download_video(&self, id: UserId, video_id: String) -> Result<(), String> {
        tokio::spawn({
            let telegram = self.telegram.clone();
            async move {
                let output = format!("{}.mp4", video_id);

                let _ = Command::new("yt-dlp")
                    .arg("-f")
                    .arg("bestvideo[vcodec^=avc]+bestaudio[ext=m4a]/best[ext=mp4]/best")
                    .arg(&video_id)
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .await
                    .unwrap();

                let path = env::current_dir().unwrap().join(&output);
                let _ = telegram.send_local_video(path.clone(), id).await;
                let _ = remove_file(path).await;
            }
        });

        Ok(())
    }

    async fn download_audio(&self, id: UserId, video_id: String) -> Result<(), String> {
        tokio::spawn({
            let telegram = self.telegram.clone();
            async move {
                let output = format!("{}.mp3", video_id);
                let _ = Command::new("yt-dlp")
                    .arg("-x")
                    .arg("--audio-format")
                    .arg("mp3")
                    .arg("--audio-quality")
                    .arg("0")
                    .arg(&video_id)
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .await
                    .unwrap();

                let path = env::current_dir().unwrap().join(&output);
                let _ = telegram.send_local_audio(path.clone(), id).await;
                let _ = remove_file(path).await;
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
