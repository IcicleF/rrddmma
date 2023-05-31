use std::io::prelude::*;
use std::net::*;
use std::path::Path;

use anyhow::Result;
use local_ip_address::list_afinet_netifas;

use super::{barrier::Barrier, connecter::Connecter};
use crate::rdma::{cq::Cq, pd::Pd, qp::*};
use crate::utils::select::*;

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

    /// Create full (all-to-all) connection among the nodes in the cluster.
    /// The parameter `num_links` specifies the number of parallel connections
    /// to establish between each pair of nodes.
    ///
    /// All pariticipants are expected to call this method at the same time.
    /// They will synchronize with each other using `Barrier`.
    ///
    /// If `share_send_cq` is true, all QPs will share a single send CQ.
    /// Otherwise, each QP will have its own send CQ. The same holds for
    /// `share_recv_cq`.
    ///
    /// # Demonstration
    ///
    /// For example, if there are three nodes in the cluster, then the
    /// established connections will look like a 3-vertex complete graph when
    /// `num_links` is 1:
    ///
    /// ```plain
    /// 0 ------ 1 ------ 2
    /// |                 |
    /// +-----------------+
    /// ```
    ///
    /// If `num_links` is more than one, then the above graph will simply be
    /// replicated for `num_links` times.
    ///
    /// # Connection order
    ///
    /// The order of connection to different nodes is determinate, but not in
    /// ascending order.
    ///
    /// Specifically, the iteration can be divided into `N - 1` steps, where
    /// `N` is the cluster size `n` rounded up to a power of 2.
    /// In step `i`, the participant with rank `x` connects with an opponent
    /// with rank `x ^ i`. This ensures that in each phase, pariticipants
    /// form pairs and connect with each other. If such an opponent does not
    /// exist, the iterator will block until all other pariticipants also
    /// finished this step and move to the next step.
    #[inline]
    pub fn connect_fc(
        &self,
        pd: Pd,
        num_links: usize,
        qp_type: QpType,
        share_send_cq: bool,
        share_recv_cq: bool,
    ) -> Result<Vec<Option<Vec<(Qp, Option<QpPeer>)>>>> {
        // XOR connection
        let n = {
            let mut n = 1;
            while n < self.size() {
                n *= 2;
            }
            n
        };

        let mut connections = Vec::with_capacity(self.size());
        for _ in 0..self.size() {
            connections.push(None);
        }

        let shared_scq = share_send_cq.select(
            Some(Cq::new(
                pd.context(),
                Cq::DEFAULT_CQ_DEPTH * self.size() as i32,
            )?),
            None,
        );
        let shared_rcq = share_recv_cq.select(
            Some(Cq::new(
                pd.context(),
                Cq::DEFAULT_CQ_DEPTH * self.size() as i32,
            )?),
            None,
        );

        for i in 1..n {
            let id = self.rank();
            let peer_id = i ^ id;

            // Connect with the peer if it is existent
            if peer_id < self.size() {
                // Create QPs
                let mut qps = Vec::with_capacity(num_links);
                for _ in 0..num_links {
                    let send_cq = share_send_cq.select(
                        shared_scq.clone().unwrap(),
                        Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH)?,
                    );
                    let recv_cq = share_recv_cq.select(
                        shared_rcq.clone().unwrap(),
                        Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH)?,
                    );

                    let qp = Qp::new(
                        pd.clone(),
                        QpInitAttr::new(send_cq, recv_cq, QpCaps::default(), qp_type, true),
                    )?;
                    qps.push(qp);
                }

                // Connect the QPs with the current peer
                let peers = Connecter::within_cluster(self, peer_id)?.connect_many(&qps)?;
                let conn = qps.into_iter().zip(peers).collect::<Vec<_>>();
                connections[peer_id] = Some(conn);
            }
            Barrier::wait(self);
        }
        Ok(connections)
    }

    /// Create client-server RC connection among the nodes in the cluster.
    ///
    /// This method will create a logical client and a logical server on each
    /// node. Each client will connect to all servers on all other nodes.
    ///
    /// All pariticipants are expected to call this method at the same time.
    /// They will synchronize with each other using `Barrier`.
    ///
    /// If `share_recv_cq` is true, for each server, all QPs will share the same
    /// recv CQ. Otherwise, each QP will have its recv CQ. Client QPs always have
    /// their own CQs.
    ///
    /// **NOTE:** This method supports only RC QPs since there is no connection
    /// with UD QPs. Use [`Cluster::connect_fc`] for UD QPs and manually assign
    /// each with roles of clients or servers.
    ///
    /// # Return value
    ///
    /// A tuple containing first the QPs for clients, then the QPs for servers.
    ///
    /// # Demonstration
    ///
    /// For example, if there are three nodes in the cluster, then the established
    /// connections will look like a bipartite graph:
    ///
    /// ```plain
    ///  +-- 0-cli      1-cli      2-cli --+
    ///  |        \    /     \    /        |
    ///  |         \  /       \  /         |
    ///  |          \/         \/          |
    ///  |          /\         /\          |
    ///  |         /  \       /  \         |
    ///  |        /    \     /    \        |
    ///  |   0-svr      1-svr      2-svr   |
    ///  |     |                     |     |
    ///  +---------------------------+     |
    ///        |                           |
    ///        +---------------------------+
    /// ```
    ///
    /// Note that there will not be RDMA connections between the clients and the
    /// servers in the same node.
    #[inline]
    pub fn connect_cs(
        &self,
        pd: Pd,
        share_recv_cq: bool,
    ) -> Result<(Vec<Option<Qp>>, Vec<Option<Qp>>)> {
        let n = {
            let mut n = 1;
            while n < self.size() {
                n *= 2;
            }
            n
        };

        let mut clients = Vec::with_capacity(self.size());
        let mut servers = Vec::with_capacity(self.size());
        for _ in 0..self.size() {
            clients.push(None);
            servers.push(None);
        }

        let shared_rcq = share_recv_cq.select(
            Some(Cq::new(
                pd.context(),
                Cq::DEFAULT_CQ_DEPTH * self.size() as i32,
            )?),
            None,
        );

        for i in 1..n {
            let id = self.rank();
            let peer_id = i ^ id;

            // Connect with the peer if it is existent
            if peer_id < self.size() {
                // Create QPs
                let cli = {
                    let send_cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_SHORT_DEPTH)?;
                    let recv_cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_SHORT_DEPTH)?;
                    Qp::new(
                        pd.clone(),
                        QpInitAttr::new(send_cq, recv_cq, QpCaps::default(), QpType::Rc, true),
                    )?
                };
                let svr = {
                    let send_cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH)?;
                    let recv_cq = share_recv_cq.select(
                        shared_rcq.clone().unwrap(),
                        Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH)?,
                    );
                    Qp::new(
                        pd.clone(),
                        QpInitAttr::new(send_cq, recv_cq, QpCaps::default(), QpType::Rc, true),
                    )?
                };

                // Connect the QPs with the current peer
                Connecter::within_cluster(self, peer_id)?.connect_many(&{
                    if id < peer_id {
                        vec![cli.clone(), svr.clone()]
                    } else {
                        vec![svr.clone(), cli.clone()]
                    }
                })?;
                clients[peer_id] = Some(cli);
                servers[peer_id] = Some(svr);
            }
            Barrier::wait(self);
        }
        Ok((clients, servers))
    }
}

/// Provide a helper trait to remove the [`QpPeer`]s from the return value of
/// [`Cluster::connect_fc`].
pub trait DiscardQpPeer {
    /// Remove [`QpPeer`]s from the return value of [`Cluster::connect_fc`].
    fn discard_peers(self) -> Vec<Option<Vec<Qp>>>;
}

impl DiscardQpPeer for Vec<Option<Vec<(Qp, Option<QpPeer>)>>> {
    fn discard_peers(self) -> Vec<Option<Vec<Qp>>> {
        self.into_iter()
            .map(|x| x.map(|y| y.into_iter().map(|(qp, _)| qp).collect()))
            .collect()
    }
}
