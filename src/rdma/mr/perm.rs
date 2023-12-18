use std::ops::{Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, Sub, SubAssign};

use crate::bindings::ibv_access_flags;

/// Memory region permissions.
#[repr(transparent)]
pub struct Permission(ibv_access_flags);

impl Permission {
    pub const EMPTY: Self = Self(ibv_access_flags(0));
    pub const LOCAL_WRITE: Self = Self(ibv_access_flags::IBV_ACCESS_LOCAL_WRITE);
    pub const REMOTE_READ: Self = Self(ibv_access_flags::IBV_ACCESS_REMOTE_READ);
    pub const REMOTE_WRITE: Self = Self(ibv_access_flags::IBV_ACCESS_REMOTE_WRITE);
    pub const REMOTE_ATOMIC: Self = Self(ibv_access_flags::IBV_ACCESS_REMOTE_ATOMIC);
    pub const MW_BIND: Self = Self(ibv_access_flags::IBV_ACCESS_MW_BIND);
    pub const ZERO_BASED: Self = Self(ibv_access_flags::IBV_ACCESS_ZERO_BASED);
    pub const ON_DEMAND: Self = Self(ibv_access_flags::IBV_ACCESS_ON_DEMAND);
}

impl Default for Permission {
    /// Allow local write, remote read/write, and remote atomic.
    fn default() -> Self {
        Self::LOCAL_WRITE | Self::REMOTE_READ | Self::REMOTE_WRITE | Self::REMOTE_ATOMIC
    }
}

impl From<Permission> for i32 {
    fn from(p: Permission) -> Self {
        p.0 .0 as _
    }
}

impl Add for Permission {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl AddAssign for Permission {
    fn add_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl Sub for Permission {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(ibv_access_flags(self.0 .0 & !rhs.0 .0))
    }
}

impl SubAssign for Permission {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 .0 = self.0 .0 & !rhs.0 .0;
    }
}

impl BitAnd for Permission {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for Permission {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOr for Permission {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self + rhs
    }
}

impl BitOrAssign for Permission {
    fn bitor_assign(&mut self, rhs: Self) {
        *self += rhs;
    }
}
