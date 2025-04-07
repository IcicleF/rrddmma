use std::io::{self, Read, Write};
use std::mem;
use std::net::*;
use std::time::Duration;

use crate::lo::{mr::*, qp::*};

fn stream_write(stream: &mut &TcpStream, buf: &[u8]) -> io::Result<()> {
    stream.write_all(&buf.len().to_le_bytes())?;

    let mut written = 0;
    while written < buf.len() {
        let len = stream.write(&buf[written..])?;
        written += len;
    }
    Ok(())
}

fn stream_read(stream: &mut &TcpStream) -> io::Result<Vec<u8>> {
    let mut buf = [0; mem::size_of::<usize>()];
    stream.read_exact(&mut buf)?;
    let len = usize::from_le_bytes(buf);

    let mut buf = vec![0; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

fn connect_until_success(
    server_addr: SocketAddrV4,
    wait_on_failure: Duration,
) -> io::Result<TcpStream> {
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
    /// The two QPs must all be already bound to ports.
    ///
    /// If error occurs when setting the first QP's peer, the second QP's peer
    /// will not be set.
    ///
    /// # Panics
    ///
    /// Panic if any QP is not bound to a local port.
    pub fn connect_local(first: &mut Qp, second: &mut Qp) -> io::Result<()> {
        let ep_first = first.endpoint().unwrap();
        let ep_second = second.endpoint().unwrap();
        first
            .bind_peer(ep_second)
            .and_then(|_| second.bind_peer(ep_first))
    }

    /// The default TCP port to use.
    pub const DEFAULT_PORT: u16 = 13337;

    /// Create a new `Connecter` that connects with the specified remote peer
    /// on the given TCP port.
    ///
    /// If the specified remote peer is `None`, this will be the server side.
    /// Otherwise, this will be the client side and will connect to the remote.
    pub fn new_on_port(with: Option<Ipv4Addr>, port: u16) -> io::Result<Self> {
        let stream = if let Some(addr) = with.as_ref() {
            let server_addr = SocketAddrV4::new(*addr, port);
            connect_until_success(server_addr, Duration::from_millis(200))?
        } else {
            let inaddr_any = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);
            let listener = TcpListener::bind(inaddr_any)?;
            listener.accept()?.0
        };

        Ok(Self {
            with,
            stream: Some(stream),
        })
    }

    /// Create a new `Connecter` that connects with the specified remote peer.
    pub fn new(with: Option<Ipv4Addr>) -> io::Result<Self> {
        Self::new_on_port(with, Self::DEFAULT_PORT)
    }

    /// Connect a QP with the remote peer.
    /// The QP must be already bound to a local port.
    ///
    /// Behavior:
    /// - If the QP is UC or RC, this will bring up the QP.
    /// - If the QP is UD, this will only exchange peer information.
    ///
    /// # Panics
    ///
    /// Panic if the QP is not bound to a local port.
    pub fn connect(&self, qp: &mut Qp) -> io::Result<Option<QpPeer>> {
        let ep = qp.endpoint();
        let ep = serde_json::to_string(&ep)?;

        let mut stream = self.stream.as_ref().unwrap();
        let ep = if self.with.is_some() {
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

        if qp.qp_type() == QpType::Rc {
            qp.bind_peer(ep)?;
            Ok(None)
        } else {
            let sgid_index = if qp.use_global_routing() {
                qp.port().unwrap().1
            } else {
                0
            };
            QpPeer::new(qp.pd(), sgid_index, ep).map(Some)
        }
    }

    /// Send a local MR's information to the remote side.
    ///
    /// This method accepts a `MrSlice` instead of a `Mr` to let the sender
    /// control what part of the MR to send.
    pub fn send_mr(&self, slice: MrRemote) -> io::Result<()> {
        let mr = serde_json::to_string(&slice)?;
        let mut stream = self.stream.as_ref().unwrap();
        stream_write(&mut stream, mr.as_bytes())?;

        Ok(())
    }

    /// Receive sent MR information from the opponent's side.
    pub fn recv_mr(&self) -> io::Result<MrRemote> {
        let mut stream = self.stream.as_ref().unwrap();
        let buf = stream_read(&mut stream)?;
        let mr = serde_json::from_slice::<MrRemote>(buf.as_slice())?;
        Ok(mr)
    }
}
