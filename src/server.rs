use core::fmt;
use core::mem::MaybeUninit;
use smoltcp::{
    iface::EthernetInterface,
    socket::{SocketSet, SocketHandle, TcpSocket, TcpSocketBuffer, SocketRef},
    time::Instant,
};


pub struct SocketState<S> {
    handle: SocketHandle,
    state: S,
}

/// Number of server sockets and therefore concurrent client
/// sessions. Many data structures in `Server::run()` correspond to
/// this const.
const SOCKET_COUNT: usize = 8;

const TCP_RX_BUFFER_SIZE: usize = 2048;
const TCP_TX_BUFFER_SIZE: usize = 2048;

/// Contains a number of server sockets that get all sent the same
/// data (through `fmt::Write`).
pub struct Server<'a, 'b, S> {
    net: EthernetInterface<'a, 'a, 'a, &'a mut stm32_eth::Eth<'static, 'static>>,
    sockets: SocketSet<'b, 'b, 'b>,
    states: [SocketState<S>; SOCKET_COUNT],
}

impl<'a, 'b, S: Default> Server<'a, 'b, S> {
    /// Run a server with stack-allocated sockets
    pub fn run<F>(net: EthernetInterface<'a, 'a, 'a, &'a mut stm32_eth::Eth<'static, 'static>>, f: F)
    where
        F: FnOnce(&mut Server<'a, '_, S>),
    {
        let mut sockets_storage: [_; SOCKET_COUNT] = Default::default();
        let mut sockets = SocketSet::new(&mut sockets_storage[..]);
        let mut states: [SocketState<S>; SOCKET_COUNT] = unsafe { MaybeUninit::uninit().assume_init() };

        macro_rules! create_socket {
            ($set:ident, $rx_storage:ident, $tx_storage:ident, $target:expr) => {
                let mut $rx_storage = [0; TCP_RX_BUFFER_SIZE];
                let mut $tx_storage = [0; TCP_TX_BUFFER_SIZE];
                let tcp_rx_buffer = TcpSocketBuffer::new(&mut $rx_storage[..]);
                let tcp_tx_buffer = TcpSocketBuffer::new(&mut $tx_storage[..]);
                let tcp_socket = TcpSocket::new(tcp_rx_buffer, tcp_tx_buffer);
                $target = $set.add(tcp_socket);
            }
        }
        create_socket!(sockets, tcp_rx_storage0, tcp_tx_storage0, states[0].handle);
        create_socket!(sockets, tcp_rx_storage1, tcp_tx_storage1, states[1].handle);
        create_socket!(sockets, tcp_rx_storage2, tcp_tx_storage2, states[2].handle);
        create_socket!(sockets, tcp_rx_storage3, tcp_tx_storage3, states[3].handle);
        create_socket!(sockets, tcp_rx_storage4, tcp_tx_storage4, states[4].handle);
        create_socket!(sockets, tcp_rx_storage5, tcp_tx_storage5, states[5].handle);
        create_socket!(sockets, tcp_rx_storage6, tcp_tx_storage6, states[6].handle);
        create_socket!(sockets, tcp_rx_storage7, tcp_tx_storage7, states[7].handle);

        for state in &mut states {
            state.state = S::default();
        }

        let mut server = Server {
            states,
            sockets,
            net,
        };
        f(&mut server);
    }

    /// Poll the interface and the sockets
    pub fn poll(&mut self, now: Instant) -> Result<(), smoltcp::Error> {
        // Poll smoltcp EthernetInterface,
        // pass only unexpected smoltcp errors to the caller
        match self.net.poll(&mut self.sockets, now) {
            Ok(_) => Ok(()),
            Err(smoltcp::Error::Malformed) => Ok(()),
            Err(smoltcp::Error::Unrecognized) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Iterate over all sockets managed by this server
    pub fn for_each<F: FnMut(SocketRef<TcpSocket>, &mut S)>(&mut self, mut callback: F) {
        for state in &mut self.states {
            let socket = self.sockets.get::<TcpSocket>(state.handle);
            callback(socket, &mut state.state);
        }
    }
}
