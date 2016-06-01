use std::ptr;
use std::os::unix::io::RawFd;
use std::cmp;
use std::io::{Result, Error};
use std::time::Duration;
use std::mem;
use std::fmt;

use libc;
use event::{self, EventSet};

// Returns the highest file descriptor in the given `fd_set`, searching backwards from `prev_max`.
fn find_max(set: &libc::fd_set, prev_max: RawFd) -> RawFd {
    for i in prev_max..0 {
        let isset = unsafe { libc::FD_ISSET(i, set) };
        if isset {
            return i;
        }
    }
    0
}

// Simple wrapper around the raw `select` call.
fn select(nfds: RawFd,
          rset: &mut libc::fd_set,
          wset: &mut libc::fd_set,
          timeout: Option<Duration>)
          -> Result<usize> {
    let tv = if let Some(dur) = timeout {
        let sec = dur.as_secs() as libc::time_t;
        let usec = dur.subsec_nanos() as libc::suseconds_t;

        &mut libc::timeval {
            tv_sec: sec,
            tv_usec: usec,
        } as *mut libc::timeval
    } else {
        ptr::null_mut()
    };

    let res = unsafe { libc::select(nfds, rset, wset, ptr::null_mut(), tv) };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res as usize)
    }
}

/// A set of file descriptors that can be monitored to determine readiness for I/O operations.
pub struct Selector {
    // Highest file descriptor in both `fd_set`s.
    maxfd: RawFd,

    rfds: libc::fd_set,
    wfds: libc::fd_set,
}

impl Selector {
    /// Creates an empty `Selector`.
    pub fn new() -> Result<Selector> {
        unsafe {
            Ok(Selector {
                maxfd: 0,
                rfds: mem::zeroed(),
                wfds: mem::zeroed(),
            })
        }
    }

    pub fn poll(&mut self) -> Result<Iter> {
        // Clone the `fd_set`s as `select` will modify them.
        let mut rfds = self.rfds.clone();
        let mut wfds = self.wfds.clone();
        let nfds = self.maxfd + 1;

        try!(select(nfds, &mut rfds, &mut wfds, None));

        Ok(Iter {
            maxfd: self.maxfd,
            curfd: 0,
            rfds: rfds,
            wfds: wfds,
        })
    }

    pub fn poll_timeout(&mut self, timeout: Duration) -> Result<Iter> {
        // Clone the `fd_set`s as select will modify them.
        let mut rfds = self.rfds.clone();
        let mut wfds = self.wfds.clone();
        let nfds = self.maxfd + 1;

        try!(select(nfds, &mut rfds, &mut wfds, Some(timeout)));

        Ok(Iter {
            maxfd: self.maxfd,
            curfd: 0,
            rfds: rfds,
            wfds: wfds,
        })
    }

    /// Registers a file descriptor with the `Selector`.
    ///
    /// The given file descriptor will be monitored for the events specified in `evset`.
    pub fn register(&mut self, fd: RawFd, evset: EventSet) -> Result<()> {
        if evset.is_readable() {
            unsafe {
                libc::FD_SET(fd, &mut self.rfds);
            }
            self.maxfd = cmp::max(fd, self.maxfd);
        }
        if evset.is_writable() {
            unsafe {
                libc::FD_SET(fd, &mut self.wfds);
            }
            self.maxfd = cmp::max(fd, self.maxfd);
        }

        Ok(())
    }

    /// Re-registers a file descriptor with the `Selector`.
    ///
    /// Re-registration of a file descriptor allows for modification of its associated `EventSet`.
    pub fn reregister(&mut self, fd: RawFd, evset: EventSet) -> Result<()> {
        if evset.intersects(EventSet::readable() | EventSet::writable()) {
            self.register(fd, evset)
        } else {
            self.deregister(fd)
        }
    }

    /// Deregisters a file descriptor with the `Selector`.
    pub fn deregister(&mut self, fd: RawFd) -> Result<()> {
        unsafe {
            libc::FD_CLR(fd, &mut self.rfds);
            libc::FD_CLR(fd, &mut self.rfds);
        }

        // If we removed the highest file descriptor, find the new maximum.
        if fd == self.maxfd {
            self.maxfd = cmp::max(find_max(&self.rfds, fd), find_max(&self.wfds, fd));
        }

        Ok(())
    }
}

impl fmt::Debug for Selector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Might as well give some useful debug info.
        let mut rfds = Vec::new();
        for i in 0..(self.maxfd + 1) {
            let isset = unsafe { libc::FD_ISSET(i, &self.rfds) };
            if isset {
                rfds.push(i);
            }
        }

        let mut wfds = Vec::new();
        for i in 0..(self.maxfd + 1) {
            let isset = unsafe { libc::FD_ISSET(i, &self.wfds) };
            if isset {
                wfds.push(i);
            }
        }

        f.debug_struct("Selector")
            .field("maxfd", &self.maxfd)
            .field("rfds", &rfds)
            .field("wfds", &wfds)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fired {
    fd: RawFd,
    evset: EventSet,
}

impl Fired {
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn evset(&self) -> EventSet {
        self.evset
    }
}

pub struct Iter {
    maxfd: RawFd,
    curfd: RawFd,
    rfds: libc::fd_set,
    wfds: libc::fd_set,
}

impl Iterator for Iter {
    type Item = Fired;

    fn next(&mut self) -> Option<Fired> {
        while self.curfd <= self.maxfd {
            let is_read = unsafe { libc::FD_ISSET(self.curfd, &self.rfds) };
            let is_write = unsafe { libc::FD_ISSET(self.curfd, &self.wfds) };

            if !is_read && !is_write {
                self.curfd += 1;
                continue;
            } else {
                let mut evset = EventSet::empty();

                if is_read {
                    evset.insert(event::READABLE);
                }
                if is_write {
                    evset.insert(event::WRITABLE);
                }

                let fired = Fired {
                    fd: self.curfd,
                    evset: evset,
                };

                self.curfd += 1;
                return Some(fired);
            }
        }
        None
    }
}

impl fmt::Debug for Iter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Might as well give some useful debug info.
        let mut rfds = Vec::new();
        for i in 0..(self.maxfd + 1) {
            let isset = unsafe { libc::FD_ISSET(i, &self.rfds) };
            if isset {
                rfds.push(i);
            }
        }

        let mut wfds = Vec::new();
        for i in 0..(self.maxfd + 1) {
            let isset = unsafe { libc::FD_ISSET(i, &self.wfds) };
            if isset {
                wfds.push(i);
            }
        }

        f.debug_struct("Iter")
            .field("maxfd", &self.maxfd)
            .field("curfd", &self.curfd)
            .field("rfds", &rfds)
            .field("wfds", &wfds)
            .finish()
    }
}
