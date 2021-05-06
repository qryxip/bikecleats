use std::collections::BTreeSet;
use url::Url;

pub enum ProblemInContest {
    Index { contest: String, problem: String },
    Url { url: Url },
}

#[derive(Clone)]
pub enum ProblemsInContest {
    Indexes {
        contest: String,
        problems: Option<BTreeSet<String>>,
    },
    Urls {
        urls: BTreeSet<Url>,
    },
}

pub enum SystemTestCases<F> {
    None,
    AccessToken(F),
}

impl SystemTestCases<fn(&'static str) -> anyhow::Result<String>> {
    pub fn none() -> Self {
        Self::None
    }
}
