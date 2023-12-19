//! RDMA hardware resource discovery.

mod device;
mod port;

use std::io::Error as IoError;

use regex::Regex;
use thiserror::Error;

pub(crate) use self::device::*;
pub use self::port::*;
use super::context::*;

/// Port speed filter type.
enum PortSpeedFilter {
    AtLeast(f32),
    Exactly(f32),
}

/// RDMA hardware resource finder.
pub struct NicFinder {
    /// Device name filters (match any).
    dev_names: Vec<Regex>,

    /// Port number filter.
    port_nums: Vec<u8>,

    /// Port speed filter.
    port_speed: PortSpeedFilter,

    /// Port link layer protocol filter.
    port_link_layer: Option<PortLinkLayer>,

    /// NUMA node filter (match any).
    numa_nodes: Vec<u8>,
}

impl NicFinder {
    /// Determine whether the current filter matches the specified device.
    ///
    /// Checked filter(s):
    /// - Device name
    /// - Port number
    /// - NUMA node
    ///
    /// If a filter type is set but errors occurred when querying the device,
    /// the error will be ignored and the filter will be considered unmatched.
    ///
    /// # Safety
    ///
    /// - `ctx` must be a valid pointer to an `ibv_context`.
    fn is_device_eligible(&self, ctx: IbvContext) -> bool {
        // Short-circuit evaluation.
        (
            // Device name.
            self.dev_names.is_empty() || {
                let Ok(dev_name) = ctx.dev().name() else {
                    return false;
                };
                self.dev_names.iter().any(|re| re.is_match(&dev_name))
            }
        ) && ({
            // Port number.
            self.port_nums.is_empty() || {
                let Ok(dev_attr) = ctx.query_device() else {
                    return false;
                };
                self.port_nums
                    .iter()
                    .all(|&num| num <= dev_attr.phys_port_cnt)
            }
        }) && (
            // NUMA node.
            self.numa_nodes.is_empty() || {
                let Ok(numa) = ctx.dev().numa_node() else {
                    return false;
                };
                self.numa_nodes.contains(&numa)
            }
        )
    }

    /// Determine whether the current filter matches the specified port.
    ///
    /// Checked filter(s):
    /// - Port number
    /// - Port speed
    /// - Port link layer protocol
    ///
    /// If a filter type is set but errors occurred when querying the port,
    /// the error will be ignored and the filter will be considered unmatched.
    ///
    /// # Safety
    ///
    /// - `ctx` must be a valid pointer to an `ibv_context`.
    fn is_port_eligible(&self, ctx: IbvContext, port_num: u8) -> bool {
        let Ok(port) = Port::new(ctx, port_num) else {
            return false;
        };

        // Short-circuit evaluation.
        (
            // Port number.
            self.port_nums.is_empty() || self.port_nums.contains(&port_num)
        ) && (
            // Port speed.
            match self.port_speed {
                PortSpeedFilter::AtLeast(speed) => speed <= port.speed().gbps(),
                PortSpeedFilter::Exactly(speed) => speed == port.speed().gbps(),
            }
        ) && (
            // Link layer protocol.
            self.port_link_layer.is_none() || {
                let link_layer = port.link_layer();
                self.port_link_layer == Some(link_layer)
            }
        )
    }
}

impl NicFinder {
    /// Create a new RDMA hardware resource finder that matches any device and any port.
    pub fn new() -> Self {
        Self {
            dev_names: Vec::new(),
            port_nums: Vec::new(),
            port_speed: PortSpeedFilter::AtLeast(0.0),
            port_link_layer: None,
            numa_nodes: Vec::new(),
        }
    }

    /// Set a device name filter.
    /// Permit only devices whose name matches *any* of the filters.
    ///
    /// Regular expressions are supported.
    ///
    /// Device names are those returned by `ibv_get_device_name` or shown in `ibv_devinfo`
    /// command-line tool (e.g., `mlx5_0`). Note that this is *not* the network interface name
    /// (e.g., `ib0` or `enp65s0f0`).
    #[inline]
    pub fn dev_name(mut self, name: impl AsRef<str>) -> Self {
        self.dev_names
            .push(Regex::new(name.as_ref()).expect("invalid regex pattern"));
        self
    }

    /// Set a port number filter.
    /// Permit only ports with *any* of the specified port numbers.
    ///
    /// You may check the port numbers with the `ibv_devinfo` command-line tool.
    ///
    /// # Panic
    ///
    /// Panics if `num` is 0.
    #[inline]
    pub fn port_num(mut self, num: u8) -> Self {
        assert!(num > 0, "port number must be positive");
        self.port_nums.push(num);
        self
    }

