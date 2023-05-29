use std::io::prelude::*;
use std::iter;
use std::net::*;
use std::path::Path;

use anyhow::Result;
use local_ip_address::list_afinet_netifas;
use log;

use super::{barrier::Barrier, connecter::Connecter};
use crate::rdma::{cq::Cq, pd::Pd, qp::*};

fn is_my_ip(ip: &Ipv4Addr) -> bool {
    let my_ips = list_afinet_netifas().unwrap();
    my_ips
        .iter()
        .any(|(_iface, if_ip)| *if_ip == IpAddr::V4(*ip))
}

/// Cluster information.
#[derive(Debug, Clone)]
pub struct Cluster {
    peers: Vec<Ipv4Addr>,
    id: usize,
}

impl Cluster {
    pub fn new_withid(peers: Vec<Ipv4Addr>, id: usize) -> Self {
        Cluster { peers, id }
    }

    pub fn new(peers: Vec<Ipv4Addr>) -> Self {
        let id = peers.iter().position(|x| is_my_ip(x)).unwrap();
        Self::new_withid(peers, id)
    }

    /// Load TOML cluster configuration.
    ///
    /// The TOML file should have a `rrddmma` table with a `peers` array
    /// containing the IPv4 addresses of every pair. For example:
    ///
    /// ```toml
    /// [rrddmma]
    /// peers = ["10.0.2.1", "10.0.2.2", "10.0.2.3"]
    /// ```
    ///
    /// Irrelevant fields will be ignored, and you can put the above
    /// configuration snippet in your own mixed TOML configuration.
    pub fn load_toml(toml: &str) -> Result<Self> {
        let toml: toml::Value = toml::from_str(toml)?;
        let peers = match toml["rrddmma"].as_table() {
            Some(t) => t,
            None => return Err(anyhow::anyhow!("rrddmma configuration not found")),
        };
        let peers = match peers["peers"].as_array() {
            Some(a) => a,
            None => return Err(anyhow::anyhow!("bad rrddmma configuration")),
        }
        .iter()
        .map(|x| x.as_str().unwrap().parse().unwrap())
        .collect();
        Ok(Self::new(peers))
    }

    /// Load cluster configuration from a TOML file.
    pub fn load_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut toml_str = String::new();
        file.read_to_string(&mut toml_str)?;

        Self::load_toml(&toml_str)
    }

    /// Load cluster configuration from CloudLab XML profile.
    ///
    /// The CloudLab XML profile is expected to contain a virtual network
    /// configuration that specifies an IPv4 address for each node. The IPv4
    /// address will appear in each node's configuration in the following form:
    ///
    /// ```xml
    /// <rspec ...>
    ///     <!-- ... -->
    ///     <node xmlns="..." client_id="...">
    ///         <interface xmlns="..." client_id="...">
    ///             <ip xmlns="..." address="10.0.0.1" type="ipv4"/>
    ///        </interface>
    ///     </node>
    /// </rspec>
    /// ```
    ///
    /// This function extracts the `address` attribute of such `ip` elements in
    /// each `node` and combines them together to get a configuration.
    pub fn load_cloudlab_xml(xml: &str) -> Result<Self> {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);

        let mut buf = vec![];
        let mut peers = vec![];
        loop {
            match reader.read_event_into(&mut buf) {
                Err(e) => return Err(anyhow::anyhow!(e)),
                Ok(Event::Eof) => break,

                Ok(Event::Empty(e)) => {
                    if e.name().as_ref() == b"ip" {
                        peers.push(
                            std::str::from_utf8(
                                e.attributes()
                                    .find(|x| x.clone().unwrap().key.as_ref() == b"address")
                                    .unwrap()?
                                    .value
                                    .as_ref(),
                            )
                            .unwrap()
                            .parse()
                            .unwrap(),
                        );
                    }
                }
                _ => (),
            }
        }
        buf.clear();

        Ok(Self::new(peers))
    }

    /// Load cluster configuration from a CloudLab XML profile file.
    pub fn load_cloudlab_xml_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut xml_str = String::new();
        file.read_to_string(&mut xml_str)?;

        Self::load_cloudlab_xml(&xml_str)
    }

    /// Get the IP addresses of all nodes in the cluster.
    #[inline]
    pub fn peers(&self) -> &Vec<Ipv4Addr> {
        &self.peers
    }

    /// Get the rank of this node in the cluster.
    #[inline]
    pub fn rank(&self) -> usize {
        self.id
    }

    /// Get the number of participants in the cluster.
    #[inline]
    pub fn size(&self) -> usize {
        self.peers.len()
    }

    /// Get the IP address of the node with the specified rank.
    #[inline]
    pub fn get(&self, id: usize) -> Option<Ipv4Addr> {
        self.peers.get(id).cloned()
    }

    /// Create [`Qp`]s and connect with all nodes in the cluster.
    /// Return an iterator that yields a `Vec` of `Qp`s and `QpPeer`s for
    /// each node. The `Vec`'s size is specified by `num_link`.
    ///
    /// The order of connection to different nodes is *determinate* but not
    /// in ascending order. Specifically, the iteration can be divided into
    /// `N` steps, where `N` is the cluster size `n` rounded up to a power of 2.
    /// In step `i`, the participant with rank `x` connects with an opponent
    /// with rank `x ^ i`. This ensures that in each phase, pariticipants
    /// form pairs and connect with each other. If such an opponent does not
    /// exist, the iterator will block until all other pariticipants also
    /// finished this step and move to the next step. In other words, the
    /// iterator will only yield value for `n - 1` times.
    ///
    /// **NOTE:** alll pariticipants are expected to call this function and
    /// iterate forwards concurrently. They will synchronize with each other
    /// using `Barrier`.
    #[inline]
    pub fn connect_all<'a, 'b: 'a>(
        &'a self,
        pd: &'b Pd,
        num_links: usize,
        qp_type: QpType,
    ) -> impl iter::Iterator<Item = (usize, Vec<(Qp, Option<QpPeer>)>)> + 'a {
        fn pow2_roundup(x: usize) -> usize {
            let mut n = 1;
            while n < x {
                n *= 2;
            }
            n
        }
        ConnectionIter {
            cluster: self,
            pd,
            n: pow2_roundup(self.size()),
            i: 1,
            num_links,
            qp_type,
        }
    }
}

struct ConnectionIter<'a, 'b> {
    cluster: &'a Cluster,
    pd: &'b Pd,
    n: usize,
    i: usize,
    num_links: usize,
    qp_type: QpType,
}

impl<'a, 'b> iter::Iterator for ConnectionIter<'a, 'b> {
    type Item = (usize, Vec<(Qp, Option<QpPeer>)>);

    fn next(&mut self) -> Option<Self::Item> {
        fn progress_iter<'a, 'b>(this: &mut ConnectionIter<'a, 'b>) -> Option<usize> {
            if this.i >= this.n {
                return None;
            }

            let id = this.cluster.rank();
            let peer_id = this.i ^ id;
            this.i += 1;
            Some(peer_id)
        }

        let mut peer_id = progress_iter(self)?;
        while peer_id >= self.cluster.size() {
            Barrier::wait(self.cluster);
            peer_id = progress_iter(self)?;
        }

        // Create QPs
        let qps = (0..self.num_links)
            .map(|_| {
                let send_cq = Cq::new(self.pd.context(), Cq::DEFAULT_CQ_DEPTH).unwrap();
                let recv_cq = Cq::new(self.pd.context(), Cq::DEFAULT_CQ_DEPTH).unwrap();
                let qp = Qp::new(
                    self.pd.clone(),
                    QpInitAttr::new(send_cq, recv_cq, QpCaps::default(), QpType::RC, true),
                )
                .unwrap();
                qp
            })
            .collect::<Vec<_>>();

        // Connect the QPs with the current peer
        let peers = Connecter::within_cluster(self.cluster, peer_id)
            .unwrap()
            .connect_many(&qps);
        if let Err(e) = peers {
            log::error!("rrddmma: failed to connect to peer {}: {:?}", peer_id, e);
            Barrier::wait(self.cluster);
            return None;
        }

        let ret = qps.into_iter().zip(peers.unwrap()).collect::<Vec<_>>();

        Barrier::wait(self.cluster);
        Some((peer_id, ret))
    }
}
