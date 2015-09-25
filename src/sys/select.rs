use std::mem;
use std::ptr;

mod ffi {
    use std::mem;
    use std::ptr;
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

        // Zeroes an `fd_set`.
        pub fn zero(&mut self) {
            unsafe {
                ptr::write_bytes(self, 0, 1);
            }
        }

        pub fn set(&mut self, fd: RawFd) {
            self.fds_bits[get_elt(fd)] |= get_mask(fd);
        }

        pub fn unset(&mut self, fd: RawFd) {
            self.fds_bits[get_elt(fd)] &= !get_mask(fd);
        }

        pub fn is_set(&mut self, fd: RawFd) -> bool {
            (self.fds_bits[get_elt(fd)] & get_mask(fd)) != 0
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

struct FileSet(ffi::fd_set);

impl FileSet {
    pub fn new() -> FileSet {
        FileSet(ffi::fd_set::new())
    }
}
