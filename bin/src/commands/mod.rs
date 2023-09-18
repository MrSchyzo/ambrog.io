mod echo;
mod forecast;

use ambrogio_users::data::User;
use async_trait::async_trait;

#[derive(Debug)]
pub(crate) struct InboundMessage {
    pub user: User,
    pub text: String,
}

#[async_trait]
pub trait MessageHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool;
    async fn handle(&self, msg: InboundMessage) -> Result<(), String>;
}