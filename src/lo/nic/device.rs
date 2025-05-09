use std::ops::Deref;
use std::ptr::NonNull;
use std::{io, slice};

use crate::bindings::*;
use crate::lo::context::IbvContext;

/// Wrapper for `*mut ibv_device`.
///
/// # Resource Ownership
///
/// - Does not own the device descriptor.
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct IbvDevice(NonNull<ibv_device>);

impl IbvDevice {
    /// Get the name of this device.
    pub(crate) fn name(&self) -> io::Result<String> {
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

    /// Open the device to get a context.
    pub(crate) fn open(&self) -> io::Result<IbvContext> {
        // SAFETY: FFI.
        let ctx = unsafe { ibv_open_device(self.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or_else(io::Error::last_os_error)?;
        Ok(IbvContext::from(ctx))
    }
}

impl_ibv_wrapper_traits!(RAW, ibv_device, IbvDevice);

/// Wrapper for `*mut *mut ibv_device`.
///
/// # Resource Ownership
///
/// - Owns the device list, will free it when dropped.
#[repr(transparent)]
pub(super) struct IbvDeviceList(&'static [IbvDevice]);

impl IbvDeviceList {
    /// Get a list of RDMA physical devices.
    pub fn new() -> io::Result<Self> {
        let mut n = 0;

        // SAFETY: FFI.
        let list = unsafe { ibv_get_device_list(&mut n) };
        if list.is_null() {
            return Err(io::Error::last_os_error());
        }

        // SAFETY:
        // - `IbvDevice` is a transparent wrapper of `*mut ibv_device`.
        // - `ibv_get_device_list` returns a pointer to a valid array of non-null `ibv_device` pointers.
        let list = unsafe { slice::from_raw_parts(list as _, n as usize) };
        Ok(Self(list))
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
