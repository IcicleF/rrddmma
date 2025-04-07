//! Bindings of libibverbs C interfaces.

#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(deref_nullptr)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(unused)]

mod common;

#[cfg(feature = "legacy")]
mod legacy;

#[cfg(feature = "exp")]
mod exp;

#[cfg(not(feature = "legacy"))]
mod rdma_core;

mod private {
    use libc::*;
    include!(concat!(env!("OUT_DIR"), "/verbs_bindings.rs"));

    #[cfg(feature = "legacy")]
    pub(crate) use super::legacy::*;

    #[cfg(feature = "exp")]
    pub(crate) use super::exp::*;

    #[cfg(not(feature = "legacy"))]
    pub(crate) use super::rdma_core::*;
}

pub(crate) use self::private::*;
