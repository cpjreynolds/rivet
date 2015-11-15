#[cfg(all(not(feature = "select"),
              target_os = "linux"))]
mod epoll;
#[cfg(all(not(feature = "select"),
              target_os = "linux"))]
pub use self::epoll::{
    Selector,
    IterFired,
    Fired,
};

#[cfg(all(not(feature = "select"),
          any(target_os = "freebsd",
              target_os = "openbsd",
              target_os = "netbsd",
              target_os = "bitrig",
              target_os = "dragonfly")))]
mod kqueue;
#[cfg(all(not(feature = "select"),
          any(target_os = "freebsd",
              target_os = "openbsd",
              target_os = "netbsd",
              target_os = "bitrig",
              target_os = "dragonfly")))]
pub use self::kqueue::{
    Selector,
    IterFired,
    Fired,
};

#[cfg(any(feature = "select",
          target_os = "macos"))]
mod select;
#[cfg(any(feature = "select",
          target_os = "macos"))]
pub use self::select::{
    Selector,
    IterFired,
    Fired,
};

