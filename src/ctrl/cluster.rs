use anyhow::Result;
use local_ip_address::list_afinet_netifas;
use log;

use std::io::prelude::*;
use std::iter;
use std::net::*;
use std::sync::Arc;

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

    pub fn load_toml(config_file: &str) -> Result<Self> {
        let mut file = std::fs::File::open(config_file)?;
        let mut toml_str = String::new();
        file.read_to_string(&mut toml_str)?;

        let toml: toml::Value = toml::from_str(&toml_str)?;
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

    pub fn load_cloudlab_xml(config_file: &str) -> Result<Self> {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut file = std::fs::File::open(config_file)?;
        let mut xml_str = String::new();
        file.read_to_string(&mut xml_str)?;

        let mut reader = Reader::from_str(&xml_str);
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

    #[inline]
    pub fn peers(&self) -> &Vec<Ipv4Addr> {
        &self.peers
    }

    #[inline]
    pub fn myself(&self) -> usize {
        self.id
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.peers.len()
    }

    #[inline]
    pub fn get(&self, id: usize) -> Option<Ipv4Addr> {
        self.peers.get(id).cloned()
    }

    #[inline]
    pub fn connect_all<'a, 'b: 'a>(
        &'a self,
        pd: &'b Pd<'b>,
        qp_type: QpType,
        num_links: usize,
    ) -> impl iter::Iterator<Item = (usize, Vec<(Qp<'b>, QpPeer)>)> + 'a {
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
            qp_type,
            num_links,
        }
    }
}

struct ConnectionIter<'a, 'b> {
    cluster: &'a Cluster,
    pd: &'b Pd<'b>,
    n: usize,
    i: usize,
    qp_type: QpType,
    num_links: usize,
}

impl<'a, 'b> iter::Iterator for ConnectionIter<'a, 'b> {
    type Item = (usize, Vec<(Qp<'b>, QpPeer)>);

    fn next(&mut self) -> Option<Self::Item> {
        fn progress_iter<'a, 'b>(this: &mut ConnectionIter<'a, 'b>) -> Option<usize> {
            if this.i >= this.n {
                return None;
            }

            let id = this.cluster.myself();
            let peer_id = this.i ^ id;
            this.i += 1;
            Some(peer_id)
        }

        let mut peer_id = progress_iter(self)?;
        while peer_id >= self.cluster.size() {
            Barrier::wait(self.cluster);
            peer_id = progress_iter(self)?;
        }

        let qps = (0..self.num_links)
            .map(|_| {
                let send_cq = Arc::new(Cq::new(self.pd.context(), None).unwrap());
                let recv_cq = Arc::new(Cq::new(self.pd.context(), None).unwrap());
                let qp = Qp::new(
                    self.pd,
                    QpInitAttr::new(send_cq, recv_cq, QpCaps::default(), self.qp_type, true),
                )
                .unwrap();
                qp
            })
            .collect::<Vec<_>>();
        let ret = Connecter::new(self.cluster, peer_id).connect_many(&qps);
        if let Err(e) = ret {
            log::error!("rrddmma: failed to connect to peer {}: {:?}", peer_id, e);
            Barrier::wait(self.cluster);
            return None;
        }
        let ret = qps.into_iter().zip(ret.unwrap()).collect();

        Barrier::wait(self.cluster);
        Some((peer_id, ret))
    }
}
