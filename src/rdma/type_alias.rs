/// [`u8`]: **Port number**, identifies a port on a local switch or an HCA.
pub type PortNum = u8;

/// [`u16`]: **Local identifier (LID)**, identifies a port on a switch or an HCA in the cluster.
pub type Lid = u16;

/// [`u8`]: **Global identifier (GID) index**, identifies a GID on a physical port.
pub type GidIndex = u8;

/// [`u32`]: **Queue pair number**, identifies a local queue pair.
pub type Qpn = u32;

/// [`u32`]: **Packet sequence number (PSN)**, identifies a packet in a flow.
pub type Psn = u32;

/// [`u32`]: **Queue key**, identifies a unreliable datagram queue pair.
pub type QKey = u32;

/// [`u32`]: **Local key**, identifies a local memory region.
pub type LKey = u32;

/// [`u32`]: **Remote key**, identifies a remote memory region.
pub type RKey = u32;

/// [`u64`]: **Work request identifier**, designated by the user to identify a work request.
pub type WrId = u64;

/// [`u32`]: **Immediate data**, can be carried in RDMA send work requests in network byte order.
pub type ImmData = u32;
