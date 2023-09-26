use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

pub struct FerreroHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    url: String,
}

impl FerreroHandler {
    pub fn new<Proxy>(telegram: Arc<Proxy>, url: String) -> Self
        where Proxy: TelegramProxy + Send + Sync + 'static {
        Self { telegram, url }
    }
}

#[async_trait]
impl MessageHandler for FerreroHandler {

    fn can_accept(&self, InboundMessage {text, ..}: &InboundMessage) -> bool {
        text.contains("languorino")
    }

    async fn handle(&self, InboundMessage { user, .. }: InboundMessage) -> Result<(), String> {
        let id = user.id();

        tokio::spawn((|| {
                let telegram = self.telegram.clone();
                let message = "Mi ero permesso di pensarci, Signore.".to_owned();
                let url = self.url.clone();
                async move {
                    let _ = telegram.send_text_to_user(message, id).await;
                    tokio::time::sleep(Duration::from_millis(250u64)).await;
                    let _ = telegram.send_gif_from_url(&url, id).await;
                }
            }
        )());

        Ok(())
    }
}
