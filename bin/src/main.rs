mod commands;
mod telegram;

use ambrogio_users::RedisUserRepository;
use ambrogio_users::data::User as AmbrogioUser;
use ambrogio_users::data::UserId as AmbrogioUserId;
use ambrogio_users::UserRepository;
use commands::InboundMessage;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use open_meteo::ReqwestForecastClient;
use std::env;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use teloxide::prelude::*;
use teloxide::types::User;
use telegram::TelegramProxy;

use crate::commands::MessageHandler;
use crate::commands::echo::EchoMessageHandler;
use crate::commands::forecast::ForecastHandler;
use crate::commands::users::UserHandler;
use crate::telegram::TeloxideProxy;

#[tokio::main]
async fn main() {
    let start = SystemTime::now();
    setup_global_tracing_subscriber().unwrap();
    tracing::info!("Booting up ambrog.io");

    let bot = Bot::from_env();

    let client = match reqwest::ClientBuilder::new().build() {
        Ok(client) => Arc::new(client),
        Err(err) => {
            tracing::error!("HTTP client cannot be created: {err}");
            return;
        }
    };
    
    let redis = match env::var("REDIS_URL")
        .or(Ok("redis://127.0.0.1".to_owned()))
        .and_then(redis::Client::open)
    {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Redis client cannot be created: {e}");
            return;
        }
    };

    let redis_connection = match redis.get_multiplexed_tokio_connection().await {
        Ok(connection) => connection,
        Err(e) => {
            tracing::error!("Redis client cannot be created: {e}");
            return;
        }
    };
    let repo = Arc::new(RedisUserRepository::new(redis_connection.clone()));

    let forecast_client = Arc::new(ReqwestForecastClient::new(
        client.clone(),
        "https://geocoding-api.open-meteo.com".to_owned(),
        "https://api.open-meteo.com".to_owned(),
    ));

    let super_user_id = match super_user_id_from_env("USER_ID") {
        Ok(u) => u,
        Err(str) => {
            tracing::error!("Unable to get super user id: {str}");
            return;
        }
    };
    
    greet_master(&bot, super_user_id).await.unwrap();

    let elapsed = SystemTime::now().duration_since(start).map(|d| d.as_micros()).unwrap_or(0u128);
    tracing::info!("Ambrogio initialisation took {elapsed}µs");
    
    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let start = SystemTime::now();

        // Yuck, is there a way to avoid rebuilding the entire dependency tree every time?
        let super_user_id = AmbrogioUserId(super_user_id.0);
        let telegram_proxy = Arc::new(TeloxideProxy::new(&bot.clone()));
        let repo = repo.clone();
        let handlers: Vec<Arc<dyn MessageHandler + Sync + Send>> = vec![
            Arc::new(ForecastHandler::new(telegram_proxy.clone(), forecast_client.clone())),
            Arc::new(UserHandler::new(telegram_proxy.clone(), repo.clone())),
            Arc::new(EchoMessageHandler::new(telegram_proxy.clone())),
        ];

        let elapsed = SystemTime::now().duration_since(start).map(|d| d.as_micros()).unwrap_or(0u128);
        tracing::info!("Dependency initialisation took {elapsed}µs");

        async move {
            let message = match extract_message(&msg, super_user_id) {
                None => return Ok(()),
                Some(msg) => msg,
            };

            let message = match authenticate_user(message, repo.clone()).await {
                Err(e) => {
                    tracing::info!("Unable to authenticate message: {e}");
                    return Ok(())
                },
                Ok(msg) => msg,
            };
            let user = message.user.id();

            match handlers.iter().find(|h| h.can_accept(&message)) {
                None => tracing::info!("Unrecognised command from {:?} '{}'", user, message.text),
                Some(handler) => {
                    if let Some(error) = handler.handle(message).await.err() {
                        let _ = telegram_proxy
                            .send_text_to_user(format!("Unable to execute command: {error}"), user)
                            .await;
                    }
                },
            };

            let elapsed = SystemTime::now().duration_since(start).map(|d| d.as_micros()).unwrap_or(0u128);
            tracing::info!("Executing command took {elapsed}µs");
            Ok(())
        }
    })
    .await;
}

fn setup_global_tracing_subscriber() -> Result<(), String> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .json()
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| e.to_string())
}

fn super_user_id_from_env(env_var: &str) -> Result<UserId, String> {
    let string =
        env::var(env_var).map_err(|_| format!("{env_var} environment variable not found"))?;

    u64::from_str(&string)
        .map_err(|_| format!("{env_var} environment variable is not u64, got '{string}'"))
        .map(UserId)
}

fn extract_message(msg: &Message, super_user_id: AmbrogioUserId) -> Option<commands::InboundMessage> {
    msg.from()
        .zip(msg.text())
        .map(|(user, text)| commands::InboundMessage {
            text: text.to_owned(),
            user: extract_user(user, super_user_id),
        })
}

fn extract_user(user: &User, super_user_id: AmbrogioUserId) -> AmbrogioUser {
    let ambrogio_id = AmbrogioUserId(user.id.0);
    match (user.username.clone(), ambrogio_id) {
        (_, id) if id == super_user_id => AmbrogioUser::SuperUser { id, powers: () }, 
        (Some(name), id) => AmbrogioUser::NamedUser { id, name },
        (None, id) => AmbrogioUser::SimpleUser { id }
    }
}

async fn greet_master(bot: &Bot, super_user_id: UserId) -> Result<(), String> {
    let master_name = bot
        .get_chat(UserId(super_user_id.0))
        .await
        .ok()
        .and_then(|u| u.username().map(|x| x.to_owned()))
        .unwrap_or("Master".to_owned());
    
    bot
        .send_message(super_user_id, format!("Ambrog.io greets you, {master_name}!"))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn authenticate_user(message: InboundMessage, repo: Arc<RedisUserRepository>) -> Result<InboundMessage, String> {
    let user_id = message.user.id();

    if let AmbrogioUser::SuperUser { .. } = message.user {
        return Ok(message)
    }

    repo
        .get(user_id)
        .await?
        .ok_or(format!("Unknown user {}", user_id.0))
        .map(|user| InboundMessage { user, text: message.text })
}

