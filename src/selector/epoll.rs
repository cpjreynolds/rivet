use std::os::unix::io::RawFd;
use std::slice;
use std::io::{
    Result,
    Error,
};
use std::iter::{
    Iterator,
    DoubleEndedIterator,
    ExactSizeIterator,
};


use libc;
use time::Duration;

use event::EventSet;

#[allow(dead_code)]
mod ffi {
    use libc::c_int;
    use event::EventSet;

    bitflags! {
        #[repr(C)]
        pub flags EpollFlag: c_int {
            const EPOLLIN = 0x001,
            const EPOLLOUT = 0x004,
            const EPOLLERR = 0x008,
            const EPOLLHUP = 0x010,
            const EPOLLRDHUP = 0x2000,
        }
    }

    impl From<EventSet> for EpollFlag {
        fn from(evts: EventSet) -> EpollFlag {
            let mut epflag = EpollFlag::empty();

            if evts.is_readable() {
                epflag.insert(EPOLLIN);
            }
            if evts.is_writable() {
                epflag.insert(EPOLLOUT);
            }
            if evts.is_error() {
                epflag.insert(EPOLLERR);
            }
            if evts.is_hup() {
                epflag.insert(EPOLLRDHUP);
            }

            epflag
        }
    }

    impl Into<EventSet> for EpollFlag {
        fn into(self) -> EventSet {
            let mut evts = EventSet::empty();

            if self.contains(EPOLLIN) {
                evts.insert(EventSet::readable());
            }
            if self.contains(EPOLLOUT) {
                evts.insert(EventSet::writable());
            }
            if self.contains(EPOLLERR) {
                evts.insert(EventSet::error());
            }
            if self.contains(EPOLLHUP) || self.contains(EPOLLRDHUP) {
                evts.insert(EventSet::hup());
            }

            evts
        }
    }

    bitflags! {
        #[repr(C)]
        pub flags EpollOp: c_int {
            const EPOLL_CTL_ADD = 1,
            const EPOLL_CTL_DEL = 2,
            const EPOLL_CTL_MOD = 3,
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    #[derive(Debug, Clone, Copy)]
    #[repr(C)]
    pub struct epoll_event {
        pub events: EpollFlag,
        pub data: u64,
    }

    #[cfg(target_arch = "x86_64")]
    #[derive(Debug, Clone, Copy)]
    #[repr(C, packed)]
    pub struct epoll_event {
        pub events: EpollFlag,
        pub data: u64,
    }

    extern {
        pub fn epoll_create(size: c_int) -> c_int;
        pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *const epoll_event) -> c_int;
        pub fn epoll_wait(epfd: c_int,
                          events: *mut epoll_event,
                          maxevents: c_int,
                          timeout: c_int) -> c_int;
    }
}

fn epoll_create() -> Result<RawFd> {
    let res = unsafe { ffi::epoll_create(1024) };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res)
    }
}

fn epoll_ctl(epfd: RawFd, op: ffi::EpollOp, fd: RawFd, event: &ffi::epoll_event) -> Result<()> {
    let res = unsafe {
        ffi::epoll_ctl(epfd, op.bits(), fd, event)
    };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

fn epoll_wait(epfd: RawFd, events: &mut [ffi::epoll_event], timeout: Duration) -> Result<usize> {
    let res = unsafe {
        ffi::epoll_wait(epfd,
                        events.as_mut_ptr(),
                        events.len() as libc::c_int,
                        timeout.num_milliseconds() as libc::c_int)
    };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res as usize)
    }
}


pub struct Selector {
    epfd: RawFd,
    events: Vec<ffi::epoll_event>,
}

impl Selector {
    pub fn new() -> Result<Selector> {
        let epfd = try!(epoll_create());

        Ok(Selector {
            epfd: epfd,
            events: Vec::with_capacity(1024),
        })
    }

    pub fn poll(&mut self) -> Result<IterFired> {
        let timeout = Duration::milliseconds(-1);
        self.poll_timeout(timeout)
    }

    pub fn poll_timeout(&mut self, timeout: Duration) -> Result<IterFired> {
        // Pass kernel the entire length of the `events` buffer, it will overwrite the memory as
        // needed and return the new length.
        let dst = unsafe {
            slice::from_raw_parts_mut(
                self.events.as_mut_ptr(),
                self.events.capacity())
        };

        // `events` becomes unsafe to access after this call.
        let nevents = try!(epoll_wait(self.epfd, dst, timeout));

        // `events` is now safe to access again.
        unsafe {
            self.events.set_len(nevents);
        }

        Ok(IterFired(self.events.iter()))
    }

    pub fn register(&mut self, fd: RawFd, evts: EventSet) -> Result<()> {
        let evt = ffi::epoll_event {
            events: evts.into(),
            data: fd as u64,
        };

        epoll_ctl(self.epfd, ffi::EPOLL_CTL_ADD, fd, &evt)
    }

    pub fn reregister(&mut self, fd: RawFd, evts: EventSet) -> Result<()> {
        let evt = ffi::epoll_event {
            events: evts.into(),
            data: fd as u64,
        };

        epoll_ctl(self.epfd, ffi::EPOLL_CTL_MOD, fd, &evt)
    }

    pub fn deregister(&mut self, fd: RawFd) -> Result<()> {
        let evt = ffi::epoll_event {
            events: ffi::EpollFlag::empty(),
            data: 0,
        };

        epoll_ctl(self.epfd, ffi::EPOLL_CTL_DEL, fd, &evt)
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = unsafe {
            libc::close(self.epfd)
        };
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

    fn from_epoll(epev: &ffi::epoll_event) -> Fired {
        Fired {
            fd: epev.data as RawFd,
            evset: epev.events.into(),
        }
    }
}

/// Iterator over the fired events of a `Selector`.
pub struct IterFired<'a>(slice::Iter<'a, ffi::epoll_event>);

impl<'a> Iterator for IterFired<'a> {
    type Item = Fired;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Fired::from_epoll)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a> ExactSizeIterator for IterFired<'a> {}

impl<'a> DoubleEndedIterator for IterFired<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(Fired::from_epoll)
    }
}

