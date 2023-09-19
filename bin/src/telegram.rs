use ambrogio_users::data::UserId as AmbrogioUserId;
use async_trait::async_trait;
use teloxide::{Bot, requests::Requester, types::UserId};


#[async_trait]
pub trait TelegramProxy {
    async fn send_text_to_user(&self, message: String, user_id: AmbrogioUserId) -> Result<(), String>;
}

#[derive(Clone)]
pub struct TeloxideProxy {
    bot: Bot
}

impl TeloxideProxy {
    pub fn new(bot: &Bot) -> TeloxideProxy {
        Self{ bot: bot.clone() }
    }
}

#[async_trait]
impl TelegramProxy for TeloxideProxy {
    async fn send_text_to_user(&self, message: String, user_id: AmbrogioUserId) -> Result<(), String> {
            let user = UserId(user_id.0);
            self
                .bot
                .send_message(user, message)
                .await
                .map_err(|e| e.to_string())
                .map(|_| ())
        }
}