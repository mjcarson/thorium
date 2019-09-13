// The Thorium Reactor is only supported on Linux and Windows
#![feature(btree_extract_if)]
#![cfg(any(target_os = "linux", target_os = "windows"))]

mod keys;
mod launchers;
mod reactor;
mod tasks;

pub use reactor::Reactor;
