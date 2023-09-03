use std::env;
use std::str::FromStr;
use std::sync::Arc;
use log::{info, error};
use teloxide::prelude::*;
use teloxide::types::Recipient;
use open_meteo::{ReqwestForecastClient, ForecastClient, ForecastRequestBuilder};

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

    let forecast_client = Arc::new(ReqwestForecastClient::new(
        client.clone(), 
        "https://geocoding-api.open-meteo.com".to_owned(), 
        "https://api.open-meteo.com".to_owned()
    ));

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
        let meteo_client = forecast_client.clone();
        let chat_id = chat_id.clone();
        let regex = regex::Regex::new(r#"^[Mm]eteo"#).unwrap();

        async move {
            let user = msg.chat.username().unwrap_or("N/A");
            let text = msg.text().unwrap_or("N/A");
            if msg.chat.id != chat_id {
                return Ok(());
            }
            if !regex.is_match(text) {
                bot.send_message(chat_id, format!("{text} to you, {user}!")).await.ok();
                return Ok(());
            }
            let city = text.splitn(2, " ").into_iter().nth(1).unwrap_or("Pistoia");
            let req = ForecastRequestBuilder::default().past_days(0).future_days(1).place_name(city.to_owned()).build().unwrap();
            let message = match meteo_client.weather_forecast(&req).await {
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