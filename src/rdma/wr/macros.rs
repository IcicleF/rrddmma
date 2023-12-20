#[allow(unused_macros)]
macro_rules! impl_wr_basic_setters {
    ($wr:tt) => {
        impl<'a, const N: usize> $wr<'a, N> {
            /// Set the next work request.
            pub fn set_next<const M: usize>(&mut self, next: &$wr<'a, M>) -> &mut Self {
                self.wr.next = &next.wr as *const _ as _;
                self
            }

            /// Set the work request ID.
            pub fn set_id(&mut self, wr_id: u64) -> &mut Self {
                self.wr.wr_id = wr_id;
                self
            }

            /// Set the number of scatter/gather elements.
            pub fn set_sgl_len(&mut self, num_sge: u32) -> &mut Self {
                assert!(
                    num_sge <= N as u32,
                    "SGL length {} exceeds maximum {}",
                    num_sge,
                    N
                );
                self.wr.num_sge = num_sge as _;
                self
            }

            /// Set the SGE.
            /// Automatically updates the SGL length of the WR if necessary.
            ///
            /// # Panics
            ///
            /// Panic if the index is out of bounds.
            pub fn set_sge(
                &mut self,
                index: usize,
                mr_slice: &$crate::rdma::mr::MrSlice<'a>,
            ) -> &mut Self {
                self.sgl[index] = ibv_sge {
                    addr: mr_slice.addr() as _,
                    length: mr_slice.len() as _,
                    lkey: mr_slice.lkey(),
                };
                self.wr.num_sge = ::std::cmp::max(self.wr.num_sge, index as i32 + 1);
                self
            }
        }
    };
}

#[allow(unused_macros)]
macro_rules! impl_wr_flags_setters {
    ($wr:tt, $flags:tt) => {
        impl<'a, const N: usize> $wr<'a, N> {
            /// Set the work request flags.
            pub fn set_flags(&mut self, flags: u32) -> &mut Self {
                self.wr.$flags = flags;
                self
            }

            /// Set the work request flags to include `IBV_SEND_SIGNALED`.
            pub fn set_flag_signaled(&mut self) -> &mut Self {
                self.wr.$flags |= $crate::bindings::ibv_send_flags::IBV_SEND_SIGNALED.0;
                self
            }

            /// Set the work request flags to include `IBV_SEND_SOLICITED`.
            pub fn set_flag_solicited(&mut self) -> &mut Self {
                self.wr.$flags |= $crate::bindings::ibv_send_flags::IBV_SEND_SOLICITED.0;
                self
            }

            /// Set the work request flags to include `IBV_SEND_INLINE`.
            pub fn set_flag_inline(&mut self) -> &mut Self {
                self.wr.$flags |= $crate::bindings::ibv_send_flags::IBV_SEND_INLINE.0;
                self
            }
        }
    };
}

#[allow(unused_macros)]
macro_rules! impl_wr_raw_accessors {
    ($wr:tt, $wr_type:tt) => {
        impl<'a, const N: usize> $wr<'a, N> {
            /// Get a raw pointer to the work request.
            #[inline]
            pub fn as_ptr(&self) -> *const $wr_type {
                &self.wr
            }

            /// Get a mutable raw pointer to the work request.
            #[inline]
            pub fn as_mut_ptr(&mut self) -> *mut $wr_type {
                &mut self.wr
            }

            /// Get a raw pointer to the start of the SGL.
            #[inline]
            pub fn sgl_as_ptr(&self) -> *const ibv_sge {
                self.wr.sg_list
            }

            /// Get a mutable raw pointer to the start of the SGL.
            #[inline]
            pub fn sgl_as_mut_ptr(&mut self) -> *mut ibv_sge {
                self.wr.sg_list
            }
        }
    };
}
