use std::env;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;
use chrono::{TimeZone, DateTime};
use log::{info, error};
use reqwest::Client;
use serde::Deserialize;
use teloxide::prelude::*;
use teloxide::types::Recipient;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    info!("Booting up ambrog.io");

    let client = match reqwest::ClientBuilder::new().build(){
        Ok(client) => Arc::new(client),
        Err(err) => {
            error!("HTTP client cannot be created: {err}");
            return;
        }
    };

    let super_user_id = match super_user_id_from_env("USER_ID") {
        Ok(u) => u,
        Err(str) => {
            error!("Unable to get super user id: {str}");
            return;
        } 
    };

    let chat_id = ChatId::from(super_user_id);
    let super_user = Recipient::from(super_user_id.clone());

    let bot = Bot::from_env();
    let _ = bot.send_message(super_user, "Ambrog.io greets you, sir!").await;

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let client = client.clone();
        let chat_id = chat_id.clone();

        async move {
            let user = msg.chat.username().unwrap_or("N/A");
            let text = msg.text().unwrap_or("N/A");
            if msg.chat.id != chat_id {
                return Ok(());
            }
            if !text.starts_with("meteo") {
                bot.send_message(chat_id, format!("{text} to you, {user}!")).await.ok();
                return Ok(());
            }
            let city = text.split(" ").into_iter().nth(1).unwrap_or("Pistoia");
            let message = match weather_forecast_tomorrow(client.clone(), city).await {
                Ok(m) => {
                    let time = m.time_series.first().map(|t| format!("{}", t.time.with_timezone(&chrono_tz::Europe::Rome).format("%d/%m/%Y"))).unwrap_or("today".to_owned());
                    format!("Meteo {time} {city}\n----------------------------\n{m}")
                }
                Err(e) => {
                    format!("No meteo: {e}")
                }
            };
            bot.send_message(chat_id, message).await.ok();

            Ok(())
        }
    }).await;
}

fn super_user_id_from_env(env_var: &str) -> Result<UserId, String> {
    let string = env::var(env_var)
        .map_err(|_| format!("{env_var} environment variable not found"))?;

    u64::from_str(&string)
        .map_err(|_| format!("{env_var} environment variable is not u64, got '{string}'"))
        .map(UserId)
}

async fn weather_forecast_tomorrow(client: Arc<Client>, place_name: &str) -> Result<Meteo, String> {
    let geo = geolocalise(client.clone(), place_name).await.map_err(|err| format!("{err}"))?;

    let url = reqwest::Url::parse_with_params(
        "https://api.open-meteo.com/v1/forecast", 
        &[
            ("latitude", &geo.latitude.to_string()), 
            ("longitude", &geo.longitude.to_string()), 
            ("hourly", &"temperature_2m,precipitation_probability,precipitation,windspeed_10m,winddirection_10m".to_string()),
            ("timezone", &geo.timezone.to_string()),
            ("past_days", &"0".to_string()),
            ("forecast_days", &"1".to_string()),
        ]
    ).map_err(|err| format!("{err}"))?;
    
    let request = client.get(url)
        .build()
        .map_err(|err| format!("{err}"))?;

    client.execute(request).await
        .map_err(|err| format!("{err}"))?
        .json::<Forecast>().await
        .map_err(|err| format!("{err}"))?
        .try_into()
}

async fn geolocalise(client: Arc<Client>, place_name: &str) -> Result<Geolocalisation, String> {
    // v1/search?name=Pistoia&count=100&language=it&format=json
    let url = reqwest::Url::parse_with_params(
        "https://geocoding-api.open-meteo.com/v1/search?format=jsonname=Pistoia&count=100&language=it&format=json", 
        &[
            ("name", place_name), 
            ("count", "1"), 
            ("language", "en")
        ]
    ).map_err(|err| format!("{err}"))?;
    
    let request = client.get(url)
        .build()
        .map_err(|err| format!("{err}"))?;

    client.execute(request).await
        .map_err(|err| format!("{err}"))?
        .json::<Geocoding>().await
        .map_err(|err| format!("{err}"))?
        .results
        .into_iter().nth(0)
        .ok_or(format!("'{place_name}' without hits"))?
        .try_into()
}

#[derive(Deserialize)]
struct Geocoding {
    pub results: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub elevation: f64,
    pub timezone: String,
}

impl TryFrom<Hit> for Geolocalisation {
    type Error = String;

