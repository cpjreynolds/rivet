#[macro_use] extern crate bitflags;
extern crate libc;
extern crate time;
extern crate num;
extern crate nix;

mod sys;
pub use sys::{
    Selector,
    Fired,
};
mod event;

use std::os::unix::io::AsRawFd;
use std::io::{
    Result,
    Error,
};

pub trait NonBlocking {
    fn set_nonblock(&mut self) -> Result<()>;
    fn set_block(&mut self) -> Result<()>;

    fn is_block(&self) -> bool;
    fn is_nonblock(&self) -> bool;
}

impl<T> NonBlocking for T
    where T: AsRawFd
{
    fn set_nonblock(&mut self) -> Result<()> {
        let fd = self.as_raw_fd();
        let res = unsafe {
            let mut flags = libc::fcntl(fd, libc::F_GETFL);
            flags |= libc::O_NONBLOCK;
            libc::fcntl(fd, libc::F_SETFL, flags)
        };

        if res == -1 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn set_block(&mut self) -> Result<()> {
        let fd = self.as_raw_fd();
        let res = unsafe {
            let mut flags = libc::fcntl(fd, libc::F_GETFL);
            flags &= !libc::O_NONBLOCK;
            libc::fcntl(fd, libc::F_SETFL, flags)
        };

        if res == -1 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn is_nonblock(&self) -> bool {
        let fd = self.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };

        (flags & libc::O_NONBLOCK) == libc::O_NONBLOCK
    }

    fn is_block(&self) -> bool {
        !self.is_nonblock()
    }
}

