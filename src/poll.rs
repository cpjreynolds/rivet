use std::os::unix::io::AsRawFd;
use std::ops::{
    Deref,
    DerefMut,
};
use std::io::{
    self,
    ErrorKind,
};
use std::vec;

use libc;
use nix::fcntl::{
    self,
    fcntl,
    OFlag,
    FcntlArg,
};
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
        pub fn poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int;
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

fn poll(fds: &mut [ffi::pollfd], timeout: Duration) -> Result<usize, io::Error> {
    let timeout_ms = timeout.num_milliseconds() as libc::c_int;
    loop {
        let res = unsafe {
            ffi::poll(fds.as_mut_ptr(), fds.len() as ffi::nfds_t, timeout_ms)
        };
        if res == -1 {
            match io::Error::last_os_error() {
                // If we're interrupted, start the call again.
                ref e if e.kind() == ErrorKind::Interrupted => continue,
                other => return Err(other),
            }
        } else {
            return Ok(res as usize)
        }
    }
}

pub struct Poll<T> {
    hdls: Vec<Handle<T>>,
    pfds: Vec<ffi::pollfd>,
}

impl<T> Poll<T>
    where T: AsRawFd
{
    pub fn new() -> Poll<T> {
        Poll {
            hdls: Vec::new(),
            pfds: Vec::new(),
        }
    }

    pub fn add(&mut self, hdl: Handle<T>) {
        self.pfds.push(hdl.as_pollfd());
        self.hdls.push(hdl);
    }

    pub fn poll(&mut self) -> Result<Events<T>, io::Error> {
        let tm = Duration::milliseconds(-1);
        self.poll_timeout(tm)
    }

    pub fn poll_timeout(&mut self, timeout: Duration) -> Result<Events<T>, io::Error> {
        try!(poll(&mut self.pfds, timeout));
        let events = self.pfds.iter()
            .zip(self.hdls.iter_mut())
            .filter_map(|(pfd, hdl)| {
                let revts = PollFlag::from_bits_truncate(pfd.revents);
                if revts.intersects(hdl.evts) {
                    Some(Event {
                        hdl: hdl,
                        evts: revts,
                    })
                } else {
                    None
                }
            }).collect::<Vec<Event<T>>>();

        Ok(events.into_iter())
    }
}

pub struct Handle<T> {
    fd: T,
    evts: PollFlag,
}

impl<T> Handle<T>
    where T: AsRawFd
{
    fn as_pollfd(&self) -> ffi::pollfd {
        ffi::pollfd {
            fd: self.fd.as_raw_fd(),
            events: self.evts.bits(),
            revents: 0,
        }
    }

    pub fn new(fd: T, evts: PollFlag) -> Handle<T> {
        Handle {
            fd: fd,
            evts: evts,
        }
    }
}

impl<T> Deref for Handle<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.fd
    }
}

impl<T> DerefMut for Handle<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.fd
    }
}

type Events<'a, T> = vec::IntoIter<Event<'a, T>>;

pub struct Event<'a, T>
    where T: 'a
{
    hdl: &'a mut Handle<T>,
    evts: PollFlag,
}

impl<'a, T> Event<'a, T>
    where T: AsRawFd
{
    pub fn handle(&mut self) -> &mut Handle<T> {
        &mut self.hdl
    }

    pub fn is_readable(&self) -> bool {
        self.evts.contains(POLLIN)
    }
}

pub trait NonBlocking {
    fn set_nonblock(&mut self) -> Result<(), Error>;
    fn set_block(&mut self) -> Result<(), Error>;
}

impl<T> NonBlocking for T
    where T: AsRawFd
{
    fn set_nonblock(&mut self) -> Result<(), Error> {
        let rawfd = self.as_raw_fd();
        let rawfl = try!(fcntl(rawfd, FcntlArg::F_GETFL));
        let mut flags = OFlag::from_bits_truncate(rawfl);
        flags.insert(fcntl::O_NONBLOCK);
        try!(fcntl(rawfd, FcntlArg::F_SETFL(flags)));
        Ok(())
    }

    fn set_block(&mut self) -> Result<(), Error> {
        let rawfd = self.as_raw_fd();
        let rawfl = try!(fcntl(rawfd, FcntlArg::F_GETFL));
        let mut flags = OFlag::from_bits_truncate(rawfl);
        flags.remove(fcntl::O_NONBLOCK);
        try!(fcntl(rawfd, FcntlArg::F_SETFL(flags)));
        Ok(())
    }
}