    /// Set the port speed filter to be at least `speed` Gbps.
    /// Permit only devices equipped with a port with at least the specified active speed.
    ///
    /// The active speed of a port is its active link width multiplied by its active link speed.
    ///
    /// This will override the previous port speed filter, if any.
    #[inline]
    pub fn port_speed_at_least(mut self, speed: f32) -> Self {
        self.port_speed = PortSpeedFilter::AtLeast(speed);
        self
    }

    /// Set the port speed filter to be exactly `speed` Gbps.
    /// Permit only devices equipped with a port with exactly the specified active speed.
    ///
    /// The active speed of a port is its active link width multiplied by its active link speed.
    ///
    /// This will override the previous port speed filter, if any.
    #[inline]
    pub fn port_speed_exactly(mut self, speed: f32) -> Self {
        self.port_speed = PortSpeedFilter::Exactly(speed);
        self
    }

    /// Set the port link layer protocol filter.
    /// Permit only devices equipped with a port with the specified link layer protocol.
    ///
    /// This will override the previous port link layer protocol filter, if any.
    #[inline]
    pub fn port_link_layer(mut self, link_layer: PortLinkLayer) -> Self {
        self.port_link_layer = Some(link_layer);
        self
    }

    /// Set a NUMA node filter.
    /// Permit only devices installed on *any* of the specified NUMA nodes.
    ///
    /// You may check the NUMA node of a device by inspecting its device directory, e.g.,
    /// `/sys/class/infiniband/mlx5_0/device/numa_node`.
    #[inline]
    pub fn numa_node(mut self, node: u8) -> Self {
        self.numa_nodes.push(node);
        self
    }

    /// Find the first eligible RDMA device and open it.
    ///
    /// **NOTE:** The returned device contains information of *all* its physical ports,
    /// not only those matching the port filter.
    #[inline]
    pub fn probe(self) -> Result<Nic, NicProbeError> {
        self.probe_nth_dev(0)
    }

    /// Find the `n`-th eligible RDMA device and open it.
    /// Start counting from 0.
    ///
    /// **NOTE:** The returned device contains information of *all* its physical ports,
    /// not only those matching the port filter.
    pub fn probe_nth_dev(self, mut n: usize) -> Result<Nic, NicProbeError> {
        let dev_list = IbvDeviceList::new()?;
        for dev in &dev_list {
            let ctx = dev.open()?;
            if self.is_device_eligible(ctx) {
                let attr = ctx.query_device()?;
                if (1..=attr.phys_port_cnt).any(|port_num| self.is_port_eligible(ctx, port_num)) {
                    // Eligible device
                    if n == 0 {
                        let ports = (1..=attr.phys_port_cnt)
                            .map(|port_num| Port::new(ctx, port_num))
                            .collect::<Result<Vec<_>, _>>()?;
                        return Ok(Nic {
                            context: Context::new(ctx, attr),
                            ports,
                        });
                    } else {
                        n -= 1;
                    }
                }
            }

            // SAFETY: call only once and no UAF.
            unsafe { ctx.close()? };
        }
        Err(NicProbeError::NotFound)
    }

    /// Find the RDMA device that contains the `n`-th eligible port and open the device.
    /// Start counting from 0.
    ///
    /// **NOTE:** The returned device contains information of *only* the ports that match the filters.
    pub fn probe_nth_port(self, mut n: usize) -> Result<Nic, NicProbeError> {
        let dev_list = IbvDeviceList::new()?;
        for dev in &dev_list {
            let ctx = dev.open()?;
            if self.is_device_eligible(ctx) {
                let attr = ctx.query_device()?;
                for port_num in 1..=attr.phys_port_cnt {
                    if self.is_port_eligible(ctx, port_num) {
                        // Eligible port
                        if n == 0 {
                            let port = Port::new(ctx, port_num)?;
                            return Ok(Nic {
                                context: Context::new(ctx, attr),
                                ports: vec![port],
                            });
                        } else {
                            n -= 1;
                        }
                    }
                }
            }

            // SAFETY: call only once and no UAF.
            unsafe { ctx.close()? };
        }
        Err(NicProbeError::NotFound)
    }
}

impl Default for NicFinder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// NIC probe result error type.
#[derive(Debug, Error)]
pub enum NicProbeError {
    /// `libibverbs` interfaces returned an error when opening or querying
    /// the device.
    #[error("I/O error from ibverbs")]
    IoError(#[from] IoError),

    /// Failed to query port attributes.
    #[error("port query error")]
    PortQueryError(#[from] PortQueryError),

    /// No eligible RDMA device found.
    #[error("no eligible RDMA device found")]
    NotFound,
}

/// NIC probe result type.
/// Contains an opened RDMA device and a set of port metadata.
pub struct Nic {
    /// Device context.
    pub context: Context,

    /// Port metadata.
    pub ports: Vec<Port>,
}

impl Nic {
    /// Create a new finder instance.
    #[inline]
    pub fn finder() -> NicFinder {
        Default::default()
    }
}
