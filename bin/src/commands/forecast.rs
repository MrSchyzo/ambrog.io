use async_trait::async_trait;
use regex::Regex;

use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

struct ForecastHandler {
    telegram: Box<dyn TelegramProxy>,
    regex: Regex,
}

impl ForecastHandler {
    fn new<C: TelegramProxy + Clone>(telegram: &C) -> Self {
        Self { 
            telegram: Box::new(telegram.clone()), 
            regex: Regex::new(r#"(?i)^meteo\s+\w+"#).unwrap()
        }
    }
}

#[async_trait]
impl MessageHandler for ForecastHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool {
        self.regex.is_match(&msg.text)
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let city = text.splitn(2, " ").into_iter().nth(1).unwrap_or("Pistoia").trim_end();
        let req = ForecastRequestBuilder::default()
            .past_days(0)
            .future_days(2)
            .place_name(city.to_owned())
            .build()
            .unwrap();
        todo!()
    }
}