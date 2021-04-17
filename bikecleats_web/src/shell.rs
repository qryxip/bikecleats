use indicatif::ProgressDrawTarget;
use std::{cell::RefCell, fmt, io};

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
    fn cell(&mut self) -> CellShell<&mut Self> {
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
