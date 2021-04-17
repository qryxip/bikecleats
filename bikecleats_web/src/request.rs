use crate::shell::{Shell, StatusCodeColor};
use anyhow::ensure;
use http::{Method, StatusCode};
use reqwest::IntoUrl;
use serde::Serialize;
use std::{
    fmt,
    ops::{RangeFull, RangeInclusive},
};

pub(crate) trait BlockingClientExt {
    fn get_with_shell<U: IntoUrl, S: Shell>(&self, url: U, shell: S) -> RequestBuilderWithShell<S>;
    fn post_with_shell<U: IntoUrl, S: Shell>(&self, url: U, shell: S)
        -> RequestBuilderWithShell<S>;
}

impl BlockingClientExt for reqwest::blocking::Client {
    fn get_with_shell<U: IntoUrl, S: Shell>(&self, url: U, shell: S) -> RequestBuilderWithShell<S> {
        RequestBuilderWithShell::new(self, Method::GET, url, shell)
    }

    fn post_with_shell<U: IntoUrl, S: Shell>(
        &self,
        url: U,
        shell: S,
    ) -> RequestBuilderWithShell<S> {
        RequestBuilderWithShell::new(self, Method::POST, url, shell)
    }
}

pub(crate) struct RequestBuilderWithShell<S> {
    req: reqwest::blocking::RequestBuilder,
    client: reqwest::blocking::Client,
    shell: S,
    colorize_status_code: Box<dyn FnOnce(StatusCode) -> StatusCodeColor>,
}

impl<S: Shell> RequestBuilderWithShell<S> {
    fn new(
        client: &reqwest::blocking::Client,
        method: Method,
        url: impl IntoUrl,
        shell: S,
    ) -> Self {
        Self {
            req: client.request(method, url),
            client: client.clone(),
            shell,
            colorize_status_code: Box::new(|_| StatusCodeColor::Unknown),
        }
    }
}

impl<S: Shell> RequestBuilderWithShell<S> {
    pub(crate) fn bearer_auth(self, token: impl fmt::Display) -> Self {
        Self {
            req: self.req.bearer_auth(token),
            ..self
        }
    }

    pub(crate) fn form(self, form: &(impl Serialize + ?Sized)) -> Self {
        Self {
            req: self.req.form(form),
            ..self
        }
    }

    pub(crate) fn json(self, json: &(impl Serialize + ?Sized)) -> Self {
        Self {
            req: self.req.json(json),
            ..self
        }
    }

    pub(crate) fn colorize_status_code(
        self,
        pass: impl StatusCodeRange,
        warning: impl StatusCodeRange,
        error: impl StatusCodeRange,
    ) -> Self {
        Self {
            colorize_status_code: Box::new(move |status| {
                if pass.contains(status) {
                    StatusCodeColor::Pass
                } else if warning.contains(status) {
                    StatusCodeColor::Warning
                } else if error.contains(status) {
                    StatusCodeColor::Error
                } else {
                    StatusCodeColor::Unknown
                }
            }),
            ..self
        }
    }

    pub(crate) fn send(mut self) -> anyhow::Result<reqwest::blocking::Response> {
        let req = self.req.build()?;
        self.shell.on_request(&req)?;
        let res = self.client.execute(req)?;
        self.shell
            .on_response(&res, (self.colorize_status_code)(res.status()))?;
        Ok(res)
    }
}

pub(crate) trait StatusCodeRange: 'static {
    fn contains(&self, status: StatusCode) -> bool;
}

impl StatusCodeRange for () {
    fn contains(&self, _: StatusCode) -> bool {
        false
    }
}

impl StatusCodeRange for RangeInclusive<u16> {
    fn contains(&self, status: StatusCode) -> bool {
        self.contains(&status.as_u16())
    }
}

impl StatusCodeRange for RangeFull {
    fn contains(&self, _: StatusCode) -> bool {
        true
    }
}

impl StatusCodeRange for &'static [u16; 1] {
    fn contains(&self, status: StatusCode) -> bool {
        self[..].contains(&status.as_u16())
    }
}

impl StatusCodeRange for &'static [u16; 2] {
    fn contains(&self, status: StatusCode) -> bool {
        self[..].contains(&status.as_u16())
    }
}

pub(crate) trait BlockingResponseExt: Sized {
    fn ensure_status(self, statuses: &'static [u16]) -> anyhow::Result<Self>;
    fn html(self) -> reqwest::Result<scraper::Html>;
}

impl BlockingResponseExt for reqwest::blocking::Response {
    fn ensure_status(self, statuses: &'static [u16]) -> anyhow::Result<Self> {
        ensure!(
            statuses.contains(&self.status().as_u16()),
            "expected {:?}, got {}",
            statuses,
            self.status(),
        );
        Ok(self)
    }

    fn html(self) -> reqwest::Result<scraper::Html> {
        let text = self.text()?;
        Ok(scraper::Html::parse_document(&text))
    }
}
