use std::sync::Arc;

use ambrogio_users::{
    data::{User, UserId},
    UserRepository,
};
use async_trait::async_trait;
use regex::Regex;
use std::str::FromStr;

use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

pub struct UserHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    repo: Arc<dyn UserRepository + Send + Sync + 'static>,
    regex: Regex,
}

impl UserHandler {
    pub fn new<Proxy, Repository>(telegram: Arc<Proxy>, repo: Arc<Repository>) -> Self
    where
        Proxy: TelegramProxy + Send + Sync + 'static,
        Repository: UserRepository + Send + Sync + 'static,
    {
        Self {
            telegram,
            repo,
            regex: Regex::new(r"(?i)^(add|remove)\s+\d+").unwrap(),
        }
    }

    fn extract_user_id(text: &str) -> Result<UserId, String> {
        text.split_once(' ')
            .map(|x| x.1)
            .ok_or("insufficient arguments for command".to_owned())
            .and_then(|p| u64::from_str(p).map_err(|e| format!("{e}")))
            .map(UserId)
    }

    async fn add_or_remove_user(&self, text: &str, target: UserId) -> String {
        if text.to_lowercase().starts_with("add") {
            self.repo.set(User::SimpleUser { id: target })
        } else {
            self.repo.remove(target)
        }
        .await
        .map(|_| format!("'{text}' ✅"))
        .unwrap_or_else(|e| format!("❌ '{text}': {e}!"))
    }
}

#[async_trait]
impl MessageHandler for UserHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool {
        self.regex.is_match(&msg.text) && msg.user.is_super_user()
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let target = Self::extract_user_id(&text)?;
        let result_message = self.add_or_remove_user(&text, target).await;

        self.telegram
            .send_text_to_user(result_message, user.id())
            .await
    }
}
