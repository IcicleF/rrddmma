use super::cluster::Cluster;
use std::io::prelude::*;
use std::net::*;

/// Distributed barrier.
///
/// Synchronize all processes in the cluster.
pub struct Barrier;

impl Barrier {
    /// Wait for all processes in the cluster to reach this point of the code
    /// using the given TCP port.
    ///
    /// ## Synchronization scheme
    ///
    /// The process with rank 0 will listen on the given port. All other
    /// processes will try to connect to the process with rank 0. Once the
    /// rank 0 process has received all connections, it will send a byte to
    /// all other processes to let them proceed.
    pub fn wait_on_port(cluster: &Cluster, port: u16) {
        if cluster.rank() == 0 {
            let inaddr_any = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);
            let listener = TcpListener::bind(inaddr_any).unwrap();

            let mut streams = vec![];
            for _ in 1..cluster.size() {
                streams.push(listener.accept().unwrap().0);
            }

            let buf = [0; 1];
            for mut stream in streams {
                stream.write(&buf).unwrap();
            }
        } else {
            let server_addr = SocketAddrV4::new(cluster.peers()[0], port);
            let mut stream;
            loop {
                stream = TcpStream::connect(server_addr);
                if stream.is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let mut stream = stream.unwrap();

            let mut buf = [0; 1];
            stream.read(&mut buf).unwrap();
        }
    }

    /// Wait for all processes in the cluster to reach this point of the code
    /// using the default TCP port 13373.
    pub fn wait(cluster: &Cluster) {
        const PORT: u16 = 13373;
        Self::wait_on_port(cluster, PORT);
    }
}
