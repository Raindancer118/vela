//! A minimal DHCP server for the Wi-Fi Direct group: the phone that joins
//! asks for an address (DISCOVER, REQUEST) and gets one from a small pool
//! (OFFER, ACK). No router or DNS is handed out: the group only carries the
//! transfer. RFC 2131/2132, just the parts a phone uses.

use std::collections::HashMap;
use std::net::Ipv4Addr;

const BOOTREQUEST: u8 = 1;
const BOOTREPLY: u8 = 2;
const MAGIC: [u8; 4] = [99, 130, 83, 99];
/// Fixed BOOTP header up to the magic cookie.
const HEADER: usize = 236;

const OPT_PAD: u8 = 0;
const OPT_SUBNET: u8 = 1;
const OPT_REQUESTED_IP: u8 = 50;
const OPT_LEASE: u8 = 51;
const OPT_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
const OPT_END: u8 = 255;

const DISCOVER: u8 = 1;
const OFFER: u8 = 2;
const REQUEST: u8 = 3;
const DECLINE: u8 = 4;
const ACK: u8 = 5;
const NAK: u8 = 6;
const RELEASE: u8 = 7;

/// An hour; the group lives for one transfer anyway.
const LEASE_SECS: u32 = 3600;

/// What a client sent, as far as the server cares.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    kind: u8,
    xid: [u8; 4],
    flags: [u8; 2],
    chaddr: [u8; 16],
    requested: Option<Ipv4Addr>,
    /// The server the client chose (REQUEST after an OFFER).
    server: Option<Ipv4Addr>,
    ciaddr: Ipv4Addr,
}

fn parse(packet: &[u8]) -> Option<Request> {
    if packet.len() < HEADER + MAGIC.len() || packet[0] != BOOTREQUEST || packet[HEADER..HEADER + 4] != MAGIC {
        return None;
    }
    let ip = |b: &[u8]| Ipv4Addr::new(b[0], b[1], b[2], b[3]);
    let (mut kind, mut requested, mut server) = (None, None, None);
    let mut opts = &packet[HEADER + 4..];
    while let Some((&code, rest)) = opts.split_first() {
        match code {
            OPT_PAD => {
                opts = rest;
                continue;
            }
            OPT_END => break,
            _ => {}
        }
        // An option running past the end stops parsing; what came before counts.
        let Some((&len, rest)) = rest.split_first() else { break };
        let Some(value) = rest.get(..len as usize) else { break };
        match (code, value.len()) {
            (OPT_TYPE, 1) => kind = Some(value[0]),
            (OPT_REQUESTED_IP, 4) => requested = Some(ip(value)),
            (OPT_SERVER_ID, 4) => server = Some(ip(value)),
            _ => {}
        }
        opts = &rest[len as usize..];
    }
    Some(Request {
        kind: kind?,
        xid: packet[4..8].try_into().ok()?,
        flags: packet[10..12].try_into().ok()?,
        chaddr: packet[28..44].try_into().ok()?,
        requested,
        server,
        ciaddr: ip(&packet[12..16]),
    })
}

/// Hands out addresses from a pool, the same one per client hardware address.
pub struct Server {
    me: Ipv4Addr,
    mask: Ipv4Addr,
    pool: Vec<Ipv4Addr>,
    leases: HashMap<[u8; 16], Ipv4Addr>,
}

impl Server {
    /// `me` serves `first..=last`, which must be in its subnet `mask`.
    pub fn new(me: Ipv4Addr, mask: Ipv4Addr, first: Ipv4Addr, last: Ipv4Addr) -> Server {
        let pool = (u32::from(first)..=u32::from(last)).map(Ipv4Addr::from).filter(|a| *a != me).collect();
        Server {
            me,
            mask,
            pool,
            leases: HashMap::new(),
        }
    }

