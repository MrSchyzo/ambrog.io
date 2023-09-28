pub mod data;

use async_trait::async_trait;
use data::{User, UserId};
use redis::AsyncCommands;

#[async_trait]
pub trait UserRepository {
    async fn get(&self, id: UserId) -> Result<Option<User>, String>;
    async fn set(&self, user: User) -> Result<bool, String>;
    async fn remove(&self, id: UserId) -> Result<bool, String>;
}

#[derive(Clone)]
pub struct RedisUserRepository {
    connection: redis::aio::MultiplexedConnection,
}

impl RedisUserRepository {
    pub fn new(connection: redis::aio::MultiplexedConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl UserRepository for RedisUserRepository {
    async fn get(&self, UserId(id): UserId) -> Result<Option<User>, String> {
        let mut conn = self.connection.clone();
        let user: Option<String> = conn
            .get(format!("user:{id}"))
            .await
            .map_err(|e| format!("{e}"))?;

        user.map(|u| serde_json::from_str::<User>(&u))
            .transpose()
            .map_err(|e| e.to_string())
    }

    async fn set(&self, user: User) -> Result<bool, String> {
        let mut conn = self.connection.clone();
        let UserId(id) = user.id();
        let payload = serde_json::to_string(&user).map_err(|e| e.to_string())?;
        let _ = conn
            .set(format!("user:{id}"), payload)
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(true)
    }

    async fn remove(&self, UserId(id): UserId) -> Result<bool, String> {
        let mut conn = self.connection.clone();
        let _ = conn
            .del(format!("user:{id}"))
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(true)
    }
}
