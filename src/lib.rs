#[macro_use] extern crate bitflags;
extern crate libc;
extern crate time;
extern crate num;
extern crate nix;

mod sys;
pub use sys::{
    epoll,
    poll,
};

use std::os::unix::io::AsRawFd;
use std::io::Result;

pub trait NonBlocking {
    fn set_nonblock(&mut self) -> Result<()>;
    fn set_block(&mut self) -> Result<()>;
}

impl<T> NonBlocking for T
    where T: AsRawFd
{
    fn set_nonblock(&mut self) -> Result<()> {
        let fd = self.as_raw_fd();
        unsafe {
            let mut flags = libc::fcntl(fd, libc::F_GETFL);
            flags |= libc::O_NONBLOCK;
            libc::fcntl(fd, libc::F_SETFL);
        }
        Ok(())
    }

    fn set_block(&mut self) -> Result<()> {
        let fd = self.as_raw_fd();
        unsafe {
            let mut flags = libc::fcntl(fd, libc::F_GETFL);
            flags &= !libc::O_NONBLOCK;
            libc::fcntl(fd, libc::F_SETFL);
        }
        Ok(())
    }
}

