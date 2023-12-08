use std::ops::Index;
use std::ptr::NonNull;
use std::{ffi, fmt, io};

use anyhow::{Context, Result};

use crate::bindings::*;
use crate::utils::interop::from_c_err;

/// An RDMA device.
///
/// This type transparently wraps a pointer to [`ibv_device`].
#[repr(transparent)]
pub struct Device(NonNull<ibv_device>);

impl Device {
    /// Get the underlying [`ibv_device`] pointer.
    pub fn as_raw(&self) -> *mut ibv_device {
        self.0.as_ptr()
    }

    /// Get a human-readable name associated with the device.
    pub fn name(&self) -> String {
        // SAFETY: FFI.
        match unsafe { ibv_get_device_name(self.as_raw()) } {
            // SAFETY: A non-null return value must point to a valid C string.
            // The device name is pure-ASCII and may never fail the UTF-8 check.
            name if !name.is_null() => unsafe { ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            _ => "(unknown)".to_owned(),
        }
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Device").field(&self.name()).finish()
    }
}

/// A list of available RDMA devices.
pub struct DeviceList {
    /// Pointer to the head of the device list.
    list: NonNull<Device>,

    /// List length.
    len: usize,
}

impl DeviceList {
    pub fn new() -> Result<Self> {
        let mut num_devices = 0;

        // SAFETY: FFI.
        let list = unsafe { ibv_get_device_list(&mut num_devices) };
        if !list.is_null() {
            Ok(Self {
                // SAFETY: `NonNull<T>` is transparent over `*mut T` and `Device`
                // is transparent over `NonNull<ibv_device>`. Also, `list` is sure
                // to be non-null at this point.
                list: unsafe { NonNull::new_unchecked(list.cast()) },
                len: num_devices as usize,
            })
        } else {
            Err(anyhow::anyhow!(io::Error::last_os_error()))
        }
    }

    /// Get the number of devices in the list.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the device list is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a pointer to the head of the device list.
    #[inline]
    pub fn as_raw(&self) -> *mut *mut ibv_device {
        self.list.as_ptr() as *mut *mut ibv_device
    }

    /// View the device list as a slice.
    ///
    /// # Safety
    ///
    /// - The very same requirements as [`std::slice::from_raw_parts`] must be met.
    #[inline]
    pub unsafe fn as_slice(&self) -> &[Device] {
        // SAFETY: a non-null device list returned by ibverbs driver is sure
        // to be contiguous, valid, and properly aligned. Also, there is no
        // means to mutate from this type's interfaces.
        unsafe { std::slice::from_raw_parts(self.list.as_ptr(), self.len) }
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = Result<DevicePort>> + '_ {
        DevicePortIter::new(self)
    }
}

impl Drop for DeviceList {
    fn drop(&mut self) {
        // SAFETY: FFI.
        unsafe { ibv_free_device_list(self.as_raw()) };
    }
}

impl Index<usize> for DeviceList {
    type Output = Device;

    fn index(&self, idx: usize) -> &Self::Output {
        if idx >= self.len {
            panic!("index {} out of device list length bound {}", idx, self.len);
        }
        // SAFETY: bounds checked
        unsafe { self.as_slice().get_unchecked(idx) }
    }
}

/// A device-port pair.
pub struct DevicePort(pub *mut ibv_context, pub u8);

/// An iterator over a device list that produces all active device-port pairs.
struct DevicePortIter<'a> {
    list: &'a DeviceList,
    device_idx: isize,
    device: *mut ibv_context,
    port_num: u8,
    port_idx: u8,
}

impl<'a> DevicePortIter<'a> {
    fn new(list: &'a DeviceList) -> Self {
        Self {
            list,
            device_idx: -1,
            device: std::ptr::null_mut(),
            port_num: 0,
            port_idx: 0,
        }
    }
}

impl Iterator for DevicePortIter<'_> {
    type Item = Result<DevicePort>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Move to the next device
            if self.port_idx >= self.port_num {
                // Close current device
                if !self.device.is_null() {
                    // SAFETY: FFI.
                    unsafe { ibv_close_device(self.device) };
                }

                self.device_idx += 1;
                self.port_idx = 1;

                if self.device_idx as usize >= self.list.len() {
                    return None;
                }
                self.device = {
                    let device = &self.list[self.device_idx as usize];
                    // SAFETY: FFI.
                    let ctx = unsafe { ibv_open_device(device.as_raw()) };
                    if ctx.is_null() {
                        return Some(Err(anyhow::anyhow!(io::Error::last_os_error())));
                    }
                    ctx
                };

                let dev_attr = match query_device(self.device) {
                    Ok(dev_attr) => dev_attr,
                    Err(e) => return Some(Err(e)),
                };
                self.port_num = dev_attr.phys_port_cnt;
            } else {
                self.port_idx += 1;
            }

            // Query port attributes
            let port_attr = match query_port(self.device, self.port_idx) {
                Ok(port_attr) => port_attr,
                Err(e) => return Some(Err(e)),
            };

            // Skip inactive ports
            if port_attr.state != ibv_port_state::IBV_PORT_ACTIVE
                && port_attr.state != ibv_port_state::IBV_PORT_ACTIVE_DEFER
            {
                continue;
            }

            return Some(Ok(DevicePort(self.device, self.port_idx)));
        }
    }
}

/// Query device attributes.
pub fn query_device(ctx: *mut ibv_context) -> Result<ibv_device_attr> {
    // SAFETY: POD type.
    let mut dev_attr = unsafe { std::mem::zeroed() };
    // SAFETY: FFI.
    let ret = unsafe { ibv_query_device(ctx, &mut dev_attr) };
    if ret != 0 {
        from_c_err(ret).with_context(|| "failed to query device attributes")
    } else {
        Ok(dev_attr)
    }
}

/// Query port attributes.
pub fn query_port(ctx: *mut ibv_context, port_num: u8) -> Result<ibv_port_attr> {
    // SAFETY: POD type.
    let mut port_attr = unsafe { std::mem::zeroed() };
    // SAFETY: FFI.
    let ret = unsafe { ___ibv_query_port(ctx, port_num, &mut port_attr) };
    if ret != 0 {
        from_c_err(ret).with_context(|| "failed to query port attributes")
    } else {
        Ok(port_attr)
    }
}
