use std::ptr::NonNull;
use std::{fmt, io, mem};

use super::device::DeviceList;
use super::gid::Gid;

use anyhow;
use rdma_sys::*;

/// RDMA device context.
///
/// This structure owns a valid opened device context (`ibv_context`) and is responsible of closing it
/// when dropped.
///
/// Rather than a pure `ibv_context`, this structure also specifies a device port.
/// To operate on different ports of the same device, it is required to create multiple `Context` objects.
///
/// Because the inner `ibv_context` is owned by this structure, `Context` is `!Send` and cannot be cloned.
/// However, it is `Sync` because access to the same device from multiple threads is guaranteed by the ibverbs
/// userspace driver.
#[allow(dead_code)]
pub struct Context {
    ctx: NonNull<ibv_context>,
    dev_attr: ibv_device_attr,

    port_attr: ibv_port_attr,
    port_num: u8,
    gid: Gid,
    gid_index: u8,
}

unsafe impl Sync for Context {}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("ctx", &self.ctx)
            .field("gid", &self.gid)
            .finish()
    }
}

impl Context {
    /// Open a device and query the related attributes (device and port).
    ///
    /// If `dev_name` is `None`, the first device found is used. Otherwise, the device with the given name is used.
    pub fn open(dev_name: Option<&str>, port_num: u8, gid_index: u8) -> anyhow::Result<Self> {
        let dev_list = DeviceList::new()?;
        let dev = dev_list
            .iter()
            .find(|dev| dev_name.map_or(true, |name| dev.name() == name))
            .ok_or_else(|| anyhow::anyhow!("device not found"))?;

        let ctx = NonNull::new(unsafe { ibv_open_device(dev.as_ptr()) })
            .ok_or_else(|| anyhow::anyhow!(io::Error::last_os_error()))?;
        drop(dev_list);

        let dev_attr = {
            let mut dev_attr = unsafe { mem::zeroed() };
            let ret = unsafe { ibv_query_device(ctx.as_ptr(), &mut dev_attr) };
            if ret != 0 {
                return Err(anyhow::anyhow!(io::Error::last_os_error()));
            }
            dev_attr
        };
        if port_num > dev_attr.phys_port_cnt {
            return Err(anyhow::anyhow!("invalid port number {}", port_num));
        }

        let port_attr = {
            let mut port_attr = unsafe { mem::zeroed() };
            let ret = unsafe { ___ibv_query_port(ctx.as_ptr(), port_num, &mut port_attr) };
            if ret != 0 {
                return Err(anyhow::anyhow!(io::Error::last_os_error()));
            }
            port_attr
        };
        if port_attr.state != ibv_port_state::IBV_PORT_ACTIVE {
            return Err(anyhow::anyhow!("port {} is not active", port_num));
        }

        let gid = {
            let mut gid = unsafe { mem::zeroed() };
            let ret = unsafe { ibv_query_gid(ctx.as_ptr(), port_num, gid_index as i32, &mut gid) };
            if ret != 0 {
                return Err(anyhow::anyhow!(io::Error::last_os_error()));
            }
            Gid::from(gid)
        };

        Ok(Self {
            ctx,
            dev_attr,
            port_attr,
            port_num,
            gid,
            gid_index,
        })
    }

    /// Get the underlying `ibv_context` pointer.
    #[inline]
    pub fn as_ptr(&self) -> *mut ibv_context {
        self.ctx.as_ptr()
    }

    /// Get the LID of the specified port.
    #[inline]
    pub fn lid(&self) -> u16 {
        self.port_attr.lid
    }

    /// Get the port number passed by the user when opening this context.
    #[inline]
    pub fn port_num(&self) -> u8 {
        self.port_num
    }

    /// Get the specified GID of the opened device.
    #[inline]
    pub fn gid(&self) -> Gid {
        self.gid.clone()
    }

    // Get the GID index passed by the user when opening this context.
    #[inline]
    pub fn gid_index(&self) -> u8 {
        self.gid_index
    }

    /// Get the path MTU of the specified port.
    ///
    /// **NOTE:** the return value is an integer and should be viewed as a value of the `ibv_mtu` enum.
    #[inline]
    pub fn active_mtu(&self) -> u32 {
        self.port_attr.active_mtu
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { ibv_close_device(self.ctx.as_ptr()) };
    }
}