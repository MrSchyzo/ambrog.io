use std::sync::Arc;

use async_trait::async_trait;
use open_meteo::{ForecastRequestBuilder, ForecastClient};
use regex::Regex;

use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

#[derive(Clone)]
pub struct ForecastHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    forecast: Arc<dyn ForecastClient + Send + Sync + 'static>,
    regex: Regex,
}

impl ForecastHandler {
    pub fn new<Proxy, Client>(telegram: Arc<Proxy>, forecast: Arc<Client>) -> Self
        where 
        Proxy: TelegramProxy + Send + Sync + 'static,
        Client: ForecastClient + Send + Sync + 'static {
        Self { 
            telegram,
            forecast,
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
        let city = text.splitn(2, " ").into_iter().nth(1).unwrap_or("Pistoia").trim();
        let req = ForecastRequestBuilder::default()
            .past_days(0)
            .future_days(2)
            .place_name(city.to_owned())
            .build()
            .map_err(|e| e.to_string())?;
        let forecast = self.forecast.weather_forecast(&req).await?;
        let time = forecast
                .time_series
                .first()
                .map(|t| {
                    format!(
                        "{}",
                        t.time
                            .with_timezone(&chrono_tz::Europe::Rome)
                            .format("%d/%m/%Y")
                    )
                })
                .unwrap_or("today".to_owned());
        let message = format!("Il meteo di {time}\n____________\n{forecast}");
        self.telegram
            .send_text_to_user(message, user.id())
            .await
            .map(|_| ())
    }
}