use std::fmt::Display;
use std::{hint, io, mem};

use thiserror::Error;

use crate::bindings::*;
use crate::rdma::context::IbvContext;
use crate::rdma::gid::*;

/// Physical port information.
#[derive(Clone)]
pub struct Port {
    /// Index of this port.
    num: u8,

    /// Port attributes.
    attr: ibv_port_attr,

    /// GIDs of this port.
    gids: Vec<GidTyped>,
}

unsafe impl Send for Port {}
unsafe impl Sync for Port {}

/// Port query error type.
#[derive(Debug, Error)]
pub enum PortQueryError {
    /// `libibverbs` interfaces returned an error when querying the port.
    #[error("ibv_query_port error")]
    IoError(#[from] io::Error),

    /// Failed to query GID attributes.
    #[error("GID query error")]
    GidQueryError(#[from] GidQueryError),
}

impl Port {
    /// Initialize information of an RDMA device's physical port.
    pub(crate) fn new(ctx: IbvContext, num: u8) -> Result<Self, PortQueryError> {
        let attr = {
            // SAFETY: POD type.
            let mut attr = unsafe { mem::zeroed() };

            // SAFETY: FFI.
            let ret = unsafe { ___ibv_query_port(ctx.as_ptr(), num, &mut attr) };
            if ret != 0 {
                eprintln!("query port error: {}", ret);
                return Err(io::Error::from_raw_os_error(ret).into());
            }
            attr
        };

        let num_gids = attr.gid_tbl_len;
        let mut gids = Vec::with_capacity(num_gids as usize);
        for i in 0..num_gids {
            match GidTyped::query(ctx, num, &attr, i as _) {
                Ok(gid) => gids.push(gid),
                Err(GidQueryError::AttributeQueryError) => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(Self { num, attr, gids })
    }

    /// Get the index of this port.
    #[inline]
    pub fn num(&self) -> u8 {
        self.num
    }

    /// Get the attributes of this port.
    /// Prefer other getters if possible.
    #[inline]
    pub fn attr(&self) -> &ibv_port_attr {
        &self.attr
    }

    /// Get the state of this port.
    #[inline]
    pub fn state(&self) -> PortState {
        match self.attr.state {
            ibv_port_state::IBV_PORT_DOWN => PortState::Down,
            ibv_port_state::IBV_PORT_INIT => PortState::Init,
            ibv_port_state::IBV_PORT_ARMED => PortState::Armed,
            ibv_port_state::IBV_PORT_ACTIVE => PortState::Active,
            ibv_port_state::IBV_PORT_ACTIVE_DEFER => PortState::ActiveDefer,

            // SAFETY: enum constraints of `libibverbs`.
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    /// Get the LID of this port.
    #[inline]
    pub fn lid(&self) -> u16 {
        self.attr.lid
    }

    /// Get the link layer protocol of this port.
    #[inline]
    pub fn link_layer(&self) -> PortLinkLayer {
        match self.attr.link_layer as i32 {
            IBV_LINK_LAYER_UNSPECIFIED | IBV_LINK_LAYER_INFINIBAND => PortLinkLayer::Infiniband,
            IBV_LINK_LAYER_ETHERNET => PortLinkLayer::Ethernet,

            // SAFETY: enum constraints of `libibverbs`.
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    /// Get the active MTU of this port.
    #[inline]
    pub fn mtu(&self) -> PortMtu {
        match self.attr.active_mtu {
            ibv_mtu::IBV_MTU_256 => PortMtu::Mtu256,
            ibv_mtu::IBV_MTU_512 => PortMtu::Mtu512,
            ibv_mtu::IBV_MTU_1024 => PortMtu::Mtu1024,
            ibv_mtu::IBV_MTU_2048 => PortMtu::Mtu2048,
            ibv_mtu::IBV_MTU_4096 => PortMtu::Mtu4096,

            // SAFETY: enum constraints of `libibverbs`.
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    /// Get the active speed of this port in Gbps.
    #[inline]
    pub fn speed(&self) -> PortSpeed {
        let width: u32 = match self.attr.active_width {
            1 => 1,
            2 => 4,
            4 => 8,
            8 => 12,

            // SAFETY: enum constraints of `libibverbs`.
            _ => unsafe { hint::unreachable_unchecked() },
        };

        // Speed times 10 to avoid floating point numbers, which are `!Eq`.
        let speed10x: u32 = match self.attr.active_speed {
            1 => 25,
            2 => 50,
            4 | 8 => 100,
            16 => 140,
            32 => 250,
            64 => 500,

            // SAFETY: enum constraints of `libibverbs`.
            _ => unsafe { hint::unreachable_unchecked() },
        };
        PortSpeed(width * speed10x)
    }

    /// Get the GIDs of this port.
    pub fn gids(&self) -> &[GidTyped] {
        &self.gids
    }

    /// Get the most recommended GID of this port.
    /// Using this GID should generally work well.
    /// - Infiniband is preferred over RoCEv2, then RoCEv1.
    /// - IPv4 is preferred over IPv6.
    ///
    /// Return the recommended GID and its index.
    ///
    /// **NOTE:** It is possible that the returned GID is not exactly what you
    /// want. In that case, you may use the `.gids()` method to get all GIDs
    /// and choose one yourself.
    ///
    /// # Panics
    ///
    /// Panics if no GIDs are found.
    pub fn recommended_gid(&self) -> (GidTyped, u8) {
        use std::cmp::Ordering;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct GidIndexed {
            idx: u8,
            gid: GidTyped,
        }

        impl Ord for GidIndexed {
            #[inline]
            fn cmp(&self, other: &Self) -> Ordering {
                use std::net::Ipv6Addr;

                if self.gid.ty != other.gid.ty {
                    return self.gid.ty.cmp(&other.gid.ty);
                }
                match (
                    Ipv6Addr::from(self.gid).to_ipv4(),
                    Ipv6Addr::from(other.gid).to_ipv4(),
                ) {
                    (Some(_), None) => Ordering::Greater,
                    (None, Some(_)) => Ordering::Less,

                    // Usually, GIDs with larger indexes are better.
                    _ => self.idx.cmp(&other.idx),
                }
            }
        }

        impl PartialOrd for GidIndexed {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut gids = self
            .gids
            .iter()
            .enumerate()
            .map(|(idx, &gid)| GidIndexed { idx: idx as _, gid })
            .collect::<Vec<_>>();

        gids.sort_unstable();
        gids.last().map(|g| (g.gid, g.idx)).expect("no GIDs found")
    }
}

/// Port state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    /// Logical link is down. The physical link of the port isn't up.
    /// All TX packets will be dropped, and RX is impossible.
    Down = ibv_port_state::IBV_PORT_DOWN as _,

    /// Logical link is Initializing.
    /// The physical link of the port is up, but the SM haven't yet configured the logical link.
    /// TX/RX SM packets, but other packets will be dropped.
    Init = ibv_port_state::IBV_PORT_INIT as _,

    /// Logical link is Armed.
    /// The physical link of the port is up, but the SM haven't yet fully configured the logical link.
    /// RX packets and TX SM packets, but other TX packets will be dropped.
    Armed = ibv_port_state::IBV_PORT_ARMED as _,

    /// Logical link is Active.
    /// TX/RX all packets.
    Active = ibv_port_state::IBV_PORT_ACTIVE as _,

    /// Logical link is Active Deferred.
    /// The physical link of the port is suffering from a failure.
    /// Return to [`PortState::Active`] if the error recovers within a timeout, or [`PortState::Down`] otherwise.
    ActiveDefer = ibv_port_state::IBV_PORT_ACTIVE_DEFER as _,
}

/// Port link layer protocol type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortLinkLayer {
    /// Infiniband.
    Infiniband,

    /// Ethernet.
    Ethernet,
}

/// Port MTU size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortMtu {
    /// 256 bytes.
    Mtu256 = ibv_mtu::IBV_MTU_256 as _,

    /// 512 bytes.
    Mtu512 = ibv_mtu::IBV_MTU_512 as _,

    /// 1024 bytes.
    Mtu1024 = ibv_mtu::IBV_MTU_1024 as _,

    /// 2048 bytes.
    Mtu2048 = ibv_mtu::IBV_MTU_2048 as _,

    /// 4096 bytes.
    Mtu4096 = ibv_mtu::IBV_MTU_4096 as _,
}

impl Display for PortMtu {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}B",
            match self {
                Self::Mtu256 => 256,
                Self::Mtu512 => 512,
                Self::Mtu1024 => 1024,
                Self::Mtu2048 => 2048,
                Self::Mtu4096 => 4096,
            }
        )
    }
}

/// Port speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PortSpeed(pub(crate) u32);

impl PortSpeed {
    pub const MAX_GBPS: f32 = 600.0;

    /// Get the speed in Gbps.
    #[inline]
    pub fn gbps(&self) -> f32 {
        self.0 as f32 / 10.0
    }
}

impl Display for PortSpeed {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}Gbps", self.gbps())
    }
}
