//! The LAN transport: `std::net::UdpSocket`, non-blocking, one `NetMsg` per
//! datagram (wire format in `wire.rs`).
//!
//! The host binds `0.0.0.0:port` and adopts the first address whose packet
//! decodes as a `Hello`; every other address is ignored from then on. The
//! joiner sends to the address it dialled and ignores anyone else.
//!
//! Lobby messages that are sent once and must not be lost (`Pick`, `Rules`,
//! `Start`, `Checksum`) go as `Frame::Reliable`: numbered, resent every
//! `RELIABLE_RESEND` until the peer acks them, and handed up exactly once
//! and in order (like `Loopback`). Everything else (`Hello`, `Inputs`,
//! `Ping`, `Pong`, `Bye`) is sent once: the session repeats those itself.

use super::wire::{self, Frame, MAX_PACKET};
use super::{NetMsg, Transport, DEFAULT_PORT};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// How often an unacknowledged reliable message is sent again.
pub const RELIABLE_RESEND: Duration = Duration::from_millis(150);
/// Reliable messages further ahead than this of the next expected one are
/// dropped (the sender resends them later).
const REORDER_WINDOW: u32 = 256;
/// Most datagrams read per `recv` (keeps one frame from stalling on a flood).
const MAX_READS: usize = 4096;

pub struct Udp {
    sock: UdpSocket,
    peer: Option<SocketAddr>,
    next_seq: u32,
    /// Reliable messages sent but not yet acked: (seq, datagram, last sent).
    unacked: VecDeque<(u32, Vec<u8>, Option<Instant>)>,
    /// Next reliable seq to hand up, and those that arrived early.
    recv_next: u32,
    recv_early: BTreeMap<u32, NetMsg>,
    /// Test hook: drop about one in N outgoing datagrams (0 = never).
    drop_every: usize,
    sent: usize,
}

impl Udp {
    /// Listen on `0.0.0.0:port` (`port` 0 = any free port, for tests).
    pub fn host(port: u16) -> io::Result<Udp> {
        Self::open(UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?, None)
    }

    /// Talk to the host at `addr`: `ip`, `ip:port`, `name` or `name:port`
    /// (port 7777 when left out).
    pub fn join(addr: &str) -> io::Result<Udp> {
        let peer = resolve(addr)?;
        let any: IpAddr = if peer.is_ipv4() { Ipv4Addr::UNSPECIFIED.into() } else { Ipv6Addr::UNSPECIFIED.into() };
        Self::open(UdpSocket::bind((any, 0))?, Some(peer))
    }

    fn open(sock: UdpSocket, peer: Option<SocketAddr>) -> io::Result<Udp> {
        sock.set_nonblocking(true)?;
        Ok(Udp { sock, peer, next_seq: 0, unacked: VecDeque::new(), recv_next: 0, recv_early: BTreeMap::new(), drop_every: 0, sent: 0 })
    }

