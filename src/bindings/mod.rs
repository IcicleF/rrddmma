#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(deref_nullptr)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(unused)]

use libc::{pthread_cond_t, pthread_mutex_t};

include!(concat!(env!("OUT_DIR"), "/verbs_bindings.rs"));

#[cfg(mlnx4)]
mod mlnx4;

#[cfg(mlnx4)]
pub use mlnx4::*;
