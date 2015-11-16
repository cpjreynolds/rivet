use std::ffi::{
    CString,
    CStr,
};
use std::os::unix::io::{
    RawFd,
    AsRawFd,
};
use std::io::{
    Result,
    Error,
    ErrorKind,
};
use std::fmt;
use std::ptr;

use libc;
use rand::{
    self,
    Rng,
};

// Returns `cap` rounded up to a multiple of the system's page size.
pub fn page_aligned(cap: usize) -> usize {
    let cap = if cap == 0 { 1 } else { cap };
    let pagesize = get_page_size();
    let alloc = cap + (pagesize - 1);
    alloc - (alloc & (pagesize - 1))
}

pub fn get_page_size() -> usize {
    unsafe {
        libc::sysconf(libc::_SC_PAGESIZE) as usize
    }
}

mod flags {
    #![allow(dead_code)]

    use libc;

    bitflags! {
        flags Protection: libc::c_int {
            const PROT_NONE = libc::PROT_NONE,
            const PROT_READ = libc::PROT_READ,
            const PROT_WRITE = libc::PROT_WRITE,
        }
    }

    bitflags! {
        flags MapFlags: libc::c_int {
            const MAP_PRIVATE = libc::MAP_PRIVATE,
            const MAP_SHARED = libc::MAP_SHARED,
            const MAP_ANONYMOUS = libc::MAP_ANONYMOUS,
            const MAP_FIXED = libc::MAP_FIXED,
        }
    }
}

pub use self::flags::{
    Protection,
    PROT_NONE,
    PROT_READ,
    PROT_WRITE,
    MapFlags,
    MAP_PRIVATE,
    MAP_SHARED,
    MAP_ANONYMOUS,
    MAP_FIXED,
};

pub struct Mapping {
    ptr: *mut u8,
    len: usize,
}

impl Mapping {
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl fmt::Debug for Mapping {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Mapping")
            .field("ptr", &self.ptr)
            .field("len", &self.len)
            .finish()
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        let _ = unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len as libc::size_t)
        };
    }
}

pub struct MapBuilder {
    prot: Protection,
    flags: MapFlags,
    len: usize,
    addr: *mut u8,
    fd: RawFd,
}

impl MapBuilder {
    pub fn new() -> MapBuilder {
        MapBuilder {
            prot: PROT_NONE,
            flags: MapFlags::empty(),
            len: 0,
            addr: ptr::null_mut(),
            fd: -1,
        }
    }

    pub fn prot(&mut self, prot: Protection) -> &mut Self {
        self.prot = prot;
        self
    }

    pub fn flags(&mut self, flags: MapFlags) -> &mut Self {
        self.flags = flags;
        self
    }

    pub fn len(&mut self, len: usize) -> &mut Self {
        self.len = len;
        self
    }

    pub fn addr(&mut self, addr: *mut u8) -> &mut Self {
        self.addr = addr;
        self
    }

    pub fn fd(&mut self, fd: RawFd) -> &mut Self {
        self.fd = fd;
        self
    }

    pub fn create(&mut self) -> Result<Mapping> {
        let ptr = try!(mmap(self.addr, self.len,
                             self.prot.bits(), self.flags.bits(),
                             self.fd, 0));

        Ok(Mapping {
            ptr: ptr,
            len: self.len,
        })
    }
}

pub struct Shm {
    name: CString,
    fd: RawFd,
}

impl Shm {
    pub fn new() -> Result<Shm> {
        const ATTEMPTS: usize = 1 << 12; // 4096.
        const PREFIX: &'static str = "/ring-";
        const POSTFIX_LEN: usize = 12; // (26 * 26 * 10) * 12 = 81120.

        for _ in 0..ATTEMPTS {
            // Generate a random name.
            let name = PREFIX.chars()
                .chain(rand::thread_rng().gen_ascii_chars().take(POSTFIX_LEN))
                .collect::<String>();
            let name = CString::new(name).unwrap();
            // read/write permissions, error if already exists.
            let flags = libc::O_RDWR | libc::O_CREAT | libc::O_EXCL;
            let mode = libc::S_IWUSR | libc::S_IRUSR;

            let res = shm_open(&name, flags, mode);
            match res {
                Ok(fd) => {
                    // Success, unlink and return.
                    let shm = Shm {
                        name: name,
                        fd: fd,
                    };
                    // Shm object will be destroyed after last fd is closed, which occurs on drop.
                    try!(shm_unlink(&shm.name));
                    return Ok(shm);
                },
                Err(ref err) if err.kind() == ErrorKind::AlreadyExists => {
                    // Name already exists, retry.
                    continue;
                },
                Err(err) => {
                    // Some other error, return it.
                    return Err(err);
                },
            }
        }
        Err(Error::new(ErrorKind::Other, "exceeded maximum number of retries"))
    }

    pub fn set_len(&mut self, size: usize) -> Result<()> {
        ftruncate(self.fd, size as libc::off_t)
    }
}

impl AsRawFd for Shm {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for Shm {
    fn drop(&mut self) {
        let _ = unsafe {
            libc::close(self.fd)
        };
    }
}


fn mmap(addr: *mut u8, len: usize, prot: libc::c_int,
        flags: libc::c_int, fd: RawFd, offset: libc::off_t) -> Result<*mut u8>
{
    let res = unsafe {
        libc::mmap(addr as *mut libc::c_void, len as libc::size_t, prot, flags, fd, offset)
    };

    if res == libc::MAP_FAILED {
        Err(Error::last_os_error())
    } else {
        Ok(res as *mut u8)
    }
}

fn shm_open(name: &CStr, flags: libc::c_int, mode: libc::mode_t) -> Result<RawFd> {
    let res = unsafe {
        libc::shm_open(name.as_ptr(), flags, mode)
    };
    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(res)
    }
}

fn shm_unlink(name: &CStr) -> Result<()> {
    let res = unsafe {
        libc::shm_unlink(name.as_ptr())
    };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

fn ftruncate(fd: RawFd, len: libc::off_t) -> Result<()> {
    let res = unsafe {
        libc::ftruncate(fd, len)
    };

    if res == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

