use std::os::unix::io::RawFd;
use std::io::{Result, Error};
use std::ptr;
use std::slice;

use libc;
use time::Duration;

use event::EventSet;

#[allow(dead_code)]
mod ffi {
    use libc;

    use event::EventSet;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(i16)]
    #[repr(C)]
    pub enum EventFilter {
        EVFILT_READ = -1,
        EVFILT_WRITE = -2,
    }
    pub use self::EventFilter::{EVFILT_READ, EVFILT_WRITE};

    impl Into<EventSet> for EventFilter {
        fn into(self) -> EventSet {
            match self {
                EVFILT_READ => EventSet::readable(),
                EVFILT_WRITE => EventSet::writable(),
            }
        }
    }

    bitflags! {
        #[repr(C)]
        flags EventFlag: libc::c_ushort {
            const EV_ADD = 0x0001,
            const EV_DELETE = 0x0002,
            const EV_ENABLE = 0x0004,
            const EV_DISABLE = 0x0008,
            const EV_RECEIPT = 0x0040,
            const EV_ONESHOT = 0x0010,
            const EV_CLEAR = 0x0020,
            const EV_EOF = 0x8000,
            const EV_ERROR = 0x4000,
        }
    }

    // Electing to use usize and isize instead of uintptr and intptr to alleviate some trivial
    // casting. If this shouldn't be done for some reason, please let me know.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(C)]
    pub struct kevent {
        pub ident: usize,
        pub filter: EventFilter,
        pub flags: EventFlag,
        pub fflags: libc::c_uint,
        pub data: isize,
        pub udata: usize,
    }

    impl Default for kevent {
        fn default() -> kevent {
            kevent {
                ident: 0,
                filter: EVFILT_READ,
                flags: EventFlag::empty(),
                fflags: 0,
                data: 0,
                udata: 0,
            }
        }
    }

    // rustc does not recognize that kevent is repr(C).
    // See nix/src/sys/event.rs.
    #[allow(improper_ctypes)]
    extern "C" {
        pub fn kqueue() -> libc::c_int;

        pub fn kevent(kq: libc::c_int,
                      changelist: *const kevent,
                      nchanges: libc::c_int,
                      eventlist: *mut kevent,
                      nevents: libc::c_int,
                      timeout: *const libc::timespec)
                      -> libc::c_int;
    }
}

fn kqueue() -> Result<RawFd> {
    let res = unsafe { ffi::kqueue() };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res)
    }
}

fn kevent(kq: RawFd,
          changelist: &[ffi::kevent],
          eventlist: &mut [ffi::kevent],
          timeout: Option<Duration>)
          -> Result<usize> {
    let tspec = if let Some(dur) = timeout {
        let sec = dur.num_seconds() as libc::time_t;
        let nsec = (dur - Duration::seconds(sec))
            .num_nanoseconds()
            .unwrap() as libc::c_long;

        &libc::timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        } as *const libc::timespec
    } else {
        ptr::null()
    };

    let res = unsafe {
        ffi::kevent(kq,
                    changelist.as_ptr(),
                    changelist.len() as libc::c_int,
                    eventlist.as_mut_ptr(),
                    eventlist.len() as libc::c_int,
                    tspec)
    };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res as usize)
    }

}

#[derive(Debug)]
pub struct Selector {
    kqfd: RawFd,
    events: Vec<ffi::kevent>,
}

impl Selector {
    pub fn new() -> Result<Selector> {
        let kqfd = try!(kqueue());

        Ok(Selector {
            kqfd: kqfd,
            events: Vec::with_capacity(1024),
        })
    }

    pub fn poll(&mut self) -> Result<IterFired> {
        let dst =
            unsafe { slice::from_raw_parts_mut(self.events.as_mut_ptr(), self.events.capacity()) };

        let nevents = try!(kevent(self.kqfd, &[], dst, None));

        unsafe {
            self.events.set_len(nevents);
        }

        Ok(IterFired(self.events.iter()))
    }

    pub fn poll_timeout(&mut self, timeout: Duration) -> Result<IterFired> {
        let dst =
            unsafe { slice::from_raw_parts_mut(self.events.as_mut_ptr(), self.events.capacity()) };

        let nevents = try!(kevent(self.kqfd, &[], dst, Some(timeout)));

        unsafe {
            self.events.set_len(nevents);
        }

        Ok(IterFired(self.events.iter()))

    }

    pub fn register(&mut self, fd: RawFd, evts: EventSet) -> Result<()> {
        let mut ke = ffi::kevent {
            ident: fd as usize,
            flags: ffi::EV_ADD,
            ..Default::default()
        };

        if evts.is_readable() {
            ke = ffi::kevent {
                filter: ffi::EVFILT_READ,
                flags: ke.flags | ffi::EV_ENABLE,
                ..ke
            };
            try!(kevent(self.kqfd, &[ke], &mut [], None));
        } else {
            ke = ffi::kevent {
                filter: ffi::EVFILT_READ,
                flags: ke.flags | ffi::EV_DISABLE,
                ..ke
            };
            try!(kevent(self.kqfd, &[ke], &mut [], None));
        }
        if evts.is_writable() {
            ke = ffi::kevent {
                filter: ffi::EVFILT_WRITE,
                flags: ke.flags | ffi::EV_ENABLE,
                ..ke
            };
            try!(kevent(self.kqfd, &[ke], &mut [], None));
        } else {
            ke = ffi::kevent {
                filter: ffi::EVFILT_WRITE,
                flags: ke.flags | ffi::EV_DISABLE,
                ..ke
            };
            try!(kevent(self.kqfd, &[ke], &mut [], None));
        }


        Ok(())
    }

    pub fn reregister(&mut self, fd: RawFd, evts: EventSet) -> Result<()> {
        self.register(fd, evts)
    }

    pub fn deregister(&mut self, fd: RawFd) -> Result<()> {
        let ke = ffi::kevent {
            ident: fd as usize,
            flags: ffi::EV_DELETE,
            ..Default::default()
        };

        let rd = ffi::kevent { filter: ffi::EVFILT_READ, ..ke };
        try!(kevent(self.kqfd, &[rd], &mut [], None));

        let wd = ffi::kevent { filter: ffi::EVFILT_WRITE, ..ke };
        try!(kevent(self.kqfd, &[wd], &mut [], None));

        Ok(())
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = unsafe { libc::close(self.kqfd) };
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

    fn from_kevent(kevt: &ffi::kevent) -> Fired {
        Fired {
            fd: kevt.ident as RawFd,
            evset: kevt.filter.into(),
        }
    }
}

/// Iterator over the fired events of a `Selector`.
pub struct IterFired<'a>(slice::Iter<'a, ffi::kevent>);

impl<'a> Iterator for IterFired<'a> {
    type Item = Fired;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Fired::from_kevent)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a> ExactSizeIterator for IterFired<'a> {}

impl<'a> DoubleEndedIterator for IterFired<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(Fired::from_kevent)
    }
}
