use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct UserId(pub u64);

impl Into<u64> for UserId {
    fn into(self) -> u64 {
        return self.0
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum User {
    SimpleUser {
        id: UserId,
    },
    SuperUser {
        id: UserId,
        powers: (),
    },
    NamedUser {
       id: UserId,
       name: String
    }
}

impl User {
    pub fn id(&self) -> UserId {
        match self {
            &User::SimpleUser { id, .. } => id,
            &User::NamedUser { id, .. } => id,
            &User::SuperUser { id, .. } => id,
        }
    }

    pub fn name(&self) -> String {
        match self {
            User::SimpleUser { id: UserId(id) } => format!("User {id}"),
            User::SuperUser { .. } => format!("Master"),
            User::NamedUser { name, .. } => name.clone(),
        }
    }

    pub fn is_super_user(&self) -> bool {
        matches!(self, User::SuperUser { .. })
    }
}

