use bikecleats_testsuite::TestSuite;
use derive_more::From;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use url::Url;

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

#[derive(Debug, Clone, Copy, From, Serialize)]
pub enum ParticipateOutcome {
    Success,
    AlreadyParticipated,
    ContestIsFinished,
}

impl ParticipateOutcome {
    pub fn to_json(self) -> String {
        serde_json::to_string(&self).expect("should not fail")
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::Success => "Successfully participated.",
            Self::AlreadyParticipated => "Already participated.",
            Self::ContestIsFinished => "The contest is already finished.",
        }
    }
}
#[non_exhaustive]
#[derive(Debug, Serialize)]
pub struct RetrieveTestCasesOutcome {
    pub problems: Vec<RetrieveTestCasesOutcomeProblem>,
}

#[non_exhaustive]
#[derive(Debug, Serialize)]
pub struct RetrieveTestCasesOutcomeProblem {
    pub contest: Option<RetrieveTestCasesOutcomeProblemContest>,
    pub index: String,
    pub url: Url,
    pub screen_name: Option<String>,
    pub display_name: String,
    pub test_suite: TestSuite,
    pub text_files: IndexMap<String, RetrieveTestCasesOutcomeProblemTextFiles>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct RetrieveTestCasesOutcomeProblemContest {
    pub id: String,
    pub display_name: String,
    pub url: Url,
    pub submissions_url: Url,
}

#[derive(Debug, Serialize)]
pub struct RetrieveTestCasesOutcomeProblemTextFiles {
    pub r#in: String,
    pub out: Option<String>,
}
