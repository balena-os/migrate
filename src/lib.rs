#[macro_use] extern crate failure;
#[cfg(windows)]
pub mod mswin;
// pub mod linux;
// pub mod darwin;
pub mod common;
pub mod mig_error;