    fn try_from(value: Hit) -> Result<Self, Self::Error> {
        Ok(Geolocalisation { 
            name: value.name, 
            latitude: value.latitude, 
            longitude: value.longitude, 
            elevation: value.elevation as i32, 
            timezone: value.timezone.parse().map_err(|err| format!("{err}"))?
        })
    }
}

struct Geolocalisation {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub elevation: i32,
    pub timezone: chrono_tz::Tz
}

#[derive(Deserialize)]
struct Forecast {
    pub timezone: String,
    pub hourly_units: HourlyUnits,
    pub hourly: Hourly,
}

#[derive(Deserialize)]
struct HourlyUnits {
    pub temperature_2m: String,
    pub precipitation: String,
    pub precipitation_probability: String,
    pub windspeed_10m: String,
    pub winddirection_10m: String,
}

#[derive(Deserialize)]
struct Hourly {
    pub time: Vec<String>,
    pub temperature_2m: Vec<f64>,
    pub precipitation: Vec<f64>,
    pub precipitation_probability: Vec<f64>,
    pub windspeed_10m: Vec<f64>,
    pub winddirection_10m: Vec<f64>,
}

impl TryFrom<Forecast> for Meteo {
    type Error = String;

    fn try_from(value: Forecast) -> Result<Self, Self::Error> {
        let utc = &chrono::Utc;
        let timezone = &value.timezone;
        let tz: chrono_tz::Tz = value.timezone.parse().map_err(|_| format!("Unparseable timezone {timezone}"))?;

        let temperature_2m = value.hourly.temperature_2m;
        let t2m_unit = value.hourly_units.temperature_2m;
        
        let precipitation = value.hourly.precipitation;
        let p_unit = value.hourly_units.precipitation;
        
        let precipitation_probability = value.hourly.precipitation_probability;
        let pp_unit = value.hourly_units.precipitation_probability;
        
        let windspeed_10m = value.hourly.windspeed_10m;
        let w10m_unit = value.hourly_units.windspeed_10m;
        
        let winddirection_10m = value.hourly.winddirection_10m;
        let wd10m_unit = value.hourly_units.winddirection_10m;
        let mut result: Vec<Weather> = vec![];
        for (i, item) in value.hourly.time.iter().enumerate() {
            let point = Weather {
                time: tz.datetime_from_str(&item.to_string(), "%Y-%m-%dT%H:%M").map_err(|e| format!("Unable to parse date {e}"))?.with_timezone(utc),
                precipitation: HumanReadableMeasure(precipitation[i], p_unit.to_owned()),
                precipitation_probability: HumanReadableMeasure(precipitation_probability[i], pp_unit.to_owned()),
                temperature_2m: HumanReadableMeasure(temperature_2m[i], t2m_unit.to_owned()),
                windspeed_10m: HumanReadableMeasure(windspeed_10m[i], w10m_unit.to_owned()),
                winddirection_10m: HumanReadableMeasure(winddirection_10m[i], wd10m_unit.to_owned()),
            };
            result.push(point)
        }

        Ok(Meteo{ time_series: result })
    }
}

struct Meteo {
    pub time_series: Vec<Weather>,
}
impl Display for Meteo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for ele in &self.time_series {
            f.write_str(&format!("{ele}\n"))?;
        }
        Ok(())
    }
}

struct Weather {
    pub time: DateTime<chrono::Utc>,
    pub temperature_2m: HumanReadableMeasure,
    pub precipitation: HumanReadableMeasure,
    pub precipitation_probability: HumanReadableMeasure,
    pub windspeed_10m: HumanReadableMeasure,
    pub winddirection_10m: HumanReadableMeasure,
}

impl Display for Weather {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let time = &self.time.with_timezone(&chrono_tz::Europe::Rome).format("%H:%M");
        let temp = &self.temperature_2m;
        let prec = &self.precipitation;
        let prob = &self.precipitation_probability;
        let wind = &self.windspeed_10m;
        let wind_dir = &self.winddirection_10m;
        f.write_str(&format!("{time} -> 🌡️{temp} - 🌧️{prec}({prob}) - 💨{wind}({wind_dir})"))
    }
}

struct HumanReadableMeasure(f64, String);

impl Display for HumanReadableMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let num = self.0;
        let unit = &self.1;
        f.write_str(&format!("{num}{unit}"))
    }
}