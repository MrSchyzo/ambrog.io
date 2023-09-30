use std::sync::Arc;

use crate::telegram::TelegramProxy;
use async_trait::async_trait;
use itertools::Itertools;
use open_meteo::{ForecastClient, ForecastRequestBuilder, Meteo, Weather};
use regex::Regex;

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
        Client: ForecastClient + Send + Sync + 'static,
    {
        Self {
            telegram,
            forecast,
            regex: Regex::new(r"(?i)^meteo\s+\w+").unwrap(),
        }
    }

    fn render_forecast(meteo: Meteo, city: &str) -> Vec<String> {
        let rome = &chrono_tz::Europe::Rome;
        let now = chrono::Utc::now();
        let header = vec![format!(
            "Meteo per \"{}\"\nLocalità: {} ({})",
            city, meteo.city_name, meteo.city_description
        )];
        let days = meteo
            .time_series
            .into_iter()
            .filter(|line| line.time.ge(&now))
            .group_by(|t| t.time.with_timezone(rome).format("%d/%m/%Y").to_string())
            .into_iter()
            .map(|(date, series)| {
                let lines = series.map(Self::render_line).join("\n");
                format!("Previsioni per {date}\n-------------------\n{lines}")
            })
            .collect::<Vec<_>>();

        header.into_iter().chain(days).collect_vec()
    }

    fn render_line(line: Weather) -> String {
        let rome = &chrono_tz::Europe::Rome;

        let rain = if line.precipitation.number() * line.precipitation_probability.number() > 0f64 {
            format!(
                "🌧️ {} (prob. {})",
                line.precipitation, line.precipitation_probability
            )
        } else {
            String::new()
        };

        let wind = if line.windspeed_10m.number() >= 6f64 {
            format!(
                "💨 {} (dir. {})",
                line.windspeed_10m, line.winddirection_10m
            )
        } else {
            String::new()
        };

        format!(
            "{} -> {} {} {}",
            line.time.with_timezone(rome).time().format("%H:%M"),
            line.temperature_2m,
            rain,
            wind
        )
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .join(" ")
    }
}

#[async_trait]
impl MessageHandler for ForecastHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool {
        self.regex.is_match(&msg.text)
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let city = text
            .split_once(' ')
            .map(|x| x.1)
            .unwrap_or("Pistoia")
            .trim();
        let req = ForecastRequestBuilder::default()
            .past_days(0)
            .future_days(2)
            .place_name(city.to_owned())
            .build()
            .map_err(|e| e.to_string())?;
        let forecast = self.forecast.weather_forecast(&req).await?;
        for message in Self::render_forecast(forecast, city) {
            let _ = self
                .telegram
                .send_text_to_user(message, user.id())
                .await
                .map(|_| ());
        }
        Ok(())
    }
}
