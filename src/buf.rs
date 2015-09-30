use std::collections::{
    VecDeque,
};
use std::io::{
    Result,
};

use super::io::{
    WriteExt,
    ReadExt,
};

const BUFSIZE: usize = 1024 * 4;

pub trait Buffer {
    fn write_into<W: ?Sized>(&mut self, w: &mut W) -> Result<usize>
        where W: WriteExt;

    fn read_from<R: ?Sized>(&mut self, r: &mut R) -> Result<usize>
        where R: ReadExt;
}

impl Buffer for VecDeque<u8> {
    fn write_into<W: ?Sized>(&mut self, w: &mut W) -> Result<usize>
        where W: WriteExt
    {
        let nwrit = {
            let (buf1, buf2) = self.as_slices();

            let nwrit1 = try!(w.write_nb(buf1));

            let nwrit2 = if nwrit1 == buf1.len() {
                try!(w.write_nb(buf2))
            } else {
                0
            };

            nwrit1 + nwrit2
        };

        for _ in 0..nwrit {
            self.pop_front();
        }
        Ok(nwrit)
    }


    fn read_from<R: ?Sized>(&mut self, r: &mut R) -> Result<usize>
        where R: ReadExt
    {
        let start_len = self.len();
        let mut len = start_len;
        let mut new_write_size = 16;
        let ret;

        loop {
            if len == self.len() {
                if new_write_size < BUFSIZE {
                    new_write_size *= 2;
                }
                self.resize(len + new_write_size, 0);
            }

            let (buf1, buf2) = self.as_mut_slices();

            let buf = {
                if buf1.len() > len {
                    &mut buf1[len..]
                } else {
                    &mut buf2[(len - buf1.len())..]
                }
            };

            match r.read_nb(buf) {
                Ok(0) => {
                    ret = Ok(len - start_len);
                    break;
                }
                Ok(n) => len += n,
                Err(e) => {
                    ret = Err(e);
                    break;
                },
            }
        }

        self.truncate(len);
        ret
    }
}

impl Buffer for Vec<u8> {
    fn write_into<W: ?Sized>(&mut self, w: &mut W) -> Result<usize>
        where W: WriteExt
    {
        let nwrit = try!(w.write_nb(self));
        self.drain(..nwrit);

        Ok(nwrit)
    }

    fn read_from<R: ?Sized>(&mut self, r: &mut R) -> Result<usize>
        where R: ReadExt
    {
        let start_len = self.len();
        let mut len = start_len;
        let mut new_write_size = 16;
        let ret;

        loop {
            if len == self.len() {
                if new_write_size < BUFSIZE {
                    new_write_size *= 2;
                }
                self.resize(len + new_write_size, 0);
            }

            match r.read_nb(&mut self[len..]) {
                Ok(0) => {
                    ret = Ok(len - start_len);
                    break;
                }
                Ok(n) => len += n,
                Err(e) => {
                    ret = Err(e);
                    break;
                },
            }
        }

        self.truncate(len);
        ret
    }
}

