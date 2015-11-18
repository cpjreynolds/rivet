use std::ptr;
use std::os::unix::io::RawFd;
use std::cmp;
use std::io::{
    Result,
    Error,
};

use libc;
use time::Duration;
use event::{
    EventSet,
    self,
};

mod ffi {
    use std::mem;
    use std::os::unix::io::RawFd;

    use libc;

    const FD_SETSIZE: usize = 1024;

    // This is a hack until there is some way to get the size of a type at compile time.
    // This also assumes the size of a `c_long` is the size of the target's pointer, which is from
    // what I can tell, true. (With the glaring exception of windows, but we don't target them).
    #[cfg(target_pointer_width = "32")]
    const NFD_BITS: usize = 32;
    #[cfg(target_pointer_width = "64")]
    const NFD_BITS: usize = 64;


    #[repr(C)]
    type fd_mask = libc::c_long;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(C)]
    pub struct fd_set {
        fds_bits: [fd_mask; (FD_SETSIZE / NFD_BITS)],
    }

    fn get_elt(fd: RawFd) -> usize {
        fd as usize / NFD_BITS
    }

    fn get_mask(fd: RawFd) -> fd_mask {
        (1usize << (fd as usize % NFD_BITS)) as fd_mask
    }


    impl fd_set {
        // Creates a new `fd_set`.
        pub fn new() -> fd_set {
            unsafe {
                mem::zeroed()
            }
        }

        pub fn set(&mut self, fd: RawFd) {
            self.fds_bits[get_elt(fd)] |= get_mask(fd);
        }

        pub fn unset(&mut self, fd: RawFd) {
            self.fds_bits[get_elt(fd)] &= !get_mask(fd);
        }

        pub fn is_set(&self, fd: RawFd) -> bool {
            (self.fds_bits[get_elt(fd)] & get_mask(fd)) != 0
        }

        pub fn find_max(&self, prev_max: RawFd) -> RawFd {
            let max_idx = get_elt(prev_max);
            for (idx, &elt) in self.fds_bits[..(max_idx + 1)].iter().enumerate().rev() {
                if elt != 0 {
                    let zeros = elt.leading_zeros() as usize;
                    let shift = NFD_BITS - (zeros + 1);

                    let new_max = (idx * NFD_BITS) + shift;

                    return new_max as RawFd;
                }
            }
            0
        }
    }

    extern {
        pub fn select(nfds: libc::c_int,
                      readfds: *mut fd_set,
                      writefds: *mut fd_set,
                      exceptfds: *mut fd_set,
                      timeout: *mut libc::timeval) -> libc::c_int;

    }
}

fn select(nfds: RawFd,
          rset: &mut ffi::fd_set,
          wset: &mut ffi::fd_set,
          timeout: Option<Duration>) -> Result<usize>
{
    let tv = if let Some(dur) = timeout {
        let sec = dur.num_seconds() as libc::time_t;
        let usec = (dur - Duration::seconds(sec))
            .num_microseconds().unwrap() as libc::suseconds_t;

        &mut libc::timeval {
            tv_sec: sec,
            tv_usec: usec,
        } as *mut libc::timeval
    } else {
        ptr::null_mut()
    };

    let res = unsafe {
        ffi::select(nfds, rset, wset, ptr::null_mut(), tv)
    };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res as usize)
    }
}

#[derive(Debug)]
pub struct Selector {
    maxfd: RawFd,

    rfds: ffi::fd_set,
    wfds: ffi::fd_set,

    // Copies of above as `select` will modify the `fd_set`s.
    _rfds: ffi::fd_set,
    _wfds: ffi::fd_set,
}

impl Selector {
    pub fn new() -> Result<Selector> {
        Ok(Selector {
            maxfd: 0,
            rfds: ffi::fd_set::new(),
            wfds: ffi::fd_set::new(),
            _rfds: ffi::fd_set::new(),
            _wfds: ffi::fd_set::new(),
        })
    }

    pub fn poll(&mut self) -> Result<IterFired> {
        self._rfds = self.rfds.clone();
        self._wfds = self.wfds.clone();

        try!(select(self.maxfd + 1, &mut self._rfds, &mut self._wfds, None));

        Ok(IterFired {
            maxfd: self.maxfd,
            curfd: 0,
            rfds: &self._rfds,
            wfds: &self._wfds,
        })
    }

    pub fn poll_timeout(&mut self, timeout: Duration) -> Result<IterFired> {
        self._rfds = self.rfds.clone();
        self._wfds = self.wfds.clone();

        try!(select(self.maxfd + 1, &mut self._rfds, &mut self._wfds, Some(timeout)));

        Ok(IterFired {
            maxfd: self.maxfd,
            curfd: 0,
            rfds: &self._rfds,
            wfds: &self._wfds,
        })
    }

    pub fn register(&mut self, fd: RawFd, evset: EventSet) -> Result<()> {
        if evset.is_readable() {
            self.rfds.set(fd);
            self.maxfd = cmp::max(fd, self.maxfd);
        }
        if evset.is_writable() {
            self.wfds.set(fd);
            self.maxfd = cmp::max(fd, self.maxfd);
        }

        Ok(())
    }

    pub fn reregister(&mut self, fd: RawFd, evset: EventSet) -> Result<()> {
        if evset.intersects(event::READABLE | event::WRITABLE) {
            self.register(fd, evset)
        } else {
            self.deregister(fd)
        }
    }

    pub fn deregister(&mut self, fd: RawFd) -> Result<()> {
        self.rfds.unset(fd);
        self.wfds.unset(fd);

        if fd == self.maxfd {
            self.maxfd = cmp::max(self.rfds.find_max(fd), self.wfds.find_max(fd));
        }

        Ok(())
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

#[derive(Debug)]
pub struct IterFired<'a> {
    maxfd: RawFd,
    curfd: RawFd,
    rfds: &'a ffi::fd_set,
    wfds: &'a ffi::fd_set,
}

impl<'a> Iterator for IterFired<'a> {
    type Item = Fired;

    fn next(&mut self) -> Option<Fired> {
        while self.curfd <= self.maxfd {
            let is_read = self.rfds.is_set(self.curfd);
            let is_write = self.wfds.is_set(self.curfd);

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
