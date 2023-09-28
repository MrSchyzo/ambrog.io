use std::sync::Arc;

use crate::telegram::TelegramProxy;
use ambrogio_users::data::User;
use async_trait::async_trait;

use super::{InboundMessage, MessageHandler};

pub struct ShutdownHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
}

impl ShutdownHandler {
    pub fn new<Proxy>(telegram: Arc<Proxy>) -> Self
    where
        Proxy: TelegramProxy + Send + Sync + 'static,
    {
        Self { telegram }
    }
}

#[async_trait]
impl MessageHandler for ShutdownHandler {
    fn can_accept(&self, InboundMessage { user, text }: &InboundMessage) -> bool {
        text.eq_ignore_ascii_case("dormi pure") && matches!(user, &User::SuperUser { .. })
    }

    async fn handle(&self, InboundMessage { user, .. }: InboundMessage) -> Result<(), String> {
        self.telegram
            .send_text_to_user("Buona notte, Signore!".to_string(), user.id())
            .await?;
        tracing::info!("Shutting down");
        std::process::exit(0)
    }
}
