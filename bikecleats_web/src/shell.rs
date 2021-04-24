use indicatif::ProgressDrawTarget;
use std::{
    cell::RefCell,
    fmt,
    io::{self, Write as _},
};
use strum::{EnumString, EnumVariantNames};
use termcolor::{BufferedStandardStream, Color, ColorSpec, WriteColor};

macro_rules! color_spec {
    ($($tt:tt)*) => {
        _color_spec_inner!(@acc(ColorSpec::new().set_reset(false)), @rest($($tt)*))
    };
}

macro_rules! _color_spec_inner {
    (@acc($acc:expr), @rest()) => {
        $acc
    };
    (@acc($acc:expr), @rest(, $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc), @rest($($rest)*))
    };
    (@acc($acc:expr), @rest(Fg($color:expr) $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc.set_fg(Some($color))), @rest($($rest)*))
    };
    (@acc($acc:expr), @rest(Bg($color:expr) $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc.set_bg(Some($color))), @rest($($rest)*))
    };
    (@acc($acc:expr), @rest(Bold $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc.set_bold(true)), @rest($($rest)*))
    };
    (@acc($acc:expr), @rest(Italic $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc.set_italic(true)), @rest($($rest)*))
    };
    (@acc($acc:expr), @rest(Underline $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc.set_underline(true)), @rest($($rest)*))
    };
    (@acc($acc:expr), @rest(Intense $($rest:tt)*)) => {
        _color_spec_inner!(@acc($acc.set_intense(true)), @rest($($rest)*))
    };
}

pub trait Shell {
    fn progress_draw_target(&self) -> ProgressDrawTarget;
    fn print_ansi(&mut self, message: &[u8]) -> io::Result<()>;
    fn warn<T: fmt::Display>(&mut self, message: T) -> io::Result<()>;
    fn on_request(&mut self, request: &reqwest::blocking::Request) -> io::Result<()>;
    fn on_response(
        &mut self,
        response: &reqwest::blocking::Response,
        status_code_color: StatusCodeColor,
    ) -> io::Result<()>;
}

impl<S: Shell> Shell for &'_ mut S {
    fn progress_draw_target(&self) -> ProgressDrawTarget {
        (**self).progress_draw_target()
    }

    fn print_ansi(&mut self, message: &[u8]) -> io::Result<()> {
        (**self).print_ansi(message)
    }

    fn warn<T: fmt::Display>(&mut self, message: T) -> io::Result<()> {
        (**self).warn(message)
    }

    fn on_request(&mut self, request: &reqwest::blocking::Request) -> io::Result<()> {
        (**self).on_request(request)
    }

    fn on_response(
        &mut self,
        response: &reqwest::blocking::Response,
        status_code_color: StatusCodeColor,
    ) -> io::Result<()> {
        (**self).on_response(response, status_code_color)
    }
}

pub trait ShellExt: Shell {
    fn as_cell(&mut self) -> CellShell<&mut Self> {
        CellShell(RefCell::new(self))
    }
}

impl<S: Shell> ShellExt for S {}

pub struct CellShell<S>(RefCell<S>);

impl<S: Shell> From<S> for CellShell<S> {
    fn from(shell: S) -> Self {
        Self(RefCell::new(shell))
    }
}

macro_rules! impl_shell_for_cell_shell((for<S: _> $({$($tt:tt)+}),*) => {
    $(
        impl<S: Shell> Shell for $($tt)* {
            fn progress_draw_target(&self) -> ProgressDrawTarget {
                self.0.borrow().progress_draw_target()
            }

            fn print_ansi(&mut self, message: &[u8]) -> io::Result<()> {
                self.0.borrow_mut().print_ansi(message)
            }

            fn warn<T: fmt::Display>(&mut self, message: T) -> io::Result<()> {
                self.0.borrow_mut().warn(message)
            }

            fn on_request(&mut self, request: &reqwest::blocking::Request) -> io::Result<()> {
                self.0.borrow_mut().on_request(request)
            }

            fn on_response(
                &mut self,
                response: &reqwest::blocking::Response,
                status_code_color: StatusCodeColor,
            ) -> io::Result<()> {
                self.0.borrow_mut().on_response(response, status_code_color)
            }
        }
    )*
});

impl_shell_for_cell_shell!(for <S: _> {CellShell<S>}, {&'_ CellShell<S>});

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub enum StatusCodeColor {
    Pass,
    Warning,
    Error,
    Unknown,
}

pub struct StandardShell {
    stderr: BufferedStandardStream,
}

impl StandardShell {
    pub fn new(color: self::ColorChoice) -> Self {
        Self {
            stderr: BufferedStandardStream::stderr(
                color.to_termcolor_color_choice(atty::Stream::Stderr),
            ),
        }
    }
}

impl Shell for StandardShell {
    fn progress_draw_target(&self) -> ProgressDrawTarget {
        ProgressDrawTarget::stderr()
    }

    fn print_ansi(&mut self, message: &[u8]) -> io::Result<()> {
        fwdansi::write_ansi(&mut self.stderr, message)
    }

    fn warn<T: fmt::Display>(&mut self, message: T) -> io::Result<()> {
        self.stderr
            .set_color(color_spec!(Bold, Fg(Color::Yellow)))?;
        write!(self.stderr, "warning:")?;
        self.stderr.reset()?;

        writeln!(self.stderr, " {}", message)?;

        self.stderr.flush()
    }

    fn on_request(&mut self, request: &reqwest::blocking::Request) -> io::Result<()> {
        self.stderr.set_color(color_spec!(Bold))?;
        write!(self.stderr, "{}", request.method())?;
        self.stderr.reset()?;

        write!(self.stderr, " ")?;

        self.stderr.set_color(color_spec!(Fg(Color::Cyan)))?;
        write!(self.stderr, "{}", request.url())?;
        self.stderr.reset()?;

        write!(self.stderr, " ... ")?;

        self.stderr.flush()
    }

    fn on_response(
        &mut self,
        response: &reqwest::blocking::Response,
        status_code_color: StatusCodeColor,
    ) -> io::Result<()> {
        let fg = match status_code_color {
            StatusCodeColor::Pass => Some(Color::Green),
            StatusCodeColor::Warning => Some(Color::Yellow),
            StatusCodeColor::Error => Some(Color::Red),
            StatusCodeColor::Unknown => None,
        };
        self.stderr.set_color(color_spec!(Bold).set_fg(fg))?;
        write!(self.stderr, "{}", response.status())?;
        self.stderr.reset()?;

        writeln!(self.stderr)?;

        self.stderr.flush()
    }
}

#[derive(EnumString, EnumVariantNames, strum::Display, Clone, Copy, Debug)]
#[strum(serialize_all = "kebab-case")]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn to_termcolor_color_choice(self, stream: atty::Stream) -> termcolor::ColorChoice {
        match (self, atty::is(stream)) {
            (Self::Auto, true) => termcolor::ColorChoice::Auto,
            (Self::Always, _) => termcolor::ColorChoice::Always,
            _ => termcolor::ColorChoice::Never,
        }
    }
}
