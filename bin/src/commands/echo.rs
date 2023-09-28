use std::sync::Arc;

use crate::telegram::TelegramProxy;
use async_trait::async_trait;

use super::{InboundMessage, MessageHandler};

pub struct EchoMessageHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
}

impl EchoMessageHandler {
    pub fn new<Proxy>(telegram: Arc<Proxy>) -> Self
    where
        Proxy: TelegramProxy + Send + Sync + 'static,
    {
        Self { telegram }
    }
}

#[async_trait]
impl MessageHandler for EchoMessageHandler {
    fn can_accept(&self, _: &InboundMessage) -> bool {
        true
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let (id, name) = (user.id(), user.name());
        let message = format!("{text} a Lei, {name}!");
        self.telegram.send_text_to_user(message, id).await
    }
}
