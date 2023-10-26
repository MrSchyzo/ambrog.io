use std::{env, sync::Arc, time::Duration};

use crate::telegram::TelegramProxy;
use async_trait::async_trait;
use regex::Regex;
use rusty_ytdl::{Video, VideoOptions};
use tokio::fs::remove_file;

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
            regex: Regex::new(r"(?i)^scarica(mi)?\s+[^\s]+").unwrap(),
        }
    }
}

#[async_trait]
impl MessageHandler for YoutubeDownloadHandler {
    fn can_accept(&self, InboundMessage { text, .. }: &InboundMessage) -> bool {
        self.regex.is_match(text)
    }

    async fn handle(&self, InboundMessage { user, .. }: InboundMessage) -> Result<(), String> {
        let id = user.id();
        self.telegram
            .send_text_to_user("Sto scaricando jNQXAC9IVRw".to_string(), id)
            .await?;

        tokio::spawn({
            let telegram = self.telegram.clone();
            async move {
                let path = env::current_dir().unwrap().join("jNQXAC9IVRw.webp");
                let _ = Video::new_with_options(
                    "jNQXAC9IVRw".to_owned(),
                    VideoOptions {
                        quality: rusty_ytdl::VideoQuality::Highest,
                        ..Default::default()
                    },
                )
                .unwrap()
                .download(path.clone())
                .await
                .unwrap();
                let _ = telegram.send_local_video(path.clone(), id).await;
                // TODO: remove this wait
                tokio::time::sleep(Duration::from_millis(1000u64)).await;

                let _ = remove_file(path).await;
            }
        });

        Ok(())
    }
}
