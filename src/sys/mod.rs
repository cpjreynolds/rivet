//#[cfg(target_os = "linux")]
pub mod epoll;

/*
#[cfg(any(target_os = "freebsd",
          target_os = "openbsd",
          target_os = "netbsd",
          target_os = "dragonfly",
          target_os = "bitrig"))]
*/
pub mod poll;

//#[cfg(target_os = "macos")]
pub mod select;

use std::io::prelude::*;
use std::io::{
    Result,
    Error,
    ErrorKind,
};
use std::ops::{
    Deref,
    DerefMut,
    BitAnd,
    BitOr,
    BitXor,
    Sub,
    Not,
};

