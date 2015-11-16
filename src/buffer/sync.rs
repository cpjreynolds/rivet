use std::mem;
use std::cmp;
use std::fmt;
use std::ptr;
use std::slice;
use std::os::unix::io::AsRawFd;
use std::thread::{
    self,
    Thread,
};
use std::cell::UnsafeCell;
use std::sync::{
    Arc,
    Condvar,
    Mutex,
};
use std::sync::atomic::{
    AtomicPtr,
    AtomicUsize,
    Ordering,
};
use std::io::Result;

use super::map::{
    self,
    Mapping,
    Shm,
    MapBuilder,
};

pub fn ring(cap: usize) -> Result<(Producer, Consumer)> {
    let ring = try!(Ring::new(cap));
    let ring = Arc::new(UnsafeCell::new(ring));
    Ok((Producer(ring.clone()), Consumer(ring)))
}

pub struct Producer(Arc<UnsafeCell<Ring>>);

unsafe impl Send for Producer {}

impl Producer {
    pub fn try_write(&self, buf: &[u8]) -> usize {
        unsafe {
            (*self.0.get()).try_write(buf)
        }
    }

    pub fn write(&self, buf: &[u8]) -> Option<usize> {
        unsafe {
            (*self.0.get()).write(buf)
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        unsafe {
            (*self.0.get()).capacity()
        }
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        unsafe {
            (*self.0.get()).disconnect()
        }
    }
}

pub struct Consumer(Arc<UnsafeCell<Ring>>);

unsafe impl Send for Consumer {}

impl Consumer {
    #[inline]
    pub fn capacity(&self) -> usize {
        unsafe {
            (*self.0.get()).capacity()
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> Option<usize> {
        unsafe {
            (*self.0.get()).read(buf)
        }
    }

    pub fn try_read(&self, buf: &mut [u8]) -> usize {
        unsafe {
            (*self.0.get()).try_read(buf)
        }
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        unsafe {
            (*self.0.get()).disconnect()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Open,
    Blocked,
    Disconnected,
}

struct Ring {
    _pad0: [u8; 64], // Cache line padding.
    cap: usize, // Total capacity.
    mask: usize, // Mask for indexing mapping.
    map: Mapping,
    lock: Mutex<State>,
    cvar: Condvar,
    _pad1: [u8; 64],
    head: AtomicUsize,
    _pad2: [u8; 64],
    tail: AtomicUsize,
}

impl Ring {
    #[inline]
    fn capacity(&self) -> usize {
        self.cap
    }

    fn new(cap: usize) -> Result<Ring> {
        // Ensure capacity is a multiple of the system's page size.
        let cap = map::page_aligned(cap);

        // Reserve twice the capacity as an anonymous mapping.
        let map = try!(MapBuilder::new()
                       .flags(map::MAP_ANONYMOUS | map::MAP_PRIVATE)
                       .len(cap << 1)
                       .create());

        let lower_ptr = map.ptr();
        let upper_ptr = unsafe { lower_ptr.offset(cap as isize) };

        // Create shm object to map into memory.
        let mut shm = try!(Shm::new());
        try!(shm.set_len(cap));
        let memfd = shm.as_raw_fd();

        let lower_map = try!(MapBuilder::new()
                             .prot(map::PROT_READ | map::PROT_WRITE)
                             .flags(map::MAP_FIXED | map::MAP_SHARED)
                             .fd(memfd)
                             .len(cap)
                             .addr(lower_ptr)
                             .create());
        let upper_map = try!(MapBuilder::new()
                              .prot(map::PROT_READ | map::PROT_WRITE)
                              .flags(map::MAP_FIXED | map::MAP_SHARED)
                              .fd(memfd)
                              .len(cap)
                              .addr(upper_ptr)
                              .create());

        // Forget upper and lower mappings so they don't unmap the backing memory on drop.
        // Backing map will unmap memory when `Ring` is dropped.
        mem::forget(lower_map);
        mem::forget(upper_map);

        Ok(Ring {
            _pad0: [0; 64],
            cap: cap,
            mask: cap - 1,
            map: map,
            lock: Mutex::new(State::Open),
            cvar: Condvar::new(),
            _pad1: [0; 64],
            head: AtomicUsize::new(0),
            _pad2: [0; 64],
            tail: AtomicUsize::new(0),
        })
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let navail = self.cap - (head - self.tail.load(Ordering::Acquire));
        let nwrit = cmp::min(navail, buf.len());
        let offset = (head & self.mask) as isize;
        unsafe {
            let src = buf.as_ptr();
            let dest = self.ptr().offset(offset);
            ptr::copy_nonoverlapping(src, dest, nwrit);
        }
        self.head.store(head + nwrit, Ordering::Release);
        self.unblock();
        nwrit
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let navail = self.head.load(Ordering::Acquire) - tail;
        let nread = cmp::min(navail, buf.len());
        let offset = (tail & self.mask) as isize;
        unsafe {
            let src = self.ptr().offset(offset);
            let dest = buf.as_mut_ptr();
            ptr::copy_nonoverlapping(src, dest, nread);
        }
        self.tail.store(tail + nread, Ordering::Release);
        self.unblock();
        nread
    }

    fn try_read(&mut self, buf: &mut [u8]) -> Option<usize> {
        loop {
            match self.try_read(buf) {
                0 => if self.block() == State::Disconnected { return None },
                n => return Some(n),
            }
        }
    }

    fn try_write(&mut self, buf: &[u8]) -> Option<usize> {
        loop {
            match self.try_write(buf) {
                0 => if self.block() == State::Disconnected { return None },
                n => return Some(n),
            }
        }
    }

    fn disconnect(&mut self) {
        let mut guard = self.lock.lock().unwrap();
        *guard = State::Disconnected;
        self.cvar.notify_all();
    }

    fn block(&mut self) -> State {
        let mut guard = self.lock.lock().unwrap();
        while *guard == State::Blocked {
            guard = self.cvar.wait(guard).unwrap();
        }
        if *guard == State::Disconnected {
            State::Disconnected
        } else {
            *guard = State::Open;
            State::Open
        }
    }

    fn unblock(&mut self) -> State {
        let mut guard = self.lock.lock().unwrap();
        if *guard == State::Disconnected {
            State::Disconnected
        } else {
            *guard = State::Open;
            self.cvar.notify_one();
            State::Open
        }
    }

    fn ptr(&self) -> *mut u8 {
        self.map.ptr()
    }
}

impl fmt::Debug for Ring {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Ring")
            .field("cap", &self.cap)
            .field("mask", &self.mask)
            .field("map", &self.map)
            .field("head", &self.head)
            .field("tail", &self.tail)
            .finish()
    }
}

