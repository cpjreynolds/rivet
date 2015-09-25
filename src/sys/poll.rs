use std::ops::{
    Deref,
    DerefMut,
};
use std::io::{
    self,
    ErrorKind,
    Error,
    Result,
};
use std::mem;
use std::fs::File;
use std::rc::Rc;
use std::os::unix::io::{
    RawFd,
    AsRawFd,
};

use libc;
use time::Duration;

mod ffi {
    use libc::{
        c_int,
        c_uint,
        c_short,
    };

    #[repr(C)]
    pub type nfds_t = c_uint;

    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct pollfd {
        pub fd: c_int,
        pub events: c_short,
        pub revents: c_short,
    }

    extern {
        pub fn poll(fds: *mut pollfd, nfds: nfds_t, timeout_ms: c_int) -> c_int;
    }
}

bitflags! {
    #[repr(C)]
    flags PollFlag: libc::c_short {
        const POLLIN = 0x0001,
        const POLLPRI = 0x0002,
        const POLLOUT = 0x0004,
        const POLLERR = 0x0008,
        const POLLHUP = 0x0010,
        const POLLNVAL = 0x0020,
    }
}

fn poll(fds: &mut [ffi::pollfd], timeout: Duration) -> Result<usize> {
    let timeout_ms = timeout.num_milliseconds() as libc::c_int;
    loop {
        let res = unsafe {
            ffi::poll(fds.as_mut_ptr(), fds.len() as ffi::nfds_t, timeout_ms)
        };
        if res == -1 {
            match Error::last_os_error() {
                // If we're interrupted, start the call again.
                ref e if e.kind() == ErrorKind::Interrupted => continue,
                // Otherwise return the error.
                other => return Err(other),
            }
        } else {
            return Ok(res as usize)
        }
    }
}

pub struct Selector {
    pfds: Vec<ffi::pollfd>,
}

impl Selector {
    pub fn new() -> Result<Selector> {
        Ok(Selector {
            pfds: Vec::with_capacity(1024),
        })
    }

    pub fn select(&mut self, timeout_ms: usize) -> Result<usize> {
        let nevents = try!(poll(&mut self.pfds, Duration::zero()));
        Ok(nevents)
    }

    pub fn register(&mut self, fd: RawFd) -> Result<()> {
        let pfd = ffi::pollfd {
            fd: fd,
            events: (POLLIN | POLLOUT).bits,
            revents: 0,
        };
        self.pfds.push(pfd);

        Ok(())
    }

    pub fn reregister(&mut self, fd: RawFd) -> Result<()> {
        let new_pfd = ffi::pollfd {
            fd: fd,
            events: (POLLIN | POLLOUT).bits,
            revents: 0,
        };

        for pfd in self.pfds.iter_mut() {
            if pfd.fd == new_pfd.fd {
                mem::replace(pfd, new_pfd);
                return Ok(())
            }
        }
        Err(Error::new(ErrorKind::NotFound, "fd to reregister not found"))
    }

    pub fn deregister(&mut self, fd: RawFd) -> Result<()> {
        self.pfds.remove(fd as usize);
        Ok(())
    }
}

