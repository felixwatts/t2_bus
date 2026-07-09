#![doc = include_str!("../README.md")]

pub mod prelude;
pub mod transport;

pub(crate) mod client;
pub(crate) mod directory;
pub(crate) mod err;
pub(crate) mod protocol;
pub(crate) mod server;
pub(crate) mod topic;

mod debug;
mod stopper;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

#[cfg(test)]
pub mod test;
