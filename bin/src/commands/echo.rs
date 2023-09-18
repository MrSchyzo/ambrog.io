use async_trait::async_trait;

use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

struct EchoMessageHandler {
    telegram: Box<dyn TelegramProxy>
}

impl EchoMessageHandler {
    fn new<C: TelegramProxy + Clone>(telegram: &C) -> Self {
        Self { telegram: Box::new(telegram.clone()) }
    }
}

#[async_trait]
impl MessageHandler for EchoMessageHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool {
        true
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let (id, name) = (user.id(), user.name());
        let message = format!("{text} to you, {name}!");
        self.telegram.send_text_to_user(message, id).await
    }
}