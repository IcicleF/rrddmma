#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(deref_nullptr)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(unused)]

mod common;

#[cfg(mlnx4)]
mod mlnx4;

#[cfg(mlnx5)]
mod mlnx5;

mod private {
    use libc::*;
    include!(concat!(env!("OUT_DIR"), "/verbs_bindings.rs"));

    #[cfg(mlnx4)]
    pub use super::mlnx4::*;

    #[cfg(mlnx5)]
    pub use super::mlnx5::*;
}

pub(crate) use private::*;
