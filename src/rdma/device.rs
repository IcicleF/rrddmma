use std::ptr::NonNull;
use std::{ffi, fmt, io, ops};

use anyhow;
use rdma_sys::*;

/// An RDMA device.
#[repr(transparent)]
pub(crate) struct Device(NonNull<ibv_device>);

impl Device {
    pub(crate) fn as_ptr(&self) -> *mut ibv_device {
        self.0.as_ptr()
    }

    pub(crate) fn name(&self) -> &str {
        unsafe { ffi::CStr::from_ptr(ibv_get_device_name(self.as_ptr())) }
            .to_str()
            .unwrap()
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Device").field(&self.name()).finish()
    }
}

/// A list of available RDMA devices.
pub(crate) struct DeviceList {
    /// Pointer to the head of the device list
    list: NonNull<Device>,

    /// List length
    len: usize,
}

impl DeviceList {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let mut num_devices = 0;
        let list = unsafe { ibv_get_device_list(&mut num_devices) };
        if list.is_null() {
            return Err(anyhow::anyhow!(io::Error::last_os_error()));
        }

        Ok(Self {
            list: unsafe { NonNull::new_unchecked(list.cast()) },
            len: num_devices as usize,
        })
    }

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
        unsafe { std::slice::from_raw_parts(self.list.as_ptr(), self.len) }
    }
}

impl Drop for DeviceList {
    fn drop(&mut self) {
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
