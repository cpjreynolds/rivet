use std::io::prelude::*;
use std::io::{
    Result,
    ErrorKind,
};

pub trait ReadExt: Read {
    fn read_nb(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut nread = 0;

        loop {
            if nread == buf.len() {
                return Ok(nread);
            }

            match self.read(&mut buf[nread..]) {
                Ok(0) => return Ok(nread),
                Ok(n) => nread += n,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {},
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => return Ok(nread),
                Err(e) => return Err(e),
            }
        }
    }
}

impl<T> ReadExt for T where T: Read {}

pub trait WriteExt: Write {
    fn write_nb(&mut self, buf: &[u8]) -> Result<usize> {
        let mut nwrit: usize = 0;

        loop {
            if nwrit == buf.len() {
                return Ok(nwrit);
            }

            match self.write(&buf[nwrit..]) {
                Ok(0) => return Ok(nwrit),
                Ok(n) => nwrit += n,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {},
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => return Ok(nwrit),
                Err(e) => return Err(e),
            }
        }
    }
}

impl<T> WriteExt for T where T: Write {}

