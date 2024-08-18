use std::sync::Arc;

use ambrogio_reminders::interface::{try_parse, Reminder, ReminderDefinition, ReminderEngine};
use ambrogio_users::data::User;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Europe;
use itertools::Itertools;
use regex::Regex;

use crate::telegram::TelegramProxy;

use super::{InboundMessage, MessageHandler};

enum Command {
    Delete { reminder_id: i32 },
    Create { definition: ReminderDefinition },
    Read { reminder_id: i32 },
    ReadAll,
    JustAnswer(String),
}

impl Command {
    fn new_create(definition: ReminderDefinition) -> Self {
        Self::Create { definition }
    }
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
        let msg = match into_command(&text, user) {
            Command::Delete { reminder_id } => {
                if self.reminder_engine.defuse(user_id.0, reminder_id).await {
                    format!("Promemoria con ID {reminder_id} eliminato")
                } else {
                    format!("Non sono riuscito a terminare il promemoria con ID {reminder_id}")
                }
            }
            Command::Create { definition } => {
                if let Some(id) = self.reminder_engine.add(definition).await {
                    format!("Promemoria creato con ID {id}")
                } else {
                    "Non sono riuscito a creare un promemoria".to_string()
                }
            }
            Command::Read { reminder_id } => self
                .reminder_engine
                .get(&user_id.0, &reminder_id)
                .as_ref()
                .map(render_full)
                .unwrap_or_else(|| format!("Non ho trovato alcun promemoria con ID {reminder_id}")),
            Command::ReadAll => {
                let reminders = self
                    .reminder_engine
                    .get_all(&user_id.0)
                    .values()
                    .sorted_by_key(|rem| {
                        rem.current_tick()
                            .map(|d| d.with_timezone(&Utc))
                            .unwrap_or(DateTime::<Utc>::MAX_UTC)
                    })
                    .map(|rem| format!("• {}", render_line(rem)))
                    .collect::<Vec<_>>();
                if !reminders.is_empty() {
                    let pages = reminders.chunks(20).collect::<Vec<_>>();
                    for (page, reminders) in pages.iter().enumerate() {
                        let list = reminders.join("\n");
                        let message = format!(
                            "Promemoria (pag. {} di {}):\n{}",
                            page + 1,
                            pages.len(),
                            list
                        );
                        let _ = self.telegram.send_text_to_user(message, user_id).await;
                    }
                    return Ok(());
                }
                "Non sono riuscito a trovare alcun promemoria".to_string()
            }
            Command::JustAnswer(msg) => msg,
        };
        let _ = self.telegram.send_text_to_user(msg, user_id).await;
        Ok(())
    }
}

fn into_command(text: &str, user: User) -> Command {
    let arguments: Vec<&str> = text.splitn(2, '\n').filter(|txt| !txt.is_empty()).collect();
    let lower_tokens = arguments[0]
        .split(' ')
        .filter(|e| !e.is_empty())
        .map(|e| e.trim_matches(&[',', ':', '.', '!', '\n']))
        .flat_map(|s| s.split('\''))
        .map(|s| s.to_lowercase())
        .collect::<Vec<String>>();

    let tokens = lower_tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();

    match tokens.first().copied() {
        Some("promemoria") => into_promemoria(tokens),
        Some("ricordami") if arguments.len() > 1 => {
            into_ricordami(tokens, arguments[1].trim_start_matches('\n'), user)
        }
        Some("scordati") => into_scordati(tokens),
        x => {
            tracing::info!("Received pragma: {:?}", x);
            generic_help()
        }
    }
}

fn into_promemoria(tokens: Vec<&str>) -> Command {
    if tokens.contains(&"miei") {
        return Command::ReadAll;
    }

    for token in tokens {
        if let Ok(reminder_id) = token.parse::<i32>() {
            return Command::Read { reminder_id };
        }
    }

    promemoria_help()
}

fn into_scordati(tokens: Vec<&str>) -> Command {
    for token in tokens {
        if let Ok(reminder_id) = token.parse::<i32>() {
            return Command::Delete { reminder_id };
        }
    }

    scordati_help()
}

fn into_ricordami(tokens: Vec<&str>, message: &str, user: User) -> Command {
    try_parse(tokens, &Utc::now())
        .map(|schedule| ReminderDefinition::new(schedule, user.id().0, message.to_owned()))
        .map(Command::new_create)
        .unwrap_or_else(ricordami_help)
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

fn render_full(reminder: &Reminder) -> String {
    let current_tick = reminder
        .current_tick()
        .map(|d| {
            d.with_timezone(&Europe::Rome)
                .format("%d/%m/%Y %T %Z")
                .to_string()
        })
        .unwrap_or_else(|| "N.D. (Terminato)".to_owned());
    let message = reminder.message().as_str().to_owned();
    let id = reminder.reminder_id().1;
    format!(
        r#"📝 Promemoria ID: {id}
🕰️ Prossima scadenza: {current_tick}

💬 Messaggio
{message}
"#,
    )
}

fn render_line(reminder: &Reminder) -> String {
    let current_tick = reminder
        .current_tick()
        .map(|d| {
            d.with_timezone(&Europe::Rome)
                .format("%d/%m/%y %H:%M %Z")
                .to_string()
        })
        .unwrap_or_else(|| "Terminato".to_owned());
    let message = {
        let msg = reminder.message().as_str().to_owned();
        match msg.len() {
            ..=50 => msg,
            _ => format!("{:.47}...", msg),
        }
    };

    let id = reminder.reminder_id().1;
    format!("[ID {id}, {current_tick}]: {message}")
}
