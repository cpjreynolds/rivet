use std::os::unix::io::RawFd;
use std::slice;
use std::io::{
    Result,
    Error,
};

use nix::sys::epoll::*;
use nix::unistd;
use time::Duration;

pub struct Selector {
    epfd: RawFd,
    events: Vec<EpollEvent>,
}

impl Selector {
    pub fn new() -> Result<Selector> {
        let epfd = try!(epoll_create().map_err(from_nix));

        Ok(Selector {
            epfd: epfd,
            events: Vec::with_capacity(1024),
        })
    }

    pub fn select(&mut self) -> Result<usize> {
        let timeout = Duration::milliseconds(-1);
        self.select_timeout(timeout)
    }

    pub fn select_timeout(&mut self, timeout: Duration) -> Result<usize> {

        // Pass kernel the entire length of the `events` buffer, it will overwrite the memory as
        // needed and return the new length.
        let dst = unsafe {
            slice::from_raw_parts_mut(
                self.events.as_mut_ptr(),
                self.events.capacity())
        };

        let timeout_ms = timeout.num_milliseconds() as isize;

        // `events` becomes unsafe to access after this call.
        let nevents = try!(epoll_wait(self.epfd, dst, timeout_ms)
                           .map_err(from_nix));

        // `events` is now safe to access again.
        unsafe {
            self.events.set_len(nevents);
        }

        Ok(nevents)
    }

    pub fn register(&mut self, fd: RawFd) -> Result<()> {
        let evt = EpollEvent {
            events: EPOLLIN | EPOLLOUT,
            data: fd as u64,
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, fd, &evt)
            .map_err(from_nix)
    }

    pub fn reregister(&mut self, fd: RawFd) -> Result<()> {
        let evt = EpollEvent {
            events: EPOLLIN | EPOLLOUT,
            data: fd as u64,
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlMod, fd, &evt)
            .map_err(from_nix)
    }

    pub fn deregister(&mut self, fd: RawFd) -> Result<()> {
        let evt = EpollEvent {
            events: EpollEventKind::empty(),
            data: 0,
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlDel, fd, &evt)
            .map_err(from_nix)
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = unistd::close(self.epfd);
    }
}

fn from_nix(err: ::nix::Error) -> ::std::io::Error {
    ::std::io::Error::from_raw_os_error(err.errno() as i32)
}