    /// The answer to `packet`, if any.
    pub fn handle(&mut self, packet: &[u8]) -> Option<Vec<u8>> {
        let req = parse(packet)?;
        match req.kind {
            DISCOVER => {
                let ip = self.lease_for(&req)?;
                Some(self.reply(&req, OFFER, ip))
            }
            REQUEST => {
                // Answering another server's offer is not ours to do.
                if req.server.is_some_and(|s| s != self.me) {
                    return None;
                }
                let wanted = req.requested.unwrap_or(req.ciaddr);
                match self.lease_for(&req) {
                    Some(ip) if ip == wanted => Some(self.reply(&req, ACK, ip)),
                    _ => Some(self.reply(&req, NAK, Ipv4Addr::UNSPECIFIED)),
                }
            }
            RELEASE | DECLINE => {
                self.leases.remove(&req.chaddr);
                None
            }
            _ => None,
        }
    }

    /// The client's address: its lease, else the address it asks for if
    /// free, else the first free one.
    fn lease_for(&mut self, req: &Request) -> Option<Ipv4Addr> {
        if let Some(ip) = self.leases.get(&req.chaddr) {
            return Some(*ip);
        }
        let taken: Vec<Ipv4Addr> = self.leases.values().copied().collect();
        let free = |ip: &Ipv4Addr| self.pool.contains(ip) && !taken.contains(ip);
        let ip = req.requested.filter(free).or_else(|| self.pool.iter().copied().find(free))?;
        self.leases.insert(req.chaddr, ip);
        Some(ip)
    }

