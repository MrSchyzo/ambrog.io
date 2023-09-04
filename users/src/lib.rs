use async_trait::async_trait;
use redis::AsyncCommands;

#[async_trait]
pub trait UserRepository {
    async fn get(&self, id: u64) -> Result<Option<User>, String>;
    async fn set(&self, user: User) -> Result<bool, String>;
    async fn remove(&self, id: u64) -> Result<bool, String>;
}

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
    async fn get(&self, id: u64) -> Result<Option<User>, String> {
        let mut conn = self.connection.clone();
        let user_id: Option<u64> = conn.get(format!("user:{id}")).await.map_err(|e| format!("{e}"))?;
        Ok(user_id.map(|_| User {id}))
    }
    async fn set(&self, user: User) -> Result<bool, String> {
        let mut conn = self.connection.clone();
        let id = user.id;
        let _ = conn.set(format!("user:{id}"), id).await.map_err(|e| format!("{e}"))?;
        Ok(true)
    }
    async fn remove(&self, id: u64) -> Result<bool, String> {
        let mut conn = self.connection.clone();
        let _ = conn.del(format!("user:{id}")).await.map_err(|e| format!("{e}"))?;
        Ok(true)
    }
}

pub struct User {
    pub id: u64
}
