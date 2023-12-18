use std::io::{self, Error as IoError, ErrorKind as IoErrorKind};
use std::ops::{Deref, DerefMut};
use std::ptr;

use crate::rdma::mr::*;
use crate::rdma::pd::*;

/// A wrapper around an owned memory area that is registered as an RDMA MR.
/// The memory area is allocated on the heap with `Box<[u8]>` and will be
/// deallocated when this structure is dropped. The MR has full permission.
///
/// **WARNING:** Since Rust disallows self-referencing, this type deceives
/// the borrow checker by storing a `Mr<'static>` inside. Generally, this
/// should not cause any safety problems since the `Mr` references memory
/// allocated on the heap, which isn't movable, and that it will definitely
/// get dropped before the referenced memory.
/// However, you should still use this type with some care.
pub struct RegisteredMem<'a> {
    /// The memory region, dropped first.
    mr: Mr<'a>,

    /// The allocated buffer, dropped after the `Mr`.
    buf: Box<[u8]>,
}

impl<'a> RegisteredMem<'a> {
    /// Allocate memory with the given length and register MR on it.
    pub fn new(pd: &'a Pd<'a>, len: usize) -> io::Result<Self> {
        if len == 0 {
            return Err(IoError::new(
                IoErrorKind::InvalidInput,
                "zero-length memory regions are disallowed",
            ));
        }

        // This should use the global allocator
        let buf = vec![0u8; len].into_boxed_slice();
        Self::new_owned(pd, buf).map_err(|(_, e)| e)
    }

    /// Take ownership of the provided memory region and register MR on it.
    /// On error, the provided buffer will be returned along with the error.
    pub fn new_owned(pd: &'a Pd<'a>, buf: Box<[u8]>) -> Result<Self, (Box<[u8]>, IoError)> {
        if buf.is_empty() {
            return Err((
                buf,
                IoError::new(
                    IoErrorKind::InvalidInput,
                    "zero-length memory regions are disallowed",
                ),
            ));
        }

        // Leak the buffer to get it as a 'a reference.
        let buf: &'a mut [u8] = Box::leak(buf);
        let mr = Mr::reg(pd, buf, Permission::default());

        // Pack the leaked buffer back.
        let buf = unsafe { Box::from_raw(buf) };

        match mr {
            Ok(mr) => Ok(Self { mr, buf }),
            Err(e) => Err((buf, e)),
        }
    }

    /// Allocate memory that shares the same length and content with the provided
    /// slice, and then register MR on it.
    pub fn new_with_content(pd: &'a Pd<'a>, content: &[u8]) -> io::Result<Self> {
        if content.is_empty() {
            return Err(IoError::new(
                IoErrorKind::InvalidInput,
                "zero-length memory regions are disallowed",
            ));
        }

        let mut ret = Self::new(pd, content.len())?;
        ret.buf.copy_from_slice(content);
        Ok(ret)
    }

    /// Get the address of the allocated memory.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        self.buf.as_ptr() as *mut u8
    }

    /// Get the length of the allocated memory.
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Get the underlying [`Mr`].
    #[inline]
    pub fn mr(&self) -> &Mr<'a> {
        &self.mr
    }

    /// Zero the allocated buffer.
    #[inline]
    pub fn clear(&mut self) {
        // SAFETY: the buffer is valid.
        unsafe {
            ptr::write_bytes(self.buf.as_mut_ptr(), 0, self.buf.len());
        }
    }
}

impl Deref for RegisteredMem<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.buf.as_ref()
    }
}

impl DerefMut for RegisteredMem<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf.as_mut()
    }
}

impl<'a, 's> Slicing<'s> for RegisteredMem<'a>
where
    'a: 's,
{
    type Output = MrSlice<'s>;

    fn addr(&'s self) -> *mut u8 {
        self.mr.addr()
    }

    fn len(&'s self) -> usize {
        self.mr.len()
    }

    unsafe fn slice_unchecked(&'s self, offset: usize, len: usize) -> Self::Output {
        MrSlice::new(&self.mr, offset, len)
    }
}
