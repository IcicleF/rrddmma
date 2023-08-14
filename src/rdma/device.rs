use std::ptr::NonNull;
use std::{ffi, fmt, io, ops};

use crate::sys::*;
use anyhow::Result;

/// An RDMA device.
#[repr(transparent)]
pub(crate) struct Device(NonNull<ibv_device>);

impl Device {
    /// Get the underlying [`ibv_device`] pointer.
    pub(crate) fn as_raw(&self) -> *mut ibv_device {
        self.0.as_ptr()
    }

    /// Get a human-readable name associated with the device.
    pub(crate) fn name(&self) -> String {
        // SAFETY: FFI.
        match unsafe { ibv_get_device_name(self.as_raw()) } {
            // SAFETY: A non-null return value must point to a valid C string.
            // The device name is pure-ASCII and may never fail the UTF-8 check.
            name if !name.is_null() => unsafe { ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            _ => String::from(""),
        }
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Device").field(&self.name()).finish()
    }
}

/// A list of available RDMA devices.
pub(crate) struct DeviceList {
    /// Pointer to the head of the device list.
    list: NonNull<Device>,

    /// List length.
    len: usize,
}

impl DeviceList {
    pub(crate) fn new() -> Result<Self> {
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

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *mut *mut ibv_device {
        self.list.as_ptr() as *mut *mut ibv_device
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[Device] {
        // SAFETY: a non-null device list returned by ibverbs driver is sure
        // to be contiguous, valid, and properly aligned. Also, there is no
        // means to mutate from this type's interfaces.
        unsafe { std::slice::from_raw_parts(self.list.as_ptr(), self.len) }
    }
}

impl Drop for DeviceList {
    fn drop(&mut self) {
        // SAFETY: FFI.
        unsafe { ibv_free_device_list(self.as_ptr()) };
    }
}

impl ops::Deref for DeviceList {
    type Target = [Device];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl fmt::Debug for DeviceList {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <[Device] as fmt::Debug>::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_list() {
        use std::process::Command;

        // Collect device names from ibv_devinfo
        let devinfo = Command::new("ibv_devinfo")
            .output()
            .expect("failed to get device info via ibv_devinfo");
        let devinfo = String::from_utf8(devinfo.stdout).unwrap();
        let mut dev_names = devinfo
            .lines()
            .filter(|line| line.starts_with("hca_id:"))
            .map(|line| line.split_whitespace().nth(1).unwrap())
            .collect::<Vec<_>>();
        dev_names.sort();

        // Evaluate DeviceList against the output of `ibv_devinfo`
        let list = DeviceList::new().unwrap();
        assert_eq!(dev_names.len(), list.len());

        let mut list_names = list.iter().map(|dev| dev.name()).collect::<Vec<_>>();
        list_names.sort();

        assert_eq!(dev_names, list_names);
    }
}
