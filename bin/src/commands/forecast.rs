use std::sync::Arc;

use crate::telegram::TelegramProxy;
use async_trait::async_trait;
use chrono::NaiveDate;
use itertools::Itertools;
use open_meteo::{ForecastClient, ForecastRequest, Meteo, Weather};
use regex::Regex;

use super::{InboundMessage, MessageHandler};

#[derive(Clone)]
pub struct ForecastHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    forecast: Arc<dyn ForecastClient + Send + Sync + 'static>,
    regex: Regex,
}

#[async_trait]
impl MessageHandler for ForecastHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool {
        self.regex.is_match(&msg.text)
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let (maybe_city, day_in_future) = Self::parse(text);
        let city = maybe_city.as_deref().unwrap_or("Roma");
        let req = day_in_future
            .map(|day| ForecastRequest::city_specific_day(city, day))
            .unwrap_or_else(|| ForecastRequest::city_only(city))?;

        let forecast = self.forecast.weather_forecast(&req).await?;
        for message in Self::render_forecast(forecast, city, day_in_future.is_some()) {
            let _ = self
                .telegram
                .send_text_to_user(message, user.id())
                .await
                .map(|_| ());
        }
        Ok(())
    }
}

impl ForecastHandler {
    const ROME: &'static chrono_tz::Tz = &chrono_tz::Europe::Rome;
    const SUPPORTED_FORMATS: [&'static str; 6] = [
        "%d/%m/%Y", "%d-%m-%Y", "%Y/%m/%d", "%Y-%m-%d", "%m/%d/%Y", "%m-%d-%Y",
    ];

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

    fn render_forecast(meteo: Meteo, city: &str, last_only: bool) -> Vec<String> {
        let now = chrono::Utc::now();
        let header = vec![format!(
            "Meteo per \"{}\"\nLocalità: {} ({})",
            city, meteo.city_name, meteo.city_description
        )];
        let days = meteo
            .time_series
            .into_iter()
            .filter(|line| line.time.ge(&now))
            .group_by(|t| {
                t.time
                    .with_timezone(Self::ROME)
                    .format("%d/%m/%Y")
                    .to_string()
            })
            .into_iter()
            .map(|(date, series)| {
                let lines = series.map(Self::render_line).join("\n");
                format!("Previsioni per {date}\n-------------------\n{lines}")
            })
            .collect_vec();

        let days_to_chain = if last_only {
            days.last().cloned().into_iter().collect_vec()
        } else {
            days
        };

        header.into_iter().chain(days_to_chain).collect_vec()
    }

    fn render_line(line: Weather) -> String {
        let rome = &chrono_tz::Europe::Rome;

        let rain = if line.precipitation.number() * line.precipitation_probability.number() > 0f64 {
            format!(
                "🌧️ {} {}",
                line.precipitation_probability, line.precipitation
            )
        } else {
            String::new()
        };

        let wind = if line.windspeed_10m.number() >= 6f64 {
            format!("💨 {} {}", line.winddirection_10m, line.windspeed_10m)
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

    fn parse(command: String) -> (Option<String>, Option<u8>) {
        let now = chrono::Utc::now().with_timezone(Self::ROME).date_naive();

        let arguments = command.split(' ').skip(1).collect_vec();
        let mut days_in_future: Option<u8> = None;
        let mut city: Vec<&str> = vec![];

        for piece in arguments {
            if let Some(d) = Self::SUPPORTED_FORMATS
                .iter()
                .find_map(|format| NaiveDate::parse_from_str(piece, format).ok())
            {
                days_in_future = u8::try_from(d.signed_duration_since(now).num_days())
                    .ok()
                    .or(days_in_future);
            } else {
                city.push(piece)
            }
        }

        (Self::build_city(&city), days_in_future)
    }

    fn build_city(pieces: &[&str]) -> Option<String> {
        if pieces.is_empty() {
            None
        } else {
            Some(pieces.join(" "))
        }
    }
}