    /// The UDP port this end is bound to.
    pub fn local_port(&self) -> u16 {
        self.sock.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// The other end, once known (the host learns it from the first Hello).
    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Test hook: lose about one in `n` datagrams this end sends (0 = none).
    /// Pseudo-random rather than strictly periodic, so a resend can't keep
    /// landing on the dropped slot.
    pub fn set_test_loss(&mut self, n: usize) {
        self.drop_every = n;
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        assert!(bytes.len() < MAX_PACKET, "net datagram too big ({} bytes)", bytes.len());
        let Some(peer) = self.peer else { return };
        self.sent += 1;
        if self.drop_every > 0 {
            let h = (self.sent as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
            if h % self.drop_every as u64 == 0 {
                return;
            }
        }
        // WouldBlock (send buffer full) and ConnectionReset (peer's port
        // closed, Windows) just lose this datagram, like the network could.
        let _ = self.sock.send_to(bytes, peer);
    }

    fn flush_reliable(&mut self, now: Instant) {
        if self.peer.is_none() {
            return;
        }
        let due: Vec<Vec<u8>> = self
            .unacked
            .iter_mut()
            .filter(|(_, _, last)| last.is_none_or(|t| now.duration_since(t) >= RELIABLE_RESEND))
            .map(|(_, bytes, last)| {
                *last = Some(now);
                bytes.clone()
            })
            .collect();
        for b in due {
            self.send_raw(&b);
        }
    }

    fn on_frame(&mut self, frame: Frame, out: &mut Vec<NetMsg>) {
        match frame {
            Frame::Plain(m) => out.push(m),
            Frame::Ack(seq) => self.unacked.retain(|(s, _, _)| *s != seq),
            Frame::Reliable { seq, msg } => {
                self.send_raw(&wire::encode_frame(&Frame::Ack(seq)));
                let ahead = seq.wrapping_sub(self.recv_next);
                if ahead == 0 {
                    out.push(msg);
                    self.recv_next = self.recv_next.wrapping_add(1);
                    while let Some(m) = self.recv_early.remove(&self.recv_next) {
                        out.push(m);
                        self.recv_next = self.recv_next.wrapping_add(1);
                    }
                } else if ahead < REORDER_WINDOW {
                    self.recv_early.insert(seq, msg);
                }
                // Otherwise a duplicate of one already handed up: acked, dropped.
            }
        }
    }
}

pub(crate) fn is_reliable(msg: &NetMsg) -> bool {
    matches!(msg, NetMsg::Pick(_) | NetMsg::Rules(_) | NetMsg::Start { .. } | NetMsg::Checksum { .. })
}

impl Transport for Udp {
    fn send(&mut self, msg: &NetMsg) {
        if is_reliable(msg) {
            let seq = self.next_seq;
            self.next_seq = self.next_seq.wrapping_add(1);
            self.unacked.push_back((seq, wire::encode_frame(&Frame::Reliable { seq, msg: msg.clone() }), None));
            self.flush_reliable(Instant::now());
        } else {
            let bytes = wire::encode(msg);
            // The app may close right after Bye: send it a few times.
            let copies = if *msg == NetMsg::Bye { 3 } else { 1 };
            for _ in 0..copies {
                self.send_raw(&bytes);
            }
        }
    }

    fn recv(&mut self) -> Vec<NetMsg> {
        let mut out = Vec::new();
        let mut buf = [0u8; 2048];
        for _ in 0..MAX_READS {
            let (n, from) = match self.sock.recv_from(&mut buf) {
                Ok(r) => r,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // Windows reports an ICMP "port unreachable" from an earlier
                // send as WSAECONNRESET on the next receive; an oversized
                // datagram is WSAEMSGSIZE (10040). Neither ends the session.
                Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::Interrupted) || e.raw_os_error() == Some(10040) => continue,
                Err(_) => break,
            };
            let Some(frame) = wire::decode_frame(&buf[..n]) else { continue };
            match self.peer {
                Some(p) if p == from => {}
                Some(_) => continue,
                None => {
                    if !matches!(frame, Frame::Plain(NetMsg::Hello { .. })) {
                        continue;
                    }
                    self.peer = Some(from);
                }
            }
            self.on_frame(frame, &mut out);
        }
        self.flush_reliable(Instant::now());
        out
    }

    fn local_port(&self) -> Option<u16> {
        Some(Udp::local_port(self))
    }
}

/// `ip`, `ip:port`, `[v6]:port`, `name`, `name:port` -> an address (IPv4
/// preferred), port `DEFAULT_PORT` when none is given.
pub fn resolve(addr: &str) -> io::Result<SocketAddr> {
    let addr = addr.trim();
    if addr.is_empty() {
        return Err(io::Error::new(ErrorKind::InvalidInput, "No address entered"));
    }
    let list: Vec<SocketAddr> = match addr.to_socket_addrs() {
        Ok(l) => l.collect(),
        Err(_) => (addr.trim_start_matches('[').trim_end_matches(']'), DEFAULT_PORT)
            .to_socket_addrs()
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, format!("Can't find address \"{addr}\"")))?
            .collect(),
    };
    list.iter()
        .find(|a| a.is_ipv4())
        .or(list.first())
        .copied()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, format!("Can't find address \"{addr}\"")))
}

