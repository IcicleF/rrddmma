use std::fs::File;
use std::io::{self, Read};
use std::iter::IntoIterator;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::path::Path;
use std::ptr::NonNull;
use std::slice;

use crate::bindings::*;
use crate::rdma::context::IbvContext;

/// Wrapper for `*mut ibv_device`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct IbvDevice(NonNull<ibv_device>);

impl IbvDevice {
    /// Get the name of this device.
    pub fn name(&self) -> io::Result<String> {
        // SAFETY: FFI.
        let name = unsafe { ibv_get_device_name(self.as_ptr()) };
        if name.is_null() {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: `ibv_get_device_name` returns a pointer to a valid C string.
        let name = unsafe { std::ffi::CStr::from_ptr(name) };
        Ok(name
            .to_str()
            .expect("`ibv_get_device_name` returned non-UTF8-compliant string")
            .to_owned())
    }

    /// Get the NUMA node of this device.
    pub fn numa_node(&self) -> io::Result<u8> {
        let name = self.name()?;

        // Read NUMA node information.
        let path = Path::new("/sys/class/infiniband")
            .join(name)
            .join("device/numa_node");
        let mut buf = String::new();
        File::open(path)?.read_to_string(&mut buf)?;

        Ok(buf
            .trim()
            .parse::<u8>()
            .expect("invalid NUMA node information in sysfs"))
    }

    /// Open the device to get a context.
    pub fn open(self) -> io::Result<IbvContext> {
        // SAFETY: FFI.
        let ctx = unsafe { ibv_open_device(self.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or_else(io::Error::last_os_error)?;
        Ok(IbvContext::from(ctx))
    }
}

impl_ibv_wrapper_traits!(ibv_device, IbvDevice);

/// Wrapper for `*mut *mut ibv_device`.
#[repr(transparent)]
pub(super) struct IbvDeviceList(ManuallyDrop<Box<[IbvDevice]>>);

impl IbvDeviceList {
    /// Get a list of RDMA physical devices.
    pub fn new() -> io::Result<Self> {
        let mut n = 0i32;

        // SAFETY: FFI.
        let list = unsafe { ibv_get_device_list(&mut n) };
        if list.is_null() {
            return Err(io::Error::last_os_error());
        }

        // SAFETY:
        // - `IbvDevice` is a transparent wrapper of `*mut ibv_device`.
        // - `ibv_get_device_list` returns a pointer to a valid array of non-null `ibv_device` pointers.
        let list = unsafe { Box::from_raw(slice::from_raw_parts_mut(list as _, n as usize)) };
        Ok(Self(ManuallyDrop::new(list)))
    }
}

impl Deref for IbvDeviceList {
    type Target = [IbvDevice];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a IbvDeviceList {
    type Item = &'a IbvDevice;
    type IntoIter = slice::Iter<'a, IbvDevice>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Drop for IbvDeviceList {
    fn drop(&mut self) {
        // SAFETY: FFI.
        unsafe { ibv_free_device_list(self.0.as_ptr() as _) };
    }
}
