use crate::{
    request::{BlockingClientExt as _, BlockingResponseExt},
    shell::Shell,
};
use anyhow::Context as _;
use maplit::hashmap;
use once_cell::sync::Lazy;
use scraper::Html;
use url::Url;

static BASE_URL: Lazy<Url> = lazy_url!("https://atcoder.jp");

pub(crate) fn login(
    client: &reqwest::blocking::Client,
    mut shell: impl Shell,
    username_and_password: impl FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
) -> anyhow::Result<()> {
    if !check_logged_in(client, &mut shell)? {
        ensure_login(client, &mut shell, username_and_password)?;
    }
    Ok(())
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

trait HtmlExt {
    fn extract_csrf_token(&self) -> anyhow::Result<String>;
}

impl HtmlExt for Html {
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
}
