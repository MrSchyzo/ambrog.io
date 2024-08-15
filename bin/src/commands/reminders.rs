use std::sync::Arc;

use ambrogio_reminders::interface::{ReminderDefinition, ReminderEngine, Schedule};
use ambrogio_users::data::User;
use async_trait::async_trait;
use regex::Regex;

use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

enum Command {
    Delete { user: User, reminder_id: i32 },
    Create { definition: ReminderDefinition },
    Read { user: User, reminder_id: i32 },
    ReadAll { user: User },
    JustAnswer(String),
}

pub struct RemindersHandler {
    telegram: Arc<dyn TelegramProxy + Send + Sync + 'static>,
    reminder_engine: Arc<ReminderEngine>,
    regex: Regex,
}

impl RemindersHandler {
    pub fn new<Proxy>(telegram: Arc<Proxy>, engine: Arc<ReminderEngine>) -> Self
    where
        Proxy: TelegramProxy + Send + Sync + 'static,
    {
        Self {
            telegram,
            reminder_engine: engine,
            regex: Regex::new(r"(?i)^(ricordami\s+|scordati\s+|promemoria)").unwrap(),
        }
    }
}

#[async_trait]
impl MessageHandler for RemindersHandler {
    fn can_accept(&self, msg: &InboundMessage) -> bool {
        self.regex.is_match(&msg.text)
    }

    async fn handle(&self, InboundMessage { user, text }: InboundMessage) -> Result<(), String> {
        let user_id = user.id();
        match into_command(&text, user) {
            Command::Delete { user, reminder_id } => {
                self.reminder_engine.defuse(user.id().0, reminder_id).await;
            }
            Command::Create { definition } => {
                self.reminder_engine.add(definition).await;
            }
            Command::Read { user, reminder_id } => {
                let _ = self
                    .telegram
                    .send_text_to_user(format!("TODO: read {reminder_id}"), user_id)
                    .await;
            }
            Command::ReadAll { user } => {
                let _ = self
                    .telegram
                    .send_text_to_user("TODO: readAll".to_owned(), user_id)
                    .await;
            }
            Command::JustAnswer(msg) => {
                let _ = self.telegram.send_text_to_user(msg, user_id).await;
            }
        }
        Ok(())
    }
}

fn into_command(text: &str, user: User) -> Command {
    let arguments: Vec<&str> = text.splitn(2, '\n').collect();
    tracing::info!("Arguments: {:?}", arguments);
    tracing::info!("Text: {}", text);
    let tokens = arguments[0]
        .split(' ')
        .filter(|e| !e.is_empty())
        .map(|e| e.trim_matches(&[',', ':', '.', '!', '\n']))
        .collect::<Vec<&str>>();
    match tokens.first().map(|x| x.to_lowercase()) {
        Some(x) if x == "promemoria" => into_promemoria(tokens, user),
        Some(x) if x == "ricordami" && arguments.len() > 1 => {
            into_ricordami(tokens, arguments[1], user)
        }
        Some(x) if x == "scordati" => into_scordati(tokens, user),
        x => {
            tracing::info!("Received pragma: {:?}", x);
            generic_help()
        }
    }
}

fn into_promemoria(tokens: Vec<&str>, user: User) -> Command {
    if tokens.contains(&"miei") {
        return Command::ReadAll { user };
    }

    for token in tokens {
        if let Ok(reminder_id) = token.parse::<i32>() {
            return Command::Read { user, reminder_id };
        }
    }

    promemoria_help()
}

fn into_scordati(tokens: Vec<&str>, user: User) -> Command {
    for token in tokens {
        if let Ok(reminder_id) = token.parse::<i32>() {
            return Command::Delete { user, reminder_id };
        }
    }

    scordati_help()
}

fn into_ricordami(_: Vec<&str>, message: &str, user: User) -> Command {
    let schedule = Schedule::every_fucking_minute_of_your_damn_life();

    return Command::Create {
        definition: ReminderDefinition::new(schedule, user.id().0, message.to_owned()),
    };

    ricordami_help()
}

fn generic_help() -> Command {
    Command::JustAnswer(
        r##"Sono costernato, ma non ho compreso il Suo desiderio.
Provi a scrivermi `ricordami`, `scordati`, `promemoria` così da aiutarmi ad aiutarla!
"##
        .to_string(),
    )
}

fn promemoria_help() -> Command {
    Command::JustAnswer(
        r##"Sono costernato, ma non ho compreso il Suo desiderio.
Scriva `promemoria miei` per vedere una lista dei suoi promemoria.
Oppure scriva `promemoria <N>` (<N> è un numero) per vedere il promemoria identificato con N.
"##
        .to_string(),
    )
}

fn scordati_help() -> Command {
    Command::JustAnswer(
        r##"Sono costernato, ma non ho compreso il Suo desiderio.
Scriva `scordati <N>` (<N> è un numero) per eliminare il promemoria identificato con N.
"##
        .to_string(),
    )
}

fn ricordami_help() -> Command {
    Command::JustAnswer(
        r##"Sono costernato, ma non ho compreso il Suo desiderio. 
Appena faccio mente locale Le faccio sapere come chiedermi di aggiungere promemoria."##
            .to_string(),
    )
}
