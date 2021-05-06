use anyhow::{bail, Context as _};
use bikecleats_web::{Session, Shell, StatusCodeColor};
use http::{Method, StatusCode};
use indicatif::ProgressDrawTarget;
use itertools::Itertools as _;
use reqwest::{cookie::Jar, redirect::Policy};
use serde::Serialize;
use std::{
    cell::RefCell,
    env, fmt,
    io::{self, Read as _},
    mem,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use url::Url;

pub fn run<
    C: OptionalCredentials,
    O: Serialize,
    F: FnOnce(&Session<Messages>, &mut C) -> anyhow::Result<O>,
>(
    perform: F,
) -> anyhow::Result<(String, O)> {
    let messages = Rc::new(RefCell::default());
    let session = Session::builder()
        .user_agent(USER_AGENT)
        .cookie_provider(Arc::new(Jar::default()))
        .redirect(Policy::none)
        .timeout(TIMEOUT)
        .build(Messages(messages.clone()))?;
    let outcome = perform(&{ session }, &mut C::new(&messages)?)?;
    let messages = Rc::try_unwrap(messages)
        .unwrap()
        .into_inner()
        .iter()
        .map(|m| m.to_string() + "\n")
        .join("");
    return Ok((messages, outcome));

    const USER_AGENT: &str = "https://github.com/qryxip/bikecleats";
    const TIMEOUT: Duration = Duration::from_secs(10);
}

pub struct Messages(Rc<RefCell<Vec<Message>>>);

impl Messages {
    fn push(&self, message: Message) {
        self.0.borrow_mut().push(message);
    }
}

impl Shell for Messages {
    fn progress_draw_target(&self) -> ProgressDrawTarget {
        ProgressDrawTarget::hidden()
    }

    fn print_ansi(&mut self, message: &[u8]) -> io::Result<()> {
        self.push(Message::PrintAnsi(from_utf8(message)?));
        return Ok(());

        fn from_utf8(bytes: &[u8]) -> io::Result<String> {
            let mut buf = "".to_owned();
            { bytes }.read_to_string(&mut buf)?;
            Ok(buf)
        }
    }

    fn warn<T: fmt::Display>(&mut self, message: T) -> io::Result<()> {
        self.push(Message::Warn(message.to_string()));
        Ok(())
    }

    fn on_request(&mut self, request: &reqwest::blocking::Request) -> io::Result<()> {
        self.push(Message::OnRequest(
            request.method().clone(),
            request.url().clone(),
        ));
        Ok(())
    }

    fn on_response(
        &mut self,
        response: &reqwest::blocking::Response,
        status_code_color: StatusCodeColor,
    ) -> io::Result<()> {
        self.push(Message::OnResponse(response.status(), status_code_color));
        Ok(())
    }
}

#[derive(Debug)]
pub enum Message {
    PrintAnsi(String),
    Warn(String),
    OnRequest(Method, Url),
    OnResponse(StatusCode, StatusCodeColor),
    CredentialsPrompt(String),
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrintAnsi(s) => write!(f, "PrintAnsi({:?})", s),
            Self::Warn(s) => write!(f, "Warn({:?})", s),
            Self::OnRequest(m, u) => write!(f, "OnRequest({:?}, {:?})", m, u.as_str()),
            Self::OnResponse(c, k) => write!(f, "OnResponse({:?}, {:?})", c, k),
            Self::CredentialsPrompt(s) => write!(f, "CredentialsPrompt({:?})", s),
        }
    }
}

pub struct Credentials {
    atcoder_username: String,
    atcoder_password: String,
    asked_atcoder_credentials: bool,
    messages: Rc<RefCell<Vec<Message>>>,
}

impl Credentials {
    pub fn atcoder(&mut self) -> impl '_ + FnMut(&str, &str) -> anyhow::Result<(String, String)> {
        move |username_prompt, password_prompt| {
            self.prompt(username_prompt);
            self.prompt(password_prompt);
            if mem::replace(&mut self.asked_atcoder_credentials, true) {
                bail!("asked AtCoder credentials twice. probably wrong credentials");
            }
            Ok((self.atcoder_username.clone(), self.atcoder_password.clone()))
        }
    }

    fn prompt(&mut self, prompt: &str) {
        self.messages
            .borrow_mut()
            .push(Message::CredentialsPrompt(prompt.to_owned()));
    }
}

pub trait OptionalCredentials: Sized {
    fn new(messages: &Rc<RefCell<Vec<Message>>>) -> anyhow::Result<Self>;
}

impl OptionalCredentials for () {
    fn new(_: &Rc<RefCell<Vec<Message>>>) -> anyhow::Result<Self> {
        Ok(())
    }
}

impl OptionalCredentials for Credentials {
    fn new(messages: &Rc<RefCell<Vec<Message>>>) -> anyhow::Result<Self> {
        return Ok(Self {
            atcoder_username: env_var("ATCODER_USERNAME")?,
            atcoder_password: env_var("ATCODER_PASSWORD")?,
            asked_atcoder_credentials: false,
            messages: messages.clone(),
        });

        fn env_var(key: &str) -> anyhow::Result<String> {
            env::var(key).with_context(|| format!("could not read `${}`", key))
        }
    }
}
