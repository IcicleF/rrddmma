use std::io::prelude::*;
use std::net::*;
use std::time::Duration;

use super::cluster::Cluster;
use crate::rdma::{qp::*, remote_mem::*};
use anyhow::{Context as _, Result};

fn stream_write(stream: &mut &TcpStream, buf: &[u8]) -> Result<()> {
    stream
        .write(&buf.len().to_le_bytes())
        .with_context(|| "failed to write message length")?;

    let mut written = 0;
    while written < buf.len() {
        let len = stream
            .write(&buf[written..])
            .with_context(|| format!("failed when writing bytes [{}..]", written))?;
        written += len;
    }
    Ok(())
}

fn stream_read(stream: &mut &TcpStream) -> Result<Vec<u8>> {
    let mut buf = [0; std::mem::size_of::<usize>()];
    stream
        .read_exact(&mut buf)
        .with_context(|| "failed to read message length")?;
    let len = usize::from_le_bytes(buf);

    let mut buf = vec![0; len];
    stream
        .read_exact(&mut buf)
        .with_context(|| "failed when reading bytes")?;
    Ok(buf)
}

fn connect_until_success(
    server_addr: SocketAddrV4,
    wait_on_failure: Duration,
) -> Result<TcpStream, std::io::Error> {
    loop {
        let stream = TcpStream::connect(server_addr);
        if stream.is_ok() {
            break stream;
        }
        std::thread::sleep(wait_on_failure);
    }
}

/// Connection manager that connects with a specific remote peer.
pub struct Connecter {
    /// Remote peer information. If `Some`, this is the client side; otherwise,
    /// this is the server side.
    with: Option<Ipv4Addr>,

    /// The established TCP connection.
    stream: Option<TcpStream>,
}

impl Connecter {
    /// Locally connect two QPs, without involving any networking.
    pub fn connect_local(first: &Qp, second: &Qp) -> Result<()> {
        let ep_first = first.endpoint();
        let ep_second = second.endpoint();

        first.connect(&ep_second).and(second.connect(&ep_first))
    }

    /// The default TCP port to use.
    pub const DEFAULT_PORT: u16 = 13337;

    /// Create a new `Connecter` that connects with the specified remote peer
    /// on the given TCP port.
    ///
    /// If the specified remote peer is `None`, this will be the server side.
    /// Otherwise, this will be the client side and will connect to the remote.
    pub fn new_on_port(with: Option<Ipv4Addr>, port: u16) -> Result<Self> {
        let stream = if let Some(addr) = with.as_ref() {
            let server_addr = SocketAddrV4::new(*addr, port);
            connect_until_success(server_addr, Duration::from_millis(200))
                .with_context(|| format!("failed to connect with {}", server_addr))?
        } else {
            let inaddr_any = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);
            let listener = TcpListener::bind(inaddr_any).with_context(|| "failed to listen")?;
            listener.accept().with_context(|| "failed to accept")?.0
        };

