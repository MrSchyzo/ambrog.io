use ambrogio_users::RedisUserRepository;
use ambrogio_users::User;
use ambrogio_users::UserRepository;
use log::warn;
use log::{error, info};
use open_meteo::{ForecastClient, ForecastRequestBuilder, ReqwestForecastClient};
use std::env;
use std::str::FromStr;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::Recipient;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    info!("Booting up ambrog.io");

    
    let client = match reqwest::ClientBuilder::new().build() {
        Ok(client) => Arc::new(client),
        Err(err) => {
            error!("HTTP client cannot be created: {err}");
            return;
        }
    };

    let redis = match redis::Client::open("redis://127.0.0.1") {
        Ok(client) => client,
        Err(e) => {
            error!("Redis client cannot be created: {e}");
            return;
        }
    };
    
    let redis_connection = match redis.get_multiplexed_tokio_connection().await {
        Ok(connection) => connection,
        Err(e) => {
            error!("Redis client cannot be created: {e}");
            return;
        }
    };

    let forecast_client = Arc::new(ReqwestForecastClient::new(
        client.clone(),
        "https://geocoding-api.open-meteo.com".to_owned(),
        "https://api.open-meteo.com".to_owned(),
    ));

    let super_user_id = match super_user_id_from_env("USER_ID") {
        Ok(u) => u,
        Err(str) => {
            error!("Unable to get super user id: {str}");
            return;
        }
    };

    let super_user_id = ChatId::from(super_user_id);
    let super_user_dest = Recipient::from(super_user_id.clone());

    let bot = Bot::from_env();
    let _ = bot
        .send_message(super_user_dest.clone(), "Ambrog.io greets you, sir!")
        .await;

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let meteo_client = forecast_client.clone();
        let super_user_id = super_user_id.clone();
        let super_user_dest = super_user_dest.clone();
        let regex = regex::Regex::new(r#"(?i)^meteo"#).unwrap();
        let user_admin_command = regex::Regex::new(r#"(?i)^add|remove"#).unwrap();
        let repo = RedisUserRepository::new(redis_connection.clone());

        async move {
            let user = msg.chat.username().unwrap_or("N/A");
            let text = msg.text().unwrap_or("N/A");
            info!("{user} {text}");
            let user_id = match repo.get(msg.chat.id.0 as u64).await.unwrap() {
                None if msg.chat.id != super_user_id => {
                    let chat = msg.chat.id;
                    bot.send_message(super_user_dest, format!("Who is {chat}?"))
                        .await
                        .ok();
                    return Ok(());
                }
                None => super_user_id,
                Some(user) => ChatId::from(UserId(user.id)),
            };
            let dest = Recipient::from(user_id.clone());

            info!("I know {user}: they're {user_id}!");
            if user_admin_command.is_match(text) && user_id == super_user_id {
                let new_id = match text
                    .splitn(2, " ")
                    .into_iter()
                    .nth(1)
                    .ok_or("insufficient arguments for command".to_owned())
                    .and_then(|p| u64::from_str(p).map_err(|e| format!("{e}")))
                {
                    Err(e) => {
                        warn!("User admin command error: {e}");
                        bot.send_message(dest, format!("User admin command error: {e}!"))
                            .await
                            .ok();
                        return Ok(());
                    }
                    Ok(x) => x,
                };
                let execution = if text.to_lowercase().starts_with("add") {
                    repo.set(User { id: new_id })
                } else {
                    repo.remove(new_id)
                }
                .await;
                match execution {
                    Ok(_) => bot
                        .send_message(dest, format!("Success in executing '{text}'"))
                        .await
                        .ok(),
                    Err(e) => bot
                        .send_message(dest, format!("Failure in executing '{text}': {e}!"))
                        .await
                        .ok(),
                };
                return Ok(());
            }

            if !regex.is_match(text) {
                bot.send_message(dest, format!("{text} to you, {user}!"))
                    .await
                    .ok();
                return Ok(());
            }
            let city = text.splitn(2, " ").into_iter().nth(1).unwrap_or("Pistoia");
            let req = ForecastRequestBuilder::default()
                .past_days(0)
                .future_days(2)
                .place_name(city.to_owned())
                .build()
                .unwrap();
            let message = match meteo_client.weather_forecast(&req).await {
                Ok(m) => {
                    let time = m
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
                    format!("Meteo {time} {city}\n----------------------------\n{m}")
                }
                Err(e) => {
                    format!("No meteo: {e}")
                }
            };
            bot.send_message(dest, message).await.ok();

            Ok(())
        }
    })
    .await;
}

fn super_user_id_from_env(env_var: &str) -> Result<UserId, String> {
    let string =
        env::var(env_var).map_err(|_| format!("{env_var} environment variable not found"))?;

    u64::from_str(&string)
        .map_err(|_| format!("{env_var} environment variable is not u64, got '{string}'"))
        .map(UserId)
}
