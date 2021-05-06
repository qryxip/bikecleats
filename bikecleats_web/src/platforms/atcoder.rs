use crate::{
    args::{ProblemsInContest, SystemTestCases},
    outcomes::{
        LoginOutcome, ParticipateOutcome, RetrieveTestCasesOutcome,
        RetrieveTestCasesOutcomeProblem, RetrieveTestCasesOutcomeProblemContest,
        RetrieveTestCasesOutcomeProblemTextFiles,
    },
    request::{BlockingClientExt as _, BlockingResponseExt},
    shell::Shell,
};
use anyhow::{anyhow, bail, ensure, Context as _};
use bikecleats_testsuite::{
    BatchTestSuite, InteractiveTestSuite, Match, PartialBatchTestCase, PositiveFinite, TestSuite,
};
use camino::Utf8Path;
use chrono::{DateTime, Local, Utc};
use easy_ext::ext;
use indexmap::{indexmap, IndexMap};
use indicatif::ProgressDrawTarget;
use itertools::Itertools as _;
use maplit::{btreemap, hashmap, hashset};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
use serde_json::json;
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    marker::PhantomData,
    mem,
    str::FromStr,
    time::Duration,
};
use url::Url;

static BASE_URL: Lazy<Url> = lazy_url!("https://atcoder.jp");

pub(crate) fn login(
    client: &reqwest::blocking::Client,
    mut shell: impl Shell,
    username_and_password: impl FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
) -> anyhow::Result<LoginOutcome> {
    if check_logged_in(client, &mut shell)? {
        Ok(LoginOutcome::AlreadyLoggedIn)
    } else {
        ensure_login(client, &mut shell, username_and_password)?;
        Ok(LoginOutcome::Success)
    }
}

fn check_logged_in(
    client: &reqwest::blocking::Client,
    shell: &mut impl Shell,
) -> anyhow::Result<bool> {
    let status = client
        .get_with_shell(url!("/settings"), shell)
        .colorize_status_code(&[200], &[302], ..)
        .send()?
        .ensure_status(&[200, 302])?
        .status();
    Ok(status == 200)
}

fn ensure_login(
    client: &reqwest::blocking::Client,
    mut shell: impl Shell,
    mut username_and_password: impl FnMut(
        &'static str,
        &'static str,
    ) -> anyhow::Result<(String, String)>,
) -> anyhow::Result<()> {
    while {
        let (username, password) = username_and_password("Username: ", "Password: ")?;

        let csrf_token = client
            .get_with_shell(url!("/login"), &mut shell)
            .colorize_status_code(&[200], (), ..)
            .send()?
            .ensure_status(&[200])?
            .html()?
            .extract_csrf_token()?;

        client
            .post_with_shell(url!("/login"), &mut shell)
            .form(&hashmap! {
                "csrf_token" => csrf_token,
                "username" => username,
                "password" => password,
            })
            .colorize_status_code(&[302], (), ..)
            .send()?
            .ensure_status(&[302])?;

        !check_logged_in(client, &mut shell)?
    } {}
    Ok(())
}

pub(crate) fn participate(
    client: &reqwest::blocking::Client,
    shell: impl Shell,
    username_and_password: impl FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
    contest: &str,
) -> anyhow::Result<ParticipateOutcome> {
    participate_if_not(
        client,
        shell,
        username_and_password,
        &CaseConverted::new(contest),
        true,
    )
}

fn participate_if_not(
    client: &reqwest::blocking::Client,
    mut shell: impl Shell,
    username_and_password: impl FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
    contest: &CaseConverted<LowerCase>,
    explicit: bool,
) -> anyhow::Result<ParticipateOutcome> {
    let res = client
        .get_with_shell(url!("/contests/{}", contest), &mut shell)
        .colorize_status_code(&[200], (), ..)
        .send()?
        .ensure_status(&[200, 404])?;

    let html = res.html()?;

    let status = ContestStatus::now(html.extract_contest_duration()?, contest);
    if !explicit {
        status.raise_if_not_begun()?;
    }

    login(client, &mut shell, username_and_password)?;

    if status.is_finished() {
        Ok(ParticipateOutcome::ContestIsFinished)
    } else {
        let html = client
            .get_with_shell(url!("/contests/{}", contest), &mut shell)
            .colorize_status_code(&[200], (), ..)
            .send()?
            .ensure_status(&[200])?
            .html()?;

        if html.contains_registration_button()? {
            let csrf_token = html.extract_csrf_token()?;

            client
                .post_with_shell(url!("/contests/{}/register", contest), shell)
                .form(&hashmap!("csrf_token" => csrf_token))
                .colorize_status_code(&[302], (), ..)
                .send()?
                .ensure_status(&[302])?;

            Ok(ParticipateOutcome::Success)
        } else {
            Ok(ParticipateOutcome::AlreadyParticipated)
        }
    }
}