/// This PC's LAN address as other PCs see it (for "tell your friend this
/// address" on the host screen). No packet is sent: connecting a UDP socket
/// only picks the outgoing interface.
pub fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    s.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_forms() {
        assert_eq!(resolve("127.0.0.1").unwrap(), "127.0.0.1:7777".parse().unwrap());
        assert_eq!(resolve(" 10.0.0.5:9000 ").unwrap(), "10.0.0.5:9000".parse().unwrap());
        assert_eq!(resolve("::1").unwrap(), "[::1]:7777".parse().unwrap());
        assert_eq!(resolve("[::1]:5").unwrap(), "[::1]:5".parse().unwrap());
        assert_eq!(resolve("localhost").unwrap().port(), 7777);
        assert!(resolve("").is_err());
        assert!(resolve("1.2.3.4:notaport").is_err());
    }

    #[test]
    fn host_adopts_first_hello_and_ignores_others() {
        let mut host = Udp::host(0).unwrap();
        let addr = format!("127.0.0.1:{}", host.local_port());
        let mut a = Udp::join(&addr).unwrap();
        let mut b = Udp::join(&addr).unwrap();
        // Before any Hello the host knows no one; a non-Hello is ignored.
        a.send(&NetMsg::Ping(1));
        wait(|| host.recv().is_empty());
        assert_eq!(host.peer(), None);
        a.send(&NetMsg::Hello { build: "x".into(), data_hash: 1 });
        let got = until(|| host.recv());
        assert!(matches!(got[0], NetMsg::Hello { .. }));
        assert_eq!(host.peer().unwrap().port(), a.local_port());
        b.send(&NetMsg::Hello { build: "y".into(), data_hash: 2 });
        b.send(&NetMsg::Ping(5));
        a.send(&NetMsg::Ping(7));
        let got = until(|| host.recv());
        assert_eq!(got, vec![NetMsg::Ping(7)], "only the adopted peer is heard");
        // Garbage from anyone is dropped quietly.
        let junk = UdpSocket::bind("127.0.0.1:0").unwrap();
        junk.send_to(b"hello there", ("127.0.0.1", host.local_port())).unwrap();
        host.send(&NetMsg::Pong(3));
        assert_eq!(until(|| a.recv()), vec![NetMsg::Pong(3)]);
        wait(|| host.recv().is_empty());
    }

    #[test]
    fn reliable_messages_arrive_once_in_order_despite_loss() {
        let mut host = Udp::host(0).unwrap();
        let mut join = Udp::join(&format!("127.0.0.1:{}", host.local_port())).unwrap();
        join.send(&NetMsg::Hello { build: String::new(), data_hash: 0 });
        until(|| host.recv());
        host.set_test_loss(2);
        join.set_test_loss(3);
        let sent: Vec<NetMsg> = (0..40).map(|k| NetMsg::Checksum { tick: k, hash: k * 7 }).collect();
        for m in &sent {
            host.send(m);
        }
        let mut got = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while got.len() < sent.len() && Instant::now() < deadline {
            got.extend(join.recv());
            host.recv();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(got, sent);
        // Acks drain the resend queue.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !host.unacked.is_empty() && Instant::now() < deadline {
            join.recv();
            host.recv();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(host.unacked.is_empty());
    }

    #[test]
    fn closed_peer_port_does_not_break_receiving() {
        // Windows: sending to a closed port makes the next recv fail with
        // WSAECONNRESET. That must read as "nothing yet", not an error.
        let gone = UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let mut j = Udp::join(&gone.to_string()).unwrap();
        for _ in 0..5 {
            j.send(&NetMsg::Ping(1));
            std::thread::sleep(Duration::from_millis(5));
            assert!(j.recv().is_empty());
        }
    }

    /// Poll until `f` returns something (2 s cap).
    fn until(mut f: impl FnMut() -> Vec<NetMsg>) -> Vec<NetMsg> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let v = f();
            if !v.is_empty() || Instant::now() > deadline {
                return v;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Give packets 30 ms to land, then check `f`.
    fn wait(f: impl FnOnce() -> bool) {
        std::thread::sleep(Duration::from_millis(30));
        assert!(f());
    }
}
