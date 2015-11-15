extern crate rev;
extern crate time;
extern crate libc;

use std::mem;
use std::io;
use std::io::prelude::*;
use std::os::unix::io::RawFd;

use rev::{Selector, EventSet};
use time::Duration;

struct Pipe {
    read: libc::c_int,
    write: libc::c_int,
}

impl Pipe {
    fn new() -> io::Result<Pipe> {
        let mut fds = [0 as libc::c_int; 2];
        unsafe {
            let ret = libc::pipe(mem::transmute(&mut fds));
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Pipe {
            read: fds[0],
            write: fds[1],
        })
    }
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = unsafe {
            let ptr = mem::transmute(buf.as_ptr());
            libc::read(self.read, ptr, buf.len() as libc::size_t)
        };
        match ret {
            -1 => Err(io::Error::last_os_error()),
            n => Ok(n as usize),
        }
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let ret = unsafe {
            let ptr = mem::transmute(buf.as_ptr());
            libc::write(self.write, ptr, buf.len() as libc::size_t)
        };
        match ret {
            -1 => Err(io::Error::last_os_error()),
            n => Ok(n as usize),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.read);
            libc::close(self.write);
        }
    }
}

#[test]
fn test_poll_timeout() {
    fn count_events(selector: &mut Selector) -> usize {
        selector.poll_timeout(Duration::milliseconds(100)).unwrap().count()
    }

    let mut pipe = Pipe::new().unwrap();
    let mut selector = Selector::new().unwrap();

    selector.register(pipe.read, EventSet::readable()).unwrap();

    assert_eq!(count_events(&mut selector), 0);
    pipe.write_all(b"hello world").unwrap();
    assert_eq!(count_events(&mut selector), 1);
}

#[test]
fn test_poll() {
    fn count_events(selector: &mut Selector) -> usize {
        selector.poll().unwrap().count()
    }

    let mut pipe1 = Pipe::new().unwrap();
    let mut pipe2 = Pipe::new().unwrap();
    let mut selector = Selector::new().unwrap();

    selector.register(pipe1.read, EventSet::readable()).unwrap();
    selector.register(pipe2.read, EventSet::readable()).unwrap();

    pipe1.write_all(b"twelve bytes").unwrap();
    assert_eq!(count_events(&mut selector), 1);
    pipe2.write_all(b"more data").unwrap();
    assert_eq!(count_events(&mut selector), 2);
    let mut buf = [0; 12];
    assert_eq!(pipe1.read(&mut buf).unwrap(), 12);
    assert_eq!(count_events(&mut selector), 1);
}

#[test]
fn test_deregister() {
    fn first_fd(selector: &mut Selector) -> RawFd {
        selector.poll().unwrap().next().unwrap().fd()
    }

    let mut pipe1 = Pipe::new().unwrap();
    let mut pipe2 = Pipe::new().unwrap();
    let mut selector = Selector::new().unwrap();

    selector.register(pipe1.read, EventSet::readable()).unwrap();
    selector.register(pipe2.read, EventSet::readable()).unwrap();
    pipe1.write_all(b"abc").unwrap();
    pipe2.write_all(b"def").unwrap();

    selector.deregister(pipe1.read).unwrap();
    assert_eq!(first_fd(&mut selector), pipe2.read);
    selector.register(pipe1.read, EventSet::readable()).unwrap();
    selector.deregister(pipe2.read).unwrap();
    assert_eq!(first_fd(&mut selector), pipe1.read);
}