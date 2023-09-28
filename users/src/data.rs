use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct UserId(pub u64);

impl From<UserId> for u64 {
    fn from(val: UserId) -> Self {
        val.0
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum User {
    SimpleUser { id: UserId },
    SuperUser { id: UserId, powers: () },
    NamedUser { id: UserId, name: String },
}

impl User {
    pub fn id(&self) -> UserId {
        match *self {
            User::SimpleUser { id, .. }
            | User::NamedUser { id, .. }
            | User::SuperUser { id, .. } => id,
        }
    }

    pub fn name(&self) -> String {
        match self {
            User::SimpleUser { id: UserId(id) } => format!("User {id}"),
            User::SuperUser { .. } => "Signore".to_owned(),
            User::NamedUser { name, .. } => name.clone(),
        }
    }

    pub fn is_super_user(&self) -> bool {
        matches!(self, User::SuperUser { .. })
    }
}
