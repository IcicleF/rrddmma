use std::fmt;
use std::net::Ipv6Addr;

use serde::{Deserialize, Serialize};

use crate::bindings::*;

/// An 128-bit identifier used to identify a port on a network adapter, a port
/// on a router, or a multicast group.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Gid(pub ibv_gid);

unsafe impl Send for Gid {}
unsafe impl Sync for Gid {}

impl fmt::Debug for Gid {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let gid = Ipv6Addr::from(*self);
        f.debug_tuple("Gid").field(&gid.to_string()).finish()
    }
}

impl PartialEq for Gid {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: byte-level reinterpretation of POD union.
        unsafe { self.0.raw == other.0.raw }
    }
}

impl Eq for Gid {}

impl From<ibv_gid> for Gid {
    #[inline]
    fn from(gid: ibv_gid) -> Self {
        Self(gid)
    }
}

impl From<Gid> for ibv_gid {
    #[inline]
    fn from(gid: Gid) -> Self {
        gid.0
    }
}

impl From<Ipv6Addr> for Gid {
    #[inline]
    fn from(addr: Ipv6Addr) -> Self {
        Self(ibv_gid { raw: addr.octets() })
    }
}

impl From<Gid> for Ipv6Addr {
    #[inline]
    fn from(gid: Gid) -> Self {
        // SAFETY: byte-level reinterpretation of POD union.
        Ipv6Addr::from(unsafe { gid.0.raw })
    }
}

impl From<[u8; 16]> for Gid {
    #[inline]
    fn from(raw: [u8; 16]) -> Self {
        Self(ibv_gid { raw })
    }
}

impl From<Gid> for [u8; 16] {
    #[inline]
    fn from(gid: Gid) -> Self {
        // SAFETY: byte-level reinterpretation of POD union.
        unsafe { gid.0.raw }
    }
}

impl Serialize for Gid {
    #[inline]
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        <[u8; 16] as Serialize>::serialize(&<[u8; 16]>::from(*self), serializer)
    }
}

impl<'de> Deserialize<'de> for Gid {
    #[inline]
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <[u8; 16] as Deserialize<'de>>::deserialize(deserializer).map(Self::from)
    }
}
