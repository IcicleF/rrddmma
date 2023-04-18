use super::cluster::Cluster;
use crate::rdma::qp::*;
use std::io::prelude::*;
use std::net::*;

pub struct Connecter<'a> {
    cluster: &'a Cluster,
    with: usize,
    _port: u16,
    stream: Option<TcpStream>,
}

impl<'a> Connecter<'a> {
    pub fn new_on_port(cluster: &'a Cluster, with: usize, port: u16) -> Self {
        if with >= cluster.size() {
            return Self {
                cluster,
                with,
                _port: port,
                stream: None,
            };
        }

        let id = cluster.id();
        assert_ne!(id, with);

        let stream = if id < with {
            let server_addr = SocketAddrV4::new(cluster.peers()[with], port);
            loop {
                let stream = TcpStream::connect(server_addr);
                if stream.is_ok() {
                    break stream;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            .unwrap()
        } else {
            let inaddr_any = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);
            let listener = TcpListener::bind(inaddr_any).unwrap();
            listener.accept().unwrap().0
        };

        Connecter {
            cluster,
            with,
            _port: port,
            stream: Some(stream),
        }
    }

    pub fn new(cluster: &'a Cluster, with: usize) -> Self {
        const PORT: u16 = 13337;
        Self::new_on_port(cluster, with, PORT)
    }

    pub fn connect(&self, qp: &'a Qp) -> anyhow::Result<QpPeer> {
        let ep = QpEndpoint::from(qp);
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let ep = if self.cluster.id() < self.with {
            // First receive
            let mut buf = [0; 1024];
            let len = stream.read(&mut buf)?;
            let peer = serde_json::from_slice::<QpEndpoint>(&buf[..len])?;

            // Then send
            stream.write(ep.as_bytes())?;
            peer
        } else {
            // First send
            stream.write(ep.as_bytes())?;

            // Then receive
            let mut buf = [0; 1024];
            let len = stream.read(&mut buf)?;
            serde_json::from_slice::<QpEndpoint>(&buf[..len])?
        };
        qp.connect(&ep)?;
        QpPeer::new(qp.pd(), ep)
    }

    pub fn connect_all(&self, qps: &Vec<Qp>) -> anyhow::Result<Vec<QpPeer>> {
        let ep = qps
            .iter()
            .map(|qp| QpEndpoint::from(qp))
            .collect::<Vec<_>>();
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let eps = if self.cluster.id() < self.with {
            // First receive
            let mut buf = [0; 1024];
            let len = stream.read(&mut buf)?;
            let peer = serde_json::from_slice::<Vec<QpEndpoint>>(&buf[..len])?;

            // Then send
            stream.write(ep.as_bytes())?;
            peer
        } else {
            // First send
            stream.write(ep.as_bytes())?;

            // Then receive
            let mut buf = [0; 1024];
            let len = stream.read(&mut buf)?;
            serde_json::from_slice::<Vec<QpEndpoint>>(&buf[..len])?
        };
        let peers = eps
            .into_iter()
            .zip(qps)
            .map(|(ep, qp)| {
                qp.connect(&ep).unwrap();
                QpPeer::new(qp.pd(), ep).unwrap()
            })
            .collect::<Vec<_>>();
        Ok(peers)
    }
}
