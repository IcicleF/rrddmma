use std::io::prelude::*;
use std::net::*;

use super::cluster::Cluster;
use crate::rdma::{mr::*, qp::*, remote_mem::*};
use anyhow::Result;

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
    /// Create a new `Connecter` that connects with the remote peer with the
    /// given rank on the given TCP port.
    ///
    /// Who will be the client is determined by the ranks of the two sides of
    /// the connection. The side with the smaller rank will be the client.
    /// Generally, you must ensure that the port is vacant on both sides.
    pub fn new_on_port(cluster: &'a Cluster, with: usize, port: u16) -> Self {
        if with >= cluster.size() {
            return Self {
                cluster,
                with,
                _port: port,
                stream: None,
            };
        }

        let id = cluster.rank();
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

    /// Create a new `Connecter` that connects with the remote peer with the
    /// given rank on the default TCP port 13337.
    pub fn new(cluster: &'a Cluster, with: usize) -> Self {
        const PORT: u16 = 13337;
        Self::new_on_port(cluster, with, PORT)
    }

    /// Connect a QP with the remote peer.
    pub fn connect(&self, qp: &'a Qp) -> Result<QpPeer> {
        let ep = QpEndpoint::from(qp);
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let ep = if self.cluster.rank() < self.with {
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

    /// Connect a list of QPs with the remote peer.
    ///
    /// It is expected that the opponent side calls this method simultaneously
    /// with a same number of QPs.
    pub fn connect_many(&self, qps: &Vec<Qp>) -> Result<Vec<QpPeer>> {
        let ep = qps
            .iter()
            .map(|qp| QpEndpoint::from(qp))
            .collect::<Vec<_>>();
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let eps = if self.cluster.rank() < self.with {
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

        if eps.len() != qps.len() {
            return Err(anyhow::anyhow!("QP number mismatch"));
        }
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

    /// Send a local MR's information to the remote side.
    ///
    /// This method accepts a `MrSlice` instead of a `Mr` to let the sender
    /// control what part of the MR to send.
    pub fn send_mr(&self, slice: MrSlice) -> Result<()> {
        let mr_data = RemoteMem::from(slice);
        let mr = serde_json::to_string(&mr_data)?;

        let mut stream = self.stream.as_ref().unwrap();
        stream_write(&mut stream, mr.as_bytes())?;

        Ok(())
    }

    /// Receive sent MR information from the opponent's side.
    pub fn recv_mr(&self) -> Result<RemoteMem> {
        let mut stream = self.stream.as_ref().unwrap();
        let buf = stream_read(&mut stream)?;
        let mr = serde_json::from_slice::<RemoteMem>(buf.as_slice())?;
        Ok(mr)
    }
}
