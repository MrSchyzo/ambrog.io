use std::env;
use std::str::FromStr;
use log::warn;
use teloxide::prelude::*;
use teloxide::types::Recipient;

#[tokio::main]
async fn main() {
    warn!("Booting up ambrog.io");

    let bot = Bot::from_env();
    let user_id_string = env::var("USER_ID").expect("Expecting USER_ID env var");
    let user_id = u64::from_str(&user_id_string)
        .map(UserId)
        .expect("Expecting numeric USER_ID env var");
    let chat_id = ChatId::from(user_id);
    let user = Recipient::from(user_id.clone());

    let _ = bot.send_message(user, "Ambrog.io greets you, sir!").await;

    teloxide::repl(bot, move |bot: Bot, msg: Message| async move {
        let user = msg.chat.username().unwrap_or("N/A");
        let text = msg.text().unwrap_or("N/A");
        if msg.chat.id == chat_id {
            bot.send_message(chat_id, format!("{text} to you, {user}!")).await?;
        }
        Ok(())
    }).await;
}
