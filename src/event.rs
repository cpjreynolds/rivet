use std::os::unix::io::RawFd;
use std::ops::{
    BitAnd,
    BitOr,
    BitXor,
    Sub,
    Not,
};

pub struct Event {
    pub fd: RawFd,
    pub set: EventSet,
}

bitflags! {
    /// The set of events associated with a file descriptor.
    flags EventSet: usize {
        const READABLE = 0b0001,
        const WRITABLE = 0b0010,
        const ERROR = 0b0100,
        const HUP = 0b1000,
    }
}

impl EventSet {
    pub fn readable() -> EventSet {
        READABLE
    }

    pub fn is_readable(&self) -> bool {
        self.contains(READABLE)
    }

    pub fn writable() -> EventSet {
        WRITABLE
    }

    pub fn is_writable(&self) -> bool {
        self.contains(WRITABLE)
    }

    pub fn error() -> EventSet {
        ERROR
    }

    pub fn is_error(&self) -> bool {
        self.contains(ERROR)
    }

    pub fn hup() -> EventSet {
        HUP
    }

    pub fn is_hup(&self) -> bool {
        self.contains(HUP)
    }
}
