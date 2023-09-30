mod commands;
mod info;
mod telegram;
mod update_listener;

use ambrogio_users::data::User as AmbrogioUser;
use ambrogio_users::data::UserId as AmbrogioUserId;
use ambrogio_users::RedisUserRepository;
use ambrogio_users::UserRepository;
use async_once_cell::OnceCell;
use commands::ferrero::FerreroHandler;
use commands::shutdown::ShutdownHandler;
use commands::InboundMessage;
use open_meteo::ReqwestForecastClient;
use redis::aio::MultiplexedConnection;
use std::env;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use telegram::TelegramProxy;
use teloxide::prelude::*;
use teloxide::types::User;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use crate::commands::echo::EchoMessageHandler;
use crate::commands::forecast::ForecastHandler;
use crate::commands::users::UserHandler;
use crate::commands::MessageHandler;
use crate::info::VERSION;
use crate::telegram::TeloxideProxy;

type Handler = dyn MessageHandler + Send + Sync;

static HANDLERS: OnceCell<Vec<Arc<Handler>>> = OnceCell::new();
static TELEGRAM: OnceCell<TeloxideProxy> = OnceCell::new();
static USERS: OnceCell<Arc<RedisUserRepository>> = OnceCell::new();
static REDIS: OnceCell<Arc<MultiplexedConnection>> = OnceCell::new();

#[tokio::main]
async fn main() {
    let start = SystemTime::now();

    setup_global_tracing_subscriber().unwrap();
    tracing::info!("Booting up ambrog.io version {VERSION}");

    let bot = Bot::from_env();

    let super_user_id = match super_user_id_from_env("USER_ID") {
        Ok(u) => u,
        Err(str) => {
            tracing::error!("Unable to get super user id: {str}");
            return;
        }
    };

    tokio::spawn({
        let bot = bot.clone();
        let redis = get_redis_connection().await.unwrap().as_ref().clone();
        async move {
            if let Err(e) =
                update_listener::run_embedded_web_listener(bot, super_user_id, redis).await
            {
                tracing::error!("Cannot listen to docker image updates: {e}")
            }
        }
    });

    greet_master(&bot, super_user_id).await.unwrap();

    let elapsed = SystemTime::now()
        .duration_since(start)
        .map(|d| d.as_micros())
        .unwrap_or(0u128);
    tracing::info!("Ambrog.io initialisation took {elapsed}µs");

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let start = SystemTime::now();

        let super_user_id = AmbrogioUserId(super_user_id.0);
        async move {
            let repo = get_users_repo().await.unwrap();
            let telegram = get_telegram(&bot).await;
            let handlers = get_handlers(&bot).await.unwrap();

            let message = match extract_message(&msg, super_user_id) {
                None => return Ok(()),
                Some(msg) => msg,
            };

            let message = match authenticate_user(message, repo).await {
                Err(e) => {
                    tracing::info!("Unable to authenticate message: {e}");
                    return Ok(());
                }
                Ok(msg) => msg,
            };

            let user = message.user.id();

            match handlers.iter().find(|h| h.can_accept(&message)) {
                None => tracing::info!("Unrecognised command from {:?} '{}'", user, message.text),
                Some(handler) => {
                    if let Some(error) = handler.handle(message).await.err() {
                        let _ = telegram
                            .send_text_to_user(format!("Unable to execute command: {error}"), user)
                            .await;
                    }
                }
            };

            let elapsed = SystemTime::now()
                .duration_since(start)
                .map(|d| d.as_micros())
                .unwrap_or(0u128);
            tracing::info!("Executing command took {elapsed}µs");
            Ok(())
        }
    })
    .await;
}

async fn get_telegram(bot: &Bot) -> &(dyn TelegramProxy + Send + Sync) {
    TELEGRAM
        .get_or_init(async { TeloxideProxy::new(bot) })
        .await
}

async fn get_handlers(bot: &Bot) -> Result<&Vec<Arc<Handler>>, String> {
    HANDLERS.get_or_try_init(setup_handlers(bot)).await
}

async fn setup_handlers(bot: &Bot) -> Result<Vec<Arc<Handler>>, String> {
    let client = reqwest::ClientBuilder::new()
        .build()
        .map_err(|e| e.to_string())?;
    let rocher_url = "https://67kqts2llyhkzax72fivullwhuo7ifgux6qlfavaherscx4xv3ca.arweave.net/99UJy0teDqyC_9FRWi12PR30FNS_oLKCoDkjIV-XrsQ".to_owned();
    let repo = get_users_repo().await?;

    let forecast_client = Arc::new(ReqwestForecastClient::new(
        &client,
        "https://geocoding-api.open-meteo.com".to_owned(),
        "https://api.open-meteo.com".to_owned(),
    ));
    let telegram_proxy = Arc::new(TeloxideProxy::new(&bot.clone()));

    Ok(vec![
        Arc::new(ForecastHandler::new(
            telegram_proxy.clone(),
            forecast_client.clone(),
        )),
        Arc::new(UserHandler::new(telegram_proxy.clone(), repo.clone())),
        Arc::new(FerreroHandler::new(telegram_proxy.clone(), rocher_url)),
        Arc::new(ShutdownHandler::new(telegram_proxy.clone())),
        Arc::new(EchoMessageHandler::new(telegram_proxy.clone())),
    ])
}

async fn get_redis_connection() -> Result<Arc<MultiplexedConnection>, String> {
    REDIS
        .get_or_try_init(async {
            let redis = env::var("REDIS_URL")
                .or(Ok("redis://127.0.0.1".to_owned()))
                .and_then(redis::Client::open)
                .map_err(|e| e.to_string())?;

            redis
                .get_multiplexed_tokio_connection()
                .await
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
        .await
        .cloned()
}

async fn get_users_repo() -> Result<Arc<RedisUserRepository>, String> {
    USERS
        .get_or_try_init(async {
            let connection = get_redis_connection().await?.as_ref().clone();

            Ok(Arc::new(RedisUserRepository::new(connection)))
        })
        .await
        .cloned()
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

    tracing::subscriber::set_global_default(subscriber).map_err(|e| e.to_string())
}

fn super_user_id_from_env(env_var: &str) -> Result<UserId, String> {
    let string =
        env::var(env_var).map_err(|_| format!("{env_var} environment variable not found"))?;

    u64::from_str(&string)
        .map_err(|_| format!("{env_var} environment variable is not u64, got '{string}'"))
        .map(UserId)
}

fn extract_message(
    msg: &Message,
    super_user_id: AmbrogioUserId,
) -> Option<commands::InboundMessage> {
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
        (None, id) => AmbrogioUser::SimpleUser { id },
    }
}

async fn greet_master(bot: &Bot, super_user_id: UserId) -> Result<(), String> {
    let master_name = bot
        .get_chat(UserId(super_user_id.0))
        .await
        .ok()
        .and_then(|u| u.username().map(|x| x.to_owned()))
        .unwrap_or("Padrone".to_owned());

    bot.send_message(
        super_user_id,
        format!("Ambrog.io v{VERSION} al Suo servizio, {master_name}!"),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

async fn authenticate_user(
    message: InboundMessage,
    repo: Arc<RedisUserRepository>,
) -> Result<InboundMessage, String> {
    let user_id = message.user.id();

    if let AmbrogioUser::SuperUser { .. } = message.user {
        return Ok(message);
    }

    repo.get(user_id)
        .await?
        .ok_or(format!("Unknown user {}", user_id.0))
        .map(|user| InboundMessage {
            user,
            text: message.text,
        })
}