        Ok(Self {
            with,
            stream: Some(stream),
        })
    }

    /// Create a new `Connecter` that connects with the specified remote peer.
    pub fn new(with: Option<Ipv4Addr>) -> Result<Self> {
        Self::new_on_port(with, Self::DEFAULT_PORT)
    }

    /// Create a new `Connecter` that connects with the remote peer with the
    /// given rank in the cluster, on the given TCP port.
    ///
    /// Who will be the client is determined by the ranks of the two sides of
    /// the connection. The side with the smaller rank is the client.
    /// Generally, you must ensure that the port is vacant on both sides.
    pub fn within_cluster_on_port(cluster: &Cluster, with: usize, port: u16) -> Result<Self> {
        if with >= cluster.size() {
            return Err(anyhow::anyhow!(
                "rank {} is out of bounds (size = {})",
                with,
                cluster.size()
            ));
        }

        let id = cluster.rank();
        assert_ne!(id, with);

        let (with, stream) = if id < with {
            let server_addr = SocketAddrV4::new(cluster.peers()[with], port);
            let stream = connect_until_success(server_addr, Duration::from_millis(200))
                .with_context(|| format!("failed to connect with {}", server_addr))?;
            (Some(cluster.peers()[with]), stream)
        } else {
            let inaddr_any = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);
            let listener = TcpListener::bind(inaddr_any).with_context(|| "failed to listen")?;
            (
                None,
                listener.accept().with_context(|| "failed to accept")?.0,
            )
        };

        Ok(Connecter {
            with,
            stream: Some(stream),
        })
    }

    /// Create a new `Connecter` that connects with the remote peer with the
    /// given rank on the default TCP port 13337.
    pub fn within_cluster(cluster: &Cluster, with: usize) -> Result<Self> {
        Self::within_cluster_on_port(cluster, with, Self::DEFAULT_PORT)
    }

    /// Connect a QP with the remote peer.
    pub fn connect(&self, qp: &Qp) -> Result<Option<QpPeer>> {
        if self.stream.is_none() {
            return Err(anyhow::anyhow!("no connection established"));
        }

        let ep = qp.endpoint();
        let ep = serde_json::to_string(&ep).with_context(|| "failed to serialize QP info")?;

        let mut stream = self.stream.as_ref().unwrap();
        let ep = if self.with.is_some() {
            // First receive
            let buf =
                stream_read(&mut stream).with_context(|| "failed to receive remote QP info")?;
            let peer = serde_json::from_slice::<QpEndpoint>(buf.as_slice())
                .with_context(|| "failed to decode remote QP info")?;

            // Then send
            stream_write(&mut stream, ep.as_bytes())
                .with_context(|| "failed to send local QP info to remote")?;
            peer
        } else {
            // First send
            stream_write(&mut stream, ep.as_bytes())
                .with_context(|| "failed to send local QP info to remote")?;

            // Then receive
            let buf =
                stream_read(&mut stream).with_context(|| "failed to receive remote QP info")?;
            serde_json::from_slice::<QpEndpoint>(buf.as_slice())
                .with_context(|| "failed to decode remote QP info")?
        };

        qp.connect(&ep)?;
        if qp.qp_type() == QpType::Rc {
            Ok(None)
        } else {
            QpPeer::new(qp.pd(), ep).map(Some)
        }
    }

    /// Connect a list of QPs with the remote peer.
    ///
    /// It is expected that the opponent side calls this method simultaneously
    /// with a same number of QPs.
    pub fn connect_many(&self, qps: &[Qp]) -> Result<Vec<Option<QpPeer>>> {
        if self.stream.is_none() {
            return Err(anyhow::anyhow!("no connection established"));
        }

        let ep = qps.iter().map(|qp| qp.endpoint()).collect::<Vec<_>>();
        let ep = serde_json::to_string(&ep).with_context(|| "failed to serialize QP info vec")?;

        let mut stream = self.stream.as_ref().unwrap();
        let eps = if self.with.is_some() {
            // First receive
            let buf = stream_read(&mut stream)
                .with_context(|| "failed to receive remote QP info vec from")?;
            let peer =
                serde_json::from_slice::<Vec<QpEndpoint>>(buf.as_slice()).with_context(|| {
                    format!(
                        "failed to decode remote QP info vec (from: {})",
                        String::from_utf8_lossy(&buf)
                    )
                })?;

            // Then send
            stream_write(&mut stream, ep.as_bytes())
                .with_context(|| "failed to send local QP info vec to remote")?;
            peer
        } else {
            // First send
            stream_write(&mut stream, ep.as_bytes())
                .with_context(|| "failed to send local QP info vec to remote")?;

            // Then receive
            let buf =
                stream_read(&mut stream).with_context(|| "failed to receive remote QP info vec")?;
            serde_json::from_slice::<Vec<QpEndpoint>>(buf.as_slice()).with_context(|| {
                format!(
                    "failed to decode remote QP info vec (from: {})",
                    String::from_utf8_lossy(&buf)
                )
            })?
        };

        // Remote side must have the same number of QPs
        if eps.len() != qps.len() {
            return Err(anyhow::anyhow!(
                "QP number mismatch: local {} ({:?}) vs remote {} ({:?})",
                qps.len(),
                ep,
                eps.len(),
                eps
            ));
        }

        let peers = eps
            .into_iter()
            .zip(qps)
            .map(|(ep, qp)| {
                qp.connect(&ep).unwrap();
                if qp.qp_type() == QpType::Rc {
                    None
                } else {
                    Some(QpPeer::new(qp.pd(), ep).unwrap())
                }
            })
            .collect::<Vec<_>>();
        Ok(peers)
    }

    /// Send a local MR's information to the remote side.
    ///
    /// This method accepts a `MrSlice` instead of a `Mr` to let the sender
    /// control what part of the MR to send.
    pub fn send_mr(&self, slice: RemoteMem) -> Result<()> {
        if self.stream.is_none() {
            return Err(anyhow::anyhow!("no connection established"));
        }

        let mr = serde_json::to_string(&slice)?;
        let mut stream = self.stream.as_ref().unwrap();
        stream_write(&mut stream, mr.as_bytes())?;

        Ok(())
    }

    /// Receive sent MR information from the opponent's side.
    pub fn recv_mr(&self) -> Result<RemoteMem> {
        if self.stream.is_none() {
            return Err(anyhow::anyhow!("no connection established"));
        }

        let mut stream = self.stream.as_ref().unwrap();
        let buf = stream_read(&mut stream)?;
        let mr = serde_json::from_slice::<RemoteMem>(buf.as_slice())?;
        Ok(mr)
    }
}
