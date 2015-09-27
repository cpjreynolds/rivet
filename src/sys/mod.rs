mod epoll;
mod select;

#[cfg(target_os = "linux")]
pub use self::epoll::{
    Selector,
    Fired,
};

#[cfg(target_os = "macos")]
pub use self::select::{
    Selector,
    Fired,
};

