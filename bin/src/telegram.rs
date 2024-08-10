use std::{path::PathBuf, str::FromStr};

use ambrogio_users::data::UserId as AmbrogioUserId;
use async_trait::async_trait;
use teloxide::{
    requests::Requester,
    types::{InputFile, UserId},
    Bot,
};
use url::Url;

#[async_trait]
#[allow(dead_code)]
pub trait TelegramProxy {
    async fn send_text_to_user(
        &self,
        message: String,
        user_id: AmbrogioUserId,
    ) -> Result<(), String>;
    async fn send_gif_from_url(&self, raw_url: &str, user_id: AmbrogioUserId)
        -> Result<(), String>;
    async fn send_local_video(&self, path: PathBuf, user_id: AmbrogioUserId) -> Result<(), String>;
    async fn send_local_audio(&self, path: PathBuf, user_id: AmbrogioUserId) -> Result<(), String>;
}

#[derive(Clone)]
pub struct TeloxideProxy {
    bot: Bot,
}

impl TeloxideProxy {
    pub fn new(bot: &Bot) -> TeloxideProxy {
        Self { bot: bot.clone() }
    }
}

#[async_trait]
impl TelegramProxy for TeloxideProxy {
    async fn send_text_to_user(
        &self,
        message: String,
        user_id: AmbrogioUserId,
    ) -> Result<(), String> {
        let user = UserId(user_id.0);
        self.bot
            .send_message(user, message)
            .await
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
    async fn send_gif_from_url(
        &self,
        raw_url: &str,
        user_id: AmbrogioUserId,
    ) -> Result<(), String> {
        let user = UserId(user_id.0);
        let file = Url::from_str(raw_url)
            .map(InputFile::url)
            .map_err(|e| e.to_string())?;

        self.bot
            .send_video(user, file)
            .await
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
    async fn send_local_video(
        &self,
        path: PathBuf,
        AmbrogioUserId(user_id): AmbrogioUserId,
    ) -> Result<(), String> {
        let user = UserId(user_id);

        self.bot
            .send_video(user, InputFile::file(path))
            .await
            .map_err(|e| e.to_string())
            .unwrap();

        Ok(())
    }
    async fn send_local_audio(
        &self,
        path: PathBuf,
        AmbrogioUserId(user_id): AmbrogioUserId,
    ) -> Result<(), String> {
        let user = UserId(user_id);

        self.bot
            .send_audio(user, InputFile::file(path))
            .await
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
}