    fn reply(&self, req: &Request, kind: u8, yiaddr: Ipv4Addr) -> Vec<u8> {
        let mut p = vec![0u8; HEADER];
        p[0] = BOOTREPLY;
        p[1] = 1; // Ethernet
        p[2] = 6;
        p[4..8].copy_from_slice(&req.xid);
        p[10..12].copy_from_slice(&req.flags);
        p[16..20].copy_from_slice(&yiaddr.octets());
        p[20..24].copy_from_slice(&self.me.octets());
        p[28..44].copy_from_slice(&req.chaddr);
        p.extend(MAGIC);
        p.extend([OPT_TYPE, 1, kind]);
        p.extend([OPT_SERVER_ID, 4]);
        p.extend(self.me.octets());
        if kind != NAK {
            p.extend([OPT_LEASE, 4]);
            p.extend(LEASE_SECS.to_be_bytes());
            p.extend([OPT_SUBNET, 4]);
            p.extend(self.mask.octets());
        }
        p.push(OPT_END);
        // Some clients drop replies shorter than a BOOTP packet.
        p.resize(p.len().max(300), 0);
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC_A: [u8; 6] = [0x02, 0xaa, 0, 0, 0, 1];
    const MAC_B: [u8; 6] = [0x02, 0xbb, 0, 0, 0, 2];
    const ME: Ipv4Addr = Ipv4Addr::new(192, 168, 49, 1);

    fn server() -> Server {
        Server::new(
            ME,
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(192, 168, 49, 2),
            Ipv4Addr::new(192, 168, 49, 3),
        )
    }

    fn request(kind: u8, mac: [u8; 6], opts: &[(u8, &[u8])]) -> Vec<u8> {
        let mut p = vec![0u8; HEADER];
        p[0] = BOOTREQUEST;
        p[1] = 1;
        p[2] = 6;
        p[4..8].copy_from_slice(&[1, 2, 3, 4]);
        p[10] = 0x80; // broadcast
        p[28..34].copy_from_slice(&mac);
        p.extend(MAGIC);
        p.extend([OPT_TYPE, 1, kind]);
        for (code, value) in opts {
            p.push(*code);
            p.push(value.len() as u8);
            p.extend(*value);
        }
        p.push(OPT_END);
        p
    }

    /// The message type and offered address of a reply.
    fn answer(p: &[u8]) -> (u8, Ipv4Addr) {
        assert_eq!(p[0], BOOTREPLY);
        assert_eq!(&p[4..8], &[1, 2, 3, 4], "same transaction");
        assert_eq!(p[10], 0x80, "flags echoed");
        assert_eq!(&p[HEADER..HEADER + 4], &MAGIC);
        assert_eq!(&p[HEADER + 4..HEADER + 7], &[OPT_TYPE, 1, p[HEADER + 6]]);
        (p[HEADER + 6], Ipv4Addr::new(p[16], p[17], p[18], p[19]))
    }

    #[test]
    fn discover_request_ack() {
        let mut s = server();
        let (kind, offered) = answer(&s.handle(&request(DISCOVER, MAC_A, &[])).unwrap());
        assert_eq!((kind, offered), (OFFER, Ipv4Addr::new(192, 168, 49, 2)));
        let ack = s
            .handle(&request(
                REQUEST,
                MAC_A,
                &[(OPT_REQUESTED_IP, &offered.octets()), (OPT_SERVER_ID, &ME.octets())],
            ))
            .unwrap();
        assert_eq!(answer(&ack), (ACK, offered));
        let opts = &ack[HEADER + 4..];
        assert!(opts.windows(6).any(|w| w == [OPT_SUBNET, 4, 255, 255, 255, 0]));
        assert!(opts.windows(6).any(|w| w == [OPT_SERVER_ID, 4, 192, 168, 49, 1]));
        assert!(ack.len() >= 300);
    }

    #[test]
    fn each_client_keeps_its_own_address() {
        let mut s = server();
        let a = answer(&s.handle(&request(DISCOVER, MAC_A, &[])).unwrap()).1;
        let b = answer(&s.handle(&request(DISCOVER, MAC_B, &[])).unwrap()).1;
        assert_ne!(a, b);
        assert_eq!(answer(&s.handle(&request(DISCOVER, MAC_A, &[])).unwrap()).1, a);
        // The pool has two addresses: a third client gets none.
        assert!(s.handle(&request(DISCOVER, [2, 0xcc, 0, 0, 0, 3], &[])).is_none());
        // Released, it is free again.
        s.handle(&request(RELEASE, MAC_A, &[]));
        assert!(s.handle(&request(DISCOVER, [2, 0xcc, 0, 0, 0, 3], &[])).is_some());
    }

    #[test]
    fn a_request_for_a_wrong_address_is_refused() {
        let mut s = server();
        let nak = s.handle(&request(REQUEST, MAC_A, &[(OPT_REQUESTED_IP, &[10, 0, 0, 7])])).unwrap();
        assert_eq!(answer(&nak).0, NAK);
        // The requested address is honoured when it is free and in the pool.
        let mut s = server();
        let want = Ipv4Addr::new(192, 168, 49, 3);
        let ack = s.handle(&request(REQUEST, MAC_A, &[(OPT_REQUESTED_IP, &want.octets())])).unwrap();
        assert_eq!(answer(&ack), (ACK, want));
    }

    #[test]
    fn a_request_for_another_server_is_ignored() {
        let mut s = server();
        assert!(s.handle(&request(REQUEST, MAC_A, &[(OPT_SERVER_ID, &[192, 168, 49, 99])])).is_none());
    }

    #[test]
    fn garbage_is_ignored() {
        let mut s = server();
        assert!(s.handle(&[]).is_none());
        assert!(s.handle(&[1; 300]).is_none());
        let mut truncated = request(DISCOVER, MAC_A, &[]);
        truncated.truncate(HEADER + 6);
        assert!(s.handle(&truncated).is_none());
        let mut bad_len = request(DISCOVER, MAC_A, &[]);
        bad_len.extend([OPT_REQUESTED_IP, 200]);
        let end = bad_len.iter().rposition(|b| *b == OPT_END).unwrap();
        bad_len.remove(end);
        assert!(s.handle(&bad_len).is_some(), "an option running past the end stops parsing, the type was read");
    }
}
