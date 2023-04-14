use local_ip_address::list_afinet_netifas;
use std::io::prelude::*;
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

    pub fn load_toml(config_file: &str) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(config_file)?;
        let mut toml_str = String::new();
        file.read_to_string(&mut toml_str)?;

        let toml: toml::Value = toml::from_str(&toml_str)?;
        let peers = toml["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().parse().unwrap())
            .collect::<Vec<Ipv4Addr>>();
        Ok(Self::new(peers))
    }

    pub fn load_cloudlab_xml(config_file: &str) -> anyhow::Result<Self> {
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
    pub fn id(&self) -> usize {
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
    pub fn connect_all<'a, 'b>(&'a self, pd: &'b Pd<'b>) -> ConnectionIter<'a, 'b> {
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
        }
    }
}

pub struct ConnectionIter<'a, 'b> {
    cluster: &'a Cluster,
    pd: &'b Pd<'b>,
    n: usize,
    i: usize,
}

impl<'a, 'b> std::iter::Iterator for ConnectionIter<'a, 'b> {
    type Item = (usize, Qp<'b>, QpPeer);

    fn next(&mut self) -> Option<Self::Item> {
        fn progress_iter<'a, 'b>(this: &mut ConnectionIter<'a, 'b>) -> Option<usize> {
            if this.i >= this.n {
                return None;
            }

            let id = this.cluster.id();
            let peer_id = this.i ^ id;
            this.i += 1;
            Some(peer_id)
        }

        let mut peer_id = progress_iter(self)?;
        while peer_id >= self.cluster.size() {
            Barrier::wait(self.cluster);
            peer_id = progress_iter(self)?;
        }

        let send_cq = Arc::new(Cq::new(self.pd.context(), None).unwrap());
        let recv_cq = Arc::new(Cq::new(self.pd.context(), None).unwrap());
        let qp = Qp::new(
            self.pd,
            QpInitAttr::new(send_cq, recv_cq, QpCaps::default(), QpType::RC, true),
        )
        .unwrap();
        let peer = Connecter::new(self.cluster, peer_id).connect(&qp).unwrap();

        Barrier::wait(self.cluster);
        Some((peer_id, qp, peer))
    }
}
