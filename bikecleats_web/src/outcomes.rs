use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub enum LoginOutcome {
    Success,
    AlreadyLoggedIn,
}

impl LoginOutcome {
    pub fn to_json(self) -> String {
        serde_json::to_string(&self).expect("should not fail")
    }
}
