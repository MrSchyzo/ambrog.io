use std::env;

use async_once_cell::OnceCell;
use std::str::FromStr;

pub struct AmbrogioConfig {
    pub user_id: u64,
    pub redis: RedisConfig,
    pub mongo: MongoConfig,
    pub ferrero: FerreroConfig,
    pub updates: UpdatesConfig,
    pub forecast: ForecastConfig,
}

pub struct RedisConfig {
    pub url: String,
}

pub struct MongoConfig {
    pub url: String,
    pub db: String,
}

pub struct FerreroConfig {
    pub gif_url: String,
}

pub struct ForecastConfig {
    pub forecast_root: String,
    pub geocoding_root: String,
}

#[derive(Clone)]
pub struct UpdatesConfig {
    pub webhook_domain: String,
    pub redis_topic: String,
}

static CONFIG: OnceCell<AmbrogioConfig> = OnceCell::new();

pub async fn get_config() -> &'static AmbrogioConfig {
    CONFIG.get_or_try_init::<String>(async {
        let gif = "https://67kqts2llyhkzax72fivullwhuo7ifgux6qlfavaherscx4xv3ca.arweave.net/99UJy0teDqyC_9FRWi12PR30FNS_oLKCoDkjIV-XrsQ".to_owned();
        Ok(AmbrogioConfig {
            user_id: env_var_as_u64("USER_ID")?,
            redis: RedisConfig {
                url: env_var("REDIS_URL").unwrap_or("redis://127.0.0.1".to_owned())
            },
            mongo: MongoConfig {
                url: env_var("MONGO_URL").unwrap_or("mongodb://127.0.0.1:27017".to_owned()),
                db: env_var("MONGO_DB").unwrap_or("ambrogio".to_owned()),
            },
            ferrero: FerreroConfig {
                gif_url: env_var("FERRERO_GIF_URL").unwrap_or(gif),
            },
            updates: UpdatesConfig {
                webhook_domain: env_var("UPDATES_WEBHOOK_DOMAIN")?,
                redis_topic: env_var("UPDATES_REDIS_TOPIC").unwrap_or("updates".to_owned())
            },
            forecast: ForecastConfig {
                forecast_root: env_var("FORECAST_MAIN_ROOT").unwrap_or("https://api.open-meteo.com".to_owned()), 
                geocoding_root: env_var("FORECAST_GEO_ROOT").unwrap_or("https://geocoding-api.open-meteo.com".to_owned()) 
            }
        })
    })
    .await
    .unwrap()
}

fn env_var(var_name: &str) -> Result<String, String> {
    env::var(var_name).map_err(|e| format!("Unable to find {var_name} in env: {e}"))
}

fn env_var_as_u64(var_name: &str) -> Result<u64, String> {
    env_var(var_name).and_then(|s| u64::from_str(&s).map_err(|e| e.to_string()))
}
