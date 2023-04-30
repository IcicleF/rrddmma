use super::cluster::Cluster;
use crate::rdma::{mr::*, qp::*};
use anyhow::Result;
use std::io::prelude::*;
use std::net::*;

fn stream_write(stream: &mut &TcpStream, buf: &[u8]) -> Result<()> {
    stream.write(&buf.len().to_le_bytes())?;

    let mut written = 0;
    while written < buf.len() {
        let len = stream.write(&buf[written..])?;
        written += len;
    }
    Ok(())
}

fn stream_read(stream: &mut &TcpStream) -> Result<Vec<u8>> {
    let mut buf = [0; std::mem::size_of::<usize>()];
    stream.read_exact(&mut buf)?;
    let len = usize::from_le_bytes(buf);

    let mut buf = vec![0; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// Connection manager that connects with a specific remote peer.
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

        let id = cluster.myself();
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

    pub fn connect(&self, qp: &'a Qp) -> Result<QpPeer> {
        let ep = QpEndpoint::from(qp);
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let ep = if self.cluster.myself() < self.with {
            // First receive
            let buf = stream_read(&mut stream)?;
            let peer = serde_json::from_slice::<QpEndpoint>(buf.as_slice())?;

            // Then send
            stream_write(&mut stream, ep.as_bytes())?;
            peer
        } else {
            // First send
            stream_write(&mut stream, ep.as_bytes())?;

            // Then receive
            let buf = stream_read(&mut stream)?;
            serde_json::from_slice::<QpEndpoint>(buf.as_slice())?
        };
        qp.connect(&ep)?;
        QpPeer::new(qp.pd(), ep)
    }

    pub fn connect_many(&self, qps: &Vec<Qp>) -> Result<Vec<QpPeer>> {
        let ep = qps
            .iter()
            .map(|qp| QpEndpoint::from(qp))
            .collect::<Vec<_>>();
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let eps = if self.cluster.myself() < self.with {
            // First receive
            let buf = stream_read(&mut stream)?;
            let peer = serde_json::from_slice::<Vec<QpEndpoint>>(buf.as_slice())?;

            // Then send
            stream_write(&mut stream, ep.as_bytes())?;
            peer
        } else {
            // First send
            stream_write(&mut stream, ep.as_bytes())?;

            // Then receive
            let buf = stream_read(&mut stream)?;
            serde_json::from_slice::<Vec<QpEndpoint>>(buf.as_slice())?
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

    pub fn send_mr(&self, mr: &Mr) -> Result<()> {
        let mr = RemoteMr::from(mr);
        let mr = serde_json::to_string(&mr)?;

        let mut stream = self.stream.as_ref().unwrap();
        stream_write(&mut stream, mr.as_bytes())?;

        Ok(())
    }

    pub fn recv_mr(&self) -> Result<RemoteMr> {
        let mut stream = self.stream.as_ref().unwrap();
        let buf = stream_read(&mut stream)?;
        let mr = serde_json::from_slice::<RemoteMr>(buf.as_slice())?;
        Ok(mr)
    }
}
