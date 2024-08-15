pub mod echo;
pub mod ferrero;
pub mod forecast;
pub mod reminders;
pub mod shutdown;
pub mod users;
pub mod youtube;

use ambrogio_users::data::User;
use async_trait::async_trait;

#[derive(Debug)]
pub struct InboundMessage {
    pub user: User,
    pub text: String,
}

#[async_trait]
pub trait MessageHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool;
    async fn handle(&self, msg: InboundMessage) -> Result<(), String>;
}