pub(crate) fn retrieve_test_cases(
    blocking_client: &reqwest::blocking::Client,
    async_client: &reqwest::Client,
    mut shell: impl Shell,
    username_and_password: impl FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
    system: SystemTestCases<impl FnMut(&'static str) -> anyhow::Result<String>>,
    targets: &ProblemsInContest,
) -> anyhow::Result<RetrieveTestCasesOutcome> {
    let mut outcome =
        retrieve_sample_test_cases(blocking_client, &mut shell, username_and_password, targets)?;
    if let SystemTestCases::AccessToken(dropbox_access_token) = system {
        retrieve_system_test_cases(
            blocking_client,
            async_client,
            shell,
            dropbox_access_token,
            &mut outcome,
        )?;
    }
    todo!();
}

fn retrieve_sample_test_cases(
    client: &reqwest::blocking::Client,
    mut shell: impl Shell,
    mut username_and_password: impl FnMut(
        &'static str,
        &'static str,
    ) -> anyhow::Result<(String, String)>,
    targets: &ProblemsInContest,
) -> anyhow::Result<RetrieveTestCasesOutcome> {
    let problems = match targets.clone() {
        ProblemsInContest::Indexes { contest, problems } => {
            let contest = CaseConverted::<LowerCase>::new(contest);
            let html = retrieve_tasks_page(client, &mut shell, username_and_password, &contest)?;

            let contest_display_name = html
                .extract_title()?
                .trim_start_matches("Tasks - ")
                .to_owned();

            let only = &mut problems
                .as_ref()
                .map(|ps| ps.iter().map(|p| p.to_uppercase()).collect::<BTreeSet<_>>());

            let index_url_pairs = html
                .extract_task_indexes_and_urls()?
                .into_iter()
                .filter(|(index, _)| {
                    if let Some(only) = only {
                        only.remove(&**index)
                    } else {
                        true
                    }
                })
                .collect::<IndexMap<_, _>>();

            if let Some(only) = only {
                ensure!(only.is_empty(), "No such problems: {:?}", only);
            }

            btreemap!(contest => (contest_display_name, index_url_pairs))
        }
        ProblemsInContest::Urls { urls } => {
            fn contest_id_from_url(url: &Url) -> anyhow::Result<String> {
                if url.domain() != Some("atcoder.jp") {
                    bail!("wrong domain. expected `atcoder.jp`: {}", url);
                }

                static_regex!(r"\A/contests/([a-z0-9_\-]+)/.*\z$")
                    .captures(url.path())
                    .map(|caps| caps[1].to_owned())
                    .with_context(|| "Could not extract contest ID of the problem")
            }

            let mut problems: BTreeMap<_, (_, _, HashSet<_>)> = btreemap!();

            for url in urls {
                let contest = CaseConverted::new(contest_id_from_url(&url)?);

                if let Some((_, _, only)) = problems.get_mut(&contest) {
                    only.insert(url);
                } else {
                    let html = retrieve_tasks_page(
                        client,
                        &mut shell,
                        &mut username_and_password,
                        &contest,
                    )?;
                    let contest_display_name = html
                        .extract_title()?
                        .trim_start_matches("Tasks - ")
                        .to_owned();
                    let indexes_and_urls = html.extract_task_indexes_and_urls()?;
                    problems.insert(
                        contest,
                        (contest_display_name, indexes_and_urls, hashset!(url)),
                    );
                }
            }

            problems
                .into_iter()
                .map(
                    |(contest, (contest_display_name, indexes_and_urls, only))| {
                        let indexes_and_urls = indexes_and_urls
                            .into_iter()
                            .filter(|(_, url)| only.contains(url))
                            .collect();
                        (contest, (contest_display_name, indexes_and_urls))
                    },
                )
                .collect()
        }
    };

    let mut outcome = RetrieveTestCasesOutcome { problems: vec![] };

    for (contest, (contest_display_name, mut index_url_pairs)) in problems {
        let test_suites = client
            .get_with_shell(url!("/contests/{}/tasks_print", contest), &mut shell)
            .colorize_status_code(&[200], (), ..)
            .send()?
            .ensure_status(&[200])?
            .html()?
            .extract_samples();

        if index_url_pairs.len() > test_suites.len() {
            shell.warn(format_args!(
                "Found {} task(s) in `tasks`, {} task(s) in `tasks_print`",
                index_url_pairs.len(),
                test_suites.len(),
            ))?;
        }

        let contest = &RetrieveTestCasesOutcomeProblemContest {
            id: (*contest).to_owned(),
            display_name: contest_display_name,
            url: url!("/contests/{}", contest),
            submissions_url: url!("/contests/{}/submissions/me", contest),
        };

        for result in test_suites {
            match result {
                Ok((index, display_name, test_suite)) => {
                    if let Some(url) = index_url_pairs.shift_remove(&*index) {
                        let screen_name = url
                            .path_segments()
                            .and_then(Iterator::last)
                            .with_context(|| "Empty URL")?
                            .to_owned();

                        let test_suite = match test_suite {
                            Ok(test_suite) => test_suite,
                            Err(err) => {
                                shell.warn(err)?;
                                TestSuite::Batch(BatchTestSuite {
                                    timelimit: None,
                                    r#match: Match::Lines,
                                    cases: vec![],
                                    extend: vec![],
                                })
                            }
                        };

                        outcome.problems.push(RetrieveTestCasesOutcomeProblem {
                            contest: Some(contest.clone()),
                            url,
                            index,
                            screen_name: Some(screen_name),
                            display_name,
                            test_suite,
                            text_files: indexmap!(),
                        });
                    }
                }
                Err(err) => shell.warn(err)?,
            }
        }

        for (index, _) in index_url_pairs {
            shell.warn(format!("could not find `{}` in `tasks_print`", index))?;
        }
    }

    Ok(outcome)
}

fn retrieve_system_test_cases(
    blocking_client: &reqwest::blocking::Client,
    async_client: &reqwest::Client,
    mut shell: impl Shell,
    dropbox_access_token: impl FnOnce(&'static str) -> anyhow::Result<String>,
    outcome: &mut RetrieveTestCasesOutcome,
) -> anyhow::Result<()> {
    let dropbox_access_token = &dropbox_access_token("Dropbox Access Token: ")?;

    for outcome in &mut outcome.problems {
        let path_prefix = {
            let contest = &outcome.contest.as_ref().expect("should be `Some`").id;
            DROPBOX_PATH_PREFIXES
                .get(contest)
                .cloned()
                .unwrap_or_else(|| format!("/{}/", contest))
        };

        let problem_dir = &format!("{}{}", path_prefix, outcome.index);

        let problem_dir_entries = list_paths_with_filter(
            blocking_client,
            &mut shell,
            dropbox_access_token,
            problem_dir,
        )?;

        let mut list_file_paths = |in_out_dir_file_name: &'static str| -> _ {
            if problem_dir_entries.contains_folder(in_out_dir_file_name) {
                list_paths_with_filter(
                    blocking_client,
                    &mut shell,
                    dropbox_access_token,
                    &format!("{}{}/{}", path_prefix, outcome.index, in_out_dir_file_name),
                )
                .map(|es| es.file_paths())
            } else {
                Ok(vec![])
            }
        };

        let (in_file_paths, out_file_paths) = match *problem_dir_entries.folder_filenames() {
            ["in", "out"] => (list_file_paths("in")?, list_file_paths("out")?),
            ["in"] => (list_file_paths("in")?, vec![]),
            ["out"] => (problem_dir_entries.file_paths(), list_file_paths("out")?),
            [] => (problem_dir_entries.file_paths(), vec![]),
            _ => bail!(
                "unexpected format (path-prefix: {:?}, files: {:?}, folders: {:?})",
                path_prefix,
                problem_dir_entries.file_paths(),
                problem_dir_entries.folder_paths(),
            ),
        };

        let retrieve_files = |file_paths| -> anyhow::Result<_> {
            retrieve_files(
                async_client,
                shell.progress_draw_target(),
                &dropbox_access_token,
                file_paths,
            )
        };
        let in_contents = retrieve_files(&in_file_paths)?;
        let mut out_contents = retrieve_files(&out_file_paths)?;

        outcome.text_files = in_contents
            .into_iter()
            .map(|(name, r#in)| {
                let out = out_contents.remove(&name);
                (name, RetrieveTestCasesOutcomeProblemTextFiles { r#in, out })
            })
            .collect();
    }
    return Ok(());

    static URL: &str = "https://www.dropbox.com/sh/arnpe0ef5wds8cv/AAAk_SECQ2Nc6SVGii3rHX6Fa?dl=0";
    static DROPBOX_PATH_PREFIXES: Lazy<HashMap<String, String>> = Lazy::new(|| {
        serde_json::from_str(include_str!("../../assets/dropbox-path-prefixes.json")).unwrap()
    });

    struct Entries(Vec<Entry>);

    impl Entries {
        fn contains_folder(&self, name: &str) -> bool {
            self.0
                .iter()
                .any(|e| matches!(e, Entry::File(p) if p.split('/').last().unwrap() == name))
        }

        fn file_paths(&self) -> Vec<String> {
            self.0
                .iter()
                .flat_map(Entry::file_path)
                .map(ToOwned::to_owned)
                .collect()
        }

        fn folder_paths(&self) -> Vec<&str> {
            self.0.iter().flat_map(Entry::folder_path).collect()
        }

        fn folder_filenames(&self) -> Vec<&str> {
            self.0
                .iter()
                .flat_map(Entry::folder_path)
                .map(|p| {
                    p.split('/')
                        .last()
                        .expect("each path should starts with \"/\"")
                })
                .collect()
        }
    }

    enum Entry {
        File(String),
        Folder(String),
    }

    impl Entry {
        fn file_path(&self) -> Option<&str> {
            match self {
                Self::File(path) => Some(path),
                Self::Folder(_) => None,
            }
        }

        fn folder_path(&self) -> Option<&str> {
            match self {
                Self::File(_) => None,
                Self::Folder(path) => Some(path),
            }
        }
    }

    fn list_paths_with_filter(
        client: &reqwest::blocking::Client,
        mut shell: impl Shell,
        access_token: &str,
        path: &str,
    ) -> anyhow::Result<Entries> {
        let res = client
            .post_with_shell(
                static_url!("https://api.dropboxapi.com/2/files/list_folder").clone(),
                &mut shell,
            )
            .bearer_auth(access_token)
            .json(&json!({ "shared_link": { "url": URL }, "path": path }))
            .colorize_status_code(&[200], (), ..)
            .send()?
            .ensure_status_ok(|| format!("could not retrieve file names in `{}`", path))?;

        let mut output = vec![];
        let mut list_folder_result = res.json::<ListFolderResult>()?;

        while {
            output.extend(mem::take(&mut list_folder_result.entries));

            let res = client
                .post_with_shell(
                    static_url!("https://api.dropboxapi.com/2/files/list_folder/continue").clone(),
                    &mut shell,
                )
                .bearer_auth(access_token)
                .json(&json!({ "cursor": &list_folder_result.cursor }))
                .colorize_status_code(&[200], (), ..)
                .send()?
                .ensure_status_ok(|| {
                    format!(
                        "could not retrieve file names at cursor `{}`",
                        list_folder_result.cursor,
                    )
                })?;

            list_folder_result = res.json()?;
            list_folder_result.has_more
        } {}

        return output
            .into_iter()
            .filter(Metadata::is_valid)
            .map(|metadata| {
                let join = |name| format!("{}/{}", path.trim_end_matches('/'), name);
                match metadata {
                    Metadata::File { name } => Ok(Entry::File(join(&name))),
                    Metadata::Folder { name } => Ok(Entry::Folder(join(&name))),
                    Metadata::Deleted { name } => bail!("deleted: {:?}", name),
                }
            })
            .collect::<anyhow::Result<_>>()
            .map(Entries);

        #[derive(Deserialize)]
        struct ListFolderResult {
            entries: Vec<Metadata>,
            cursor: String,
            has_more: bool,
        }

        #[derive(Deserialize)]
        #[serde(tag = ".tag", rename_all = "snake_case")]
        enum Metadata {
            File { name: String },
            Folder { name: String },
            Deleted { name: String },
        }

        impl Metadata {
            fn is_valid(&self) -> bool {
                !(matches!(self, Self::Folder { name } if name == "etc")
                    || matches!(
                        self, Self::File { name }
                        if !(name.is_ascii()
                            && [None, Some("txt"), Some("in"), Some("out")]
                                .contains(&Utf8Path::new(name).extension()))
                    ))
            }
        }

        #[ext]
        impl reqwest::blocking::Response {
            fn ensure_status_ok(
                self,
                err_context: impl FnOnce() -> std::string::String,
            ) -> anyhow::Result<Self>
            where
                Self: Sized,
            {
                if self.status() != 200 {
                    let msg = { self }.text()?;
                    let msg = if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&msg) {
                        serde_json::to_string_pretty(&msg).unwrap()
                    } else {
                        msg
                    };
                    return Err(anyhow!("{}", msg).context(err_context()));
                }
                Ok(self)
            }
        }
    }

    fn retrieve_files(
        client: &reqwest::Client,
        progress_draw_target: ProgressDrawTarget,
        access_token: &str,
        file_paths: &[String],
    ) -> anyhow::Result<IndexMap<String, String>> {
        let contents = crate::download::download_with_progress(
            progress_draw_target,
            file_paths
                .iter()
                .map(|path| {
                    let req = client
                        .post("https://content.dropboxapi.com/2/sharing/get_shared_link_file")
                        .bearer_auth(access_token)
                        .header(
                            "Dropbox-API-Arg",
                            json!({ "url": URL, "path": path }).to_string(),
                        );
                    (path.clone(), req)
                })
                .collect(),
        )?;

        return Ok(file_paths.iter().map(file_stem).zip_eq(contents).collect());

        fn file_stem(path: impl AsRef<str>) -> String {
            path.as_ref()
                .split('/')
                .last()
                .unwrap()
                .split('.')
                .next()
                .unwrap()
                .to_owned()
        }
    }
}

fn retrieve_tasks_page(
    client: &reqwest::blocking::Client,
    mut shell: impl Shell,
    username_and_password: impl FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
    contest: &CaseConverted<LowerCase>,
) -> anyhow::Result<Html> {
    let url = url!("/contests/{}/tasks", contest);

    let res = client
        .get_with_shell(url.clone(), &mut shell)
        .colorize_status_code(&[200], &[404], ..)
        .send()?
        .ensure_status(&[200, 404])?;

    if res.status() == 200 {
        res.html().map_err(Into::into)
    } else {
        participate_if_not(client, &mut shell, username_and_password, contest, false)?;

        client
            .get_with_shell(url, shell)
            .colorize_status_code(&[200], (), ..)
            .send()?
            .ensure_status(&[200])?
            .html()
            .map_err(Into::into)
    }
}

#[derive(Debug)]
enum ContestStatus {
    Finished,
    Active,
    NotBegun(CaseConverted<LowerCase>, DateTime<Local>),
}

impl ContestStatus {
    fn now(dur: (DateTime<Utc>, DateTime<Utc>), contest_id: &CaseConverted<LowerCase>) -> Self {
        let (start, end) = dur;
        let now = Utc::now();
        if now < start {
            ContestStatus::NotBegun(contest_id.to_owned(), start.with_timezone(&Local))
        } else if now > end {
            ContestStatus::Finished
        } else {
            ContestStatus::Active
        }
    }

    fn is_finished(&self) -> bool {
        matches!(self, ContestStatus::Finished)
    }

    fn raise_if_not_begun(&self) -> anyhow::Result<()> {
        if let ContestStatus::NotBegun(contest, time) = self {
            bail!("`{}` will begin at {}", contest, time)
        }
        Ok(())
    }
}

trait HtmlExt {
    fn extract_title(&self) -> anyhow::Result<&str>;
    fn extract_csrf_token(&self) -> anyhow::Result<String>;
    fn extract_contest_duration(&self) -> anyhow::Result<(DateTime<Utc>, DateTime<Utc>)>;
    fn contains_registration_button(&self) -> anyhow::Result<bool>;
    fn extract_task_indexes_and_urls(
        &self,
    ) -> anyhow::Result<IndexMap<CaseConverted<UpperCase>, Url>>;
    fn extract_samples(&self) -> Vec<anyhow::Result<(String, String, anyhow::Result<TestSuite>)>>;
}

impl HtmlExt for Html {
    fn extract_title(&self) -> anyhow::Result<&str> {
        self.select(static_selector!(":root > head > title"))
            .flat_map(|r| r.text())
            .exactly_one()
            .ok()
            .with_context(|| "Could not find `<title>`")
    }

    fn extract_csrf_token(&self) -> anyhow::Result<String> {
        (|| {
            let token = self
                .select(static_selector!("[name=\"csrf_token\"]"))
                .next()?
                .value()
                .attr("value")?
                .to_owned();
            (!token.is_empty()).then(|| token)
        })()
        .with_context(|| "could not find the CSRF token")
    }

    fn extract_contest_duration(&self) -> anyhow::Result<(DateTime<Utc>, DateTime<Utc>)> {
        (|| -> _ {
            let mut it = self.select(static_selector!("time"));
            let t1 = it.next()?.text().next()?;
            let t2 = it.next()?.text().next()?;
            let t1 = DateTime::parse_from_str(t1, FORMAT).ok()?;
            let t2 = DateTime::parse_from_str(t2, FORMAT).ok()?;
            return Some((t1.with_timezone(&Utc), t2.with_timezone(&Utc)));
            static FORMAT: &str = "%F %T%z";
        })()
        .with_context(|| "Could not find the contest duration")
    }

    fn contains_registration_button(&self) -> anyhow::Result<bool> {
        let insert_participant_box = self
            .select(static_selector!("#main-container .insert-participant-box"))
            .next()
            .with_context(|| "Could not find the registration button")?;

        Ok(insert_participant_box
            .select(static_selector!("form"))
            .find(|r| r.value().attr("method") == Some("POST"))
            .into_iter()
            .flat_map(|r| r.text())
            .any(|s| ["参加登録", "Register"].contains(&s)))
    }

    fn extract_task_indexes_and_urls(
        &self,
    ) -> anyhow::Result<IndexMap<CaseConverted<UpperCase>, Url>> {
        self.select(static_selector!(
            "#main-container > div.row > div.col-sm-12 > div.panel > table.table > tbody > tr",
        ))
        .map(|tr| {
            let a = tr.select(static_selector!("td.text-center > a")).next()?;
            let index = CaseConverted::new(a.text().next()?);
            let url = BASE_URL.join(a.value().attr("href")?).ok()?;
            Some((index, url))
        })
        .collect::<Option<IndexMap<_, _>>>()
        .filter(|m| !m.is_empty())
        .with_context(|| "Could not extract task indexes and URLs")
    }

    fn extract_samples(&self) -> Vec<anyhow::Result<(String, String, anyhow::Result<TestSuite>)>> {
        return self
            .select(static_selector!(
                "#main-container > div.row div[class=\"col-sm-12\"]",
            ))
            .map(|div| {
                let (index, display_name) = {
                    let title_with_index = div
                        .select(static_selector!(":scope > span"))
                        .flat_map(|r| r.text())
                        .next()
                        .with_context(|| "Could not find the title")?;

                    let caps = static_regex!(r"([A-Z0-9]+) - (.+)")
                        .captures(title_with_index)
                        .with_context(|| {
                            format!("Could not parse the title: {:?}", title_with_index)
                        })?;

                    (caps[1].to_owned(), caps[2].to_owned())
                };

                let test_suite = (|| {
                    let timelimit = div
                        .select(static_selector!(":scope > p"))
                        .flat_map(|r| r.text())
                        .flat_map(parse_timelimit)
                        .exactly_one()
                        .map_err(|_| "Could not extract the timelimit")?;

                    // In `tasks_print`, there are multiple `#task-statement`s.
                    let samples = div
                        .select(static_selector!(":scope > div[id=\"task-statement\"]"))
                        .exactly_one()
                        .ok()
                        .and_then(extract_samples)
                        .ok_or("Could not extract the sample cases")?;

                    Ok::<_, &str>(if timelimit == Duration::new(0, 0) {
                        TestSuite::Unsubmittable
                    } else if let Samples::Batch(r#match, samples) = samples {
                        TestSuite::Batch(BatchTestSuite {
                            timelimit: Some(timelimit),
                            r#match,
                            cases: samples
                                .into_iter()
                                .enumerate()
                                .map(|(i, (input, output))| PartialBatchTestCase {
                                    name: Some(format!("sample{}", i + 1)),
                                    r#in: input.into(),
                                    out: Some(output.into()),
                                    timelimit: None,
                                    r#match: None,
                                })
                                .collect(),
                            extend: vec![],
                        })
                    } else {
                        TestSuite::Interactive(InteractiveTestSuite {
                            timelimit: Some(timelimit),
                        })
                    })
                })()
                .map_err(|e| anyhow!("{}: {}", index, e));

                Ok((index, display_name, test_suite))
            })
            .collect();

        fn parse_timelimit(text: &str) -> Option<Duration> {
            let caps =
                static_regex!(r"\A\D*([0-9]{1,9})(\.[0-9]{1,3})?\s*(m)?sec.*\z").captures(text)?;
            let (mut b, mut e) = (caps[1].parse::<u64>().unwrap(), 0);
            if let Some(cap) = caps.get(2) {
                let n = cap.as_str().len() as u32 - 1;
                b *= 10u64.pow(n);
                b += cap.as_str()[1..].parse::<u64>().ok()?;
                e -= n as i32;
            }
            if caps.get(3).is_none() {
                e += 3;
            }
            let timelimit = if e < 0 {
                b / 10u64.pow(-e as u32)
            } else {
                b * 10u64.pow(e as u32)
            };
            Some(Duration::from_millis(timelimit))
        }

        fn extract_samples(task_statement: ElementRef<'_>) -> Option<Samples> {
            // TODO:
            // - https://atcoder.jp/contests/arc019/tasks/arc019_4 (interactive)
            // - https://atcoder.jp/contests/arc021/tasks/arc021_4 (interactive)
            // - https://atcoder.jp/contests/cf17-final-open/tasks/cf17_final_f
            // - https://atcoder.jp/contests/jag2016-domestic/tasks
            // - https://atcoder.jp/contests/chokudai001/tasks/chokudai_001_a

            let try_extract_samples = |h, c, i, o| try_extract_samples(task_statement, h, c, i, o);
            return try_extract_samples(&P1_HEAD, &P1_CONTENT, &IN_JA, &OUT_JA)
                .or_else(|| try_extract_samples(&P2_HEAD, &P2_CONTENT, &IN_EN, &OUT_EN))
                .or_else(|| try_extract_samples(&P3_HEAD, &P3_CONTENT, &IN_JA, &OUT_JA))
                .or_else(|| try_extract_samples(&P4_HEAD, &P4_CONTENT, &IN_JA, &OUT_JA))
                .or_else(|| try_extract_samples(&P5_HEAD, &P5_CONTENT, &IN_JA, &OUT_JA))
                .or_else(|| try_extract_samples(&P6_HEAD, &P6_CONTENT, &IN_JA, &OUT_JA))
                .or_else(|| try_extract_samples(&P7_HEAD, &P7_CONTENT, &IN_JA, &OUT_JA))
                .or_else(|| try_extract_samples(&P8_HEAD, &P8_CONTENT, &IN_JA, &OUT_JA));

            // Current style (Japanese)
            static P1_HEAD: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-ja > div.part > section > h3");
            static P1_CONTENT: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-ja > div.part > section > pre");
            // Current style (English)
            static P2_HEAD: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-en > div.part > section > h3");
            static P2_CONTENT: Lazy<Selector> =
                lazy_selector!("span.lang>span.lang-en>div.part>section>pre");
            // ARC019..ARC057 \ {ARC019/C, ARC046/D, ARC050, ARC052/{A, C}, ARC053, ARC055},
            // ABC007..ABC040 \ {ABC036}, ATC001, ATC002
            static P3_HEAD: Lazy<Selector> = lazy_selector!("div.part > section > h3");
            static P3_CONTENT: Lazy<Selector> = lazy_selector!("div.part > section > pre");
            // ARC002..ARC018, ARC019/C, ABC001..ABC006
            static P4_HEAD: Lazy<Selector> = lazy_selector!("div.part > h3,pre");
            static P4_CONTENT: Lazy<Selector> = lazy_selector!("div.part > section > pre");
            // ARC001, dwacon2018-final/{A, B}
            static P5_HEAD: Lazy<Selector> = lazy_selector!("h3,pre");
            static P5_CONTENT: Lazy<Selector> = lazy_selector!("section > pre");
            // ARC046/D, ARC050, ARC052/{A, C}, ARC053, ARC055, ABC036, ABC041
            static P6_HEAD: Lazy<Selector> = lazy_selector!("section > h3");
            static P6_CONTENT: Lazy<Selector> = lazy_selector!("section > pre");
            // ABC034
            static P7_HEAD: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-ja > section > h3");
            static P7_CONTENT: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-ja > section > pre");
            // practice contest (Japanese)
            static P8_HEAD: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-ja > div.part > h3");
            static P8_CONTENT: Lazy<Selector> =
                lazy_selector!("span.lang > span.lang-ja > div.part > section > pre");

            static IN_JA: Lazy<Regex> = lazy_regex!(r"\A[\s\n]*入力例\s*(\d{1,2})[.\n]*\z");
            static OUT_JA: Lazy<Regex> = lazy_regex!(r"\A[\s\n]*出力例\s*(\d{1,2})[.\n]*\z");
            static IN_EN: Lazy<Regex> = lazy_regex!(r"\ASample Input\s?([0-9]{1,2}).*\z");
            static OUT_EN: Lazy<Regex> = lazy_regex!(r"\ASample Output\s?([0-9]{1,2}).*\z");
        }

        fn try_extract_samples(
            task_statement: ElementRef<'_>,
            selector_for_header: &'static Selector,
            selector_for_content: &'static Selector,
            re_input: &'static Regex,
            re_output: &'static Regex,
        ) -> Option<Samples> {
            #[allow(clippy::blocks_in_if_conditions)]
            if task_statement
                .select(static_selector!("strong"))
                .flat_map(|r| r.text())
                .any(|s| {
                    s.contains("インタラクティブ")
                        || s.contains("対話式の問題")
                        || s.contains("Interactive")
                })
            {
                return Some(Samples::Interactive);
            }

            let matching = {
                let error = task_statement
                    .select(static_selector!("var"))
                    .flat_map(|r| r.text())
                    .flat_map(|t| parse_floating_error(t))
                    .next();

                let relative = task_statement
                    .text()
                    .any(|s| s.contains("相対誤差") || s.contains("relative error"));

                let absolute = task_statement
                    .text()
                    .any(|s| s.contains("絶対誤差") || s.contains("absolute error"));

                match (error, relative, absolute) {
                    (Some(error), true, true) => Match::Float {
                        relative_error: Some(error),
                        absolute_error: Some(error),
                    },
                    (Some(error), true, false) => Match::Float {
                        relative_error: Some(error),
                        absolute_error: None,
                    },
                    (Some(error), false, true) => Match::Float {
                        relative_error: None,
                        absolute_error: Some(error),
                    },
                    _ => Match::Lines,
                }
            };

            let mut inputs = BTreeMap::<usize, _>::new();
            let mut outputs = BTreeMap::<usize, _>::new();
            let mut next = None;
            let selector = or(selector_for_header, selector_for_content);
            for elem_ref in task_statement.select(&selector) {
                if elem_ref.value().name() == "h3" {
                    let text = elem_ref.collect_text();
                    if let Some(caps) = re_input.captures(&text) {
                        next = Some((true, parse_zenkaku(&caps[1]).ok()?));
                    } else if let Some(caps) = re_output.captures(&text) {
                        next = Some((false, parse_zenkaku(&caps[1]).ok()?));
                    }
                } else if ["pre", "section"].contains(&elem_ref.value().name()) {
                    if let Some((is_input, n)) = next {
                        let text = elem_ref.collect_text();
                        if is_input {
                            inputs.insert(n, text);
                        } else {
                            outputs.insert(n, text);
                        }
                    }
                    next = None;
                }
            }
            let mut samples = vec![];
            for (i, input) in inputs {
                if let Some(output) = outputs.remove(&i) {
                    samples.push((input, output));
                }
            }

            for (input, output) in &mut samples {
                for s in &mut [input, output] {
                    if !(s.is_empty() || s.ends_with('\n')) {
                        s.push('\n');
                    }

                    if !is_valid_text(s) {
                        return None;
                    }
                }
            }

            if samples.is_empty() {
                return None;
            }

            Some(Samples::Batch(matching, samples))
        }

        fn or(selector1: &Selector, selector2: &Selector) -> Selector {
            let mut ret = selector1.clone();
            ret.selectors.extend(selector2.selectors.clone());
            ret
        }

        fn parse_floating_error(s: &str) -> Option<PositiveFinite<f64>> {
            let caps = static_regex!(r"\A10\^\{(-?[0-9]{1,2})\}\z").captures(s)?;
            format!("1e{}", &caps[1]).parse().ok()
        }

        fn parse_zenkaku<T: FromStr>(s: &str) -> Result<T, T::Err> {
            match s.parse() {
                Ok(v) => Ok(v),
                Err(e) => {
                    if s.chars().all(|c| ('０'..='９').contains(&c)) {
                        s.chars()
                            .map(|c| {
                                char::from((u32::from(c) - u32::from('０') + u32::from('0')) as u8)
                            })
                            .collect::<String>()
                            .parse()
                    } else {
                        Err(e)
                    }
                }
            }
        }

        fn is_valid_text(s: &str) -> bool {
            s == "\n"
                || ![' ', '\n'].iter().any(|&c| s.starts_with(c))
                    && s.chars().all(|c| {
                        c.is_ascii() && (c.is_ascii_whitespace() == [' ', '\n'].contains(&c))
                    })
        }

        #[ext]
        impl ElementRef<'_> {
            fn collect_text(&self) -> String {
                self.text().fold("".to_owned(), |mut r, s| {
                    r.push_str(s);
                    r
                })
            }
        }

        enum Samples {
            Batch(Match, Vec<(String, String)>),
            Interactive,
        }
    }
}

#[derive(derivative::Derivative, derive_more::Deref, derive_more::Display)]
#[derivative(Default, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[deref(forward)]
#[display(fmt = "{}", inner)]
struct CaseConverted<C> {
    inner: String,
    #[deref(ignore)]
    _marker: PhantomData<fn() -> C>,
}

impl<C: CaseConvertion> CaseConverted<C> {
    fn new(s: impl AsRef<str>) -> Self {
        Self {
            inner: C::CONVERT(s.as_ref()),
            _marker: PhantomData,
        }
    }
}

impl<C> Borrow<str> for CaseConverted<C> {
    fn borrow(&self) -> &str {
        &self.inner
    }
}

impl<C> From<CaseConverted<C>> for String {
    fn from(from: CaseConverted<C>) -> String {
        from.inner
    }
}

impl<C> fmt::Debug for CaseConverted<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

trait CaseConvertion: fmt::Debug {
    const CONVERT: fn(&str) -> String;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
enum LowerCase {}

impl CaseConvertion for LowerCase {
    const CONVERT: fn(&str) -> String = str::to_lowercase;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
enum UpperCase {}

impl CaseConvertion for UpperCase {
    const CONVERT: fn(&str) -> String = str::to_uppercase;
}
