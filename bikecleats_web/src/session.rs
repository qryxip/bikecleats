use crate::{
    platforms::atcoder,
    shell::{CellShell, Shell},
};
use http::HeaderValue;
use reqwest::redirect::Policy;
use std::{convert::TryInto, marker::PhantomData, sync::Arc, time::Duration};

pub struct Session<S> {
    async_client: reqwest::Client,
    blocking_client: reqwest::blocking::Client,
    shell: CellShell<S>,
}

impl<S: Shell> Session<S> {
    pub fn builder() -> SessionBuilder<S> {
        SessionBuilder::default()
    }

    pub fn atcoder_login<
        F: FnMut(&'static str, &'static str) -> anyhow::Result<(String, String)>,
    >(
        &self,
        username_and_password: F,
    ) -> anyhow::Result<()> {
        atcoder::login(&self.blocking_client, &self.shell, username_and_password)
    }
}

pub struct SessionBuilder<S> {
    async_builder: reqwest::ClientBuilder,
    blocking_builder: reqwest::blocking::ClientBuilder,
    _marker: PhantomData<S>,
}

impl<S: Shell> SessionBuilder<S> {
    pub fn build(self, shell: S) -> reqwest::Result<Session<S>> {
        let async_client = self.async_builder.build()?;
        let blocking_client = self.blocking_builder.build()?;
        Ok(Session {
            async_client,
            blocking_client,
            shell: shell.into(),
        })
    }

    pub fn user_agent<V>(self, value: V) -> Self
    where
        V: Clone + TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        Self {
            async_builder: self.async_builder.user_agent(value.clone()),
            blocking_builder: self.blocking_builder.user_agent(value),
            _marker: PhantomData,
        }
    }

    pub fn cookie_provider<C: reqwest::cookie::CookieStore + 'static>(
        self,
        cookie_store: Arc<C>,
    ) -> Self {
        Self {
            async_builder: self.async_builder.cookie_provider(cookie_store.clone()),
            blocking_builder: self.blocking_builder.cookie_provider(cookie_store),
            _marker: PhantomData,
        }
    }

    pub fn redirect<F: FnMut() -> Policy>(self, mut policy: F) -> Self {
        Self {
            async_builder: self.async_builder.redirect(policy()),
            blocking_builder: self.blocking_builder.redirect(policy()),
            _marker: PhantomData,
        }
    }

    pub fn timeout(self, timeout: Duration) -> Self {
        Self {
            async_builder: self.async_builder.timeout(timeout),
            blocking_builder: self.blocking_builder.timeout(timeout),
            _marker: PhantomData,
        }
    }
}

impl<S> Default for SessionBuilder<S> {
    fn default() -> Self {
        Self {
            async_builder: reqwest::Client::builder(),
            blocking_builder: reqwest::blocking::Client::builder(),
            _marker: PhantomData,
        }
    }
}
