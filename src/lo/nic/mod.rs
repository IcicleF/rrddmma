//! RDMA hardware resource discovery.

mod device;
mod port;

use std::io::{self, Error as IoError};

pub(crate) use self::device::*;
pub use self::port::*;
use super::context::*;

/// NIC probe result type.
/// Contains an opened RDMA device and a set of port metadata.
pub struct Nic {
    /// Device context.
    pub context: Context,

    /// Port metadata.
    pub ports: Vec<Port>,
}

impl Nic {
    /// Open a specified NIC.
    #[inline]
    pub fn open(dev_name: impl AsRef<str>) -> io::Result<Self> {
        let dev_name = dev_name.as_ref();
        let dev_list = IbvDeviceList::new()?;
        for dev in &dev_list {
            let Ok(name) = dev.name() else { continue };
            if name != dev_name {
                continue;
            }
            let ctx = dev.open()?;
            let attr = ctx.query_device()?;
            let ports = (1..=attr.phys_port_cnt)
                .map(|port_num| Port::new(ctx, port_num))
                .collect::<Result<Vec<_>, _>>()?;

            return Ok(Self {
                context: Context::new(ctx, attr),
                ports,
            });
        }
        Err(IoError::new(
            std::io::ErrorKind::NotFound,
            format!("cannot find NIC {}", dev_name),
        ))
    }
}
