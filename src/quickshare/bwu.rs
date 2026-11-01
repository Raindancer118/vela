//! The bandwidth upgrade of a transfer that started over Bluetooth: once it
//! is accepted, we offer the phone our Wi-Fi LAN address
//! (UPGRADE_PATH_AVAILABLE). The phone opens a TCP connection to serve's
//! port and introduces itself on it in plaintext (CLIENT_INTRODUCTION),
//! [`Rendezvous`] hands that connection to the waiting session, which acks
//! the introduction and moves its [`Conn`] over ([`Conn::switch_to`]): same
//! keys, same sequence numbers, no second UKEY2. If the phone does not come,
//! the transfer finishes over Bluetooth. Sequence from google/nearby
//! (Apache-2.0), `connections/implementation/bwu_manager.cc` and
//! `mediums/wifi_lan_bwu_handler.cc`.

use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::conn::Conn;
pub use super::proto::connections::BandwidthUpgradeNegotiationFrame;
use super::proto::connections::bandwidth_upgrade_negotiation_frame::upgrade_path_info::{Medium, WifiHotspotCredentials, WifiLanSocket};
use super::proto::connections::bandwidth_upgrade_negotiation_frame::{ClientIntroductionAck, EventType, UpgradePathInfo};
use super::proto::connections::{V1Frame, v1_frame};
use super::write_plain;

/// How long the phone has to connect over Wi-Fi after the offer.
#[cfg(not(test))]
const ARRIVAL_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(test)]
const ARRIVAL_TIMEOUT: Duration = Duration::from_millis(500);

/// How long a session that stopped waiting still waits for a hand-over
/// that claimed it just before (it is writing the ack).
const CLAIMED_WAIT: Duration = Duration::from_secs(5);

/// `f` as a frame.
pub fn v1(f: BandwidthUpgradeNegotiationFrame) -> V1Frame {
    V1Frame {
        r#type: Some(v1_frame::FrameType::BandwidthUpgradeNegotiation as i32),
        bandwidth_upgrade_negotiation: Some(f),
        ..V1Frame::default()
    }
}

/// A frame with only its event set.
pub fn event(kind: EventType) -> BandwidthUpgradeNegotiationFrame {
    BandwidthUpgradeNegotiationFrame {
        event_type: Some(kind as i32),
        ..BandwidthUpgradeNegotiationFrame::default()
    }
}

/// The offer to join our access point and connect to `ip`:`port` there.
pub fn upgrade_path_available_wifi_hotspot(ap: &crate::wifidirect::Credentials, port: u16) -> BandwidthUpgradeNegotiationFrame {
    BandwidthUpgradeNegotiationFrame {
        upgrade_path_info: Some(UpgradePathInfo {
            medium: Some(Medium::WifiHotspot as i32),
            wifi_hotspot_credentials: Some(WifiHotspotCredentials {
                ssid: Some(ap.ssid.clone()),
                password: Some(ap.passphrase.clone()),
                port: Some(i32::from(port)),
                gateway: Some(ap.ip.to_string()),
                frequency: Some(i32::try_from(ap.frequency).unwrap_or(-1)),
            }),
            supports_client_introduction_ack: Some(true),
            ..UpgradePathInfo::default()
        }),
        ..event(EventType::UpgradePathAvailable)
    }
}

/// The offer to move to Wi-Fi LAN at `ip`:`port`, with the ack after the
/// phone's introduction. Encryption stays on.
pub fn upgrade_path_available_wifi_lan(ip: Ipv4Addr, port: u16) -> BandwidthUpgradeNegotiationFrame {
    BandwidthUpgradeNegotiationFrame {
        upgrade_path_info: Some(UpgradePathInfo {
            medium: Some(Medium::WifiLan as i32),
            wifi_lan_socket: Some(WifiLanSocket {
                ip_address: Some(ip.octets().to_vec()),
                wifi_port: Some(i32::from(port)),
            }),
            supports_client_introduction_ack: Some(true),
            ..UpgradePathInfo::default()
        }),
        ..event(EventType::UpgradePathAvailable)
    }
}

/// Our plaintext answer to the phone's CLIENT_INTRODUCTION.
fn client_introduction_ack() -> V1Frame {
    v1(BandwidthUpgradeNegotiationFrame {
        client_introduction_ack: Some(ClientIntroductionAck {}),
        ..event(EventType::ClientIntroductionAck)
    })
}

/// The endpoint ids (`endpoint_id`, then `last_endpoint_id`) of a phone that
/// introduces itself on a new connection to move a transfer there.
pub fn client_introduction(v1: &V1Frame) -> Option<Vec<String>> {
    let f = v1.bandwidth_upgrade_negotiation.as_ref()?;
    if v1.r#type() != v1_frame::FrameType::BandwidthUpgradeNegotiation || f.event_type() != EventType::ClientIntroduction {
        return None;
    }
    let intro = f.client_introduction.as_ref()?;
    let ids: Vec<String> = [&intro.endpoint_id, &intro.last_endpoint_id]
        .into_iter()
        .flatten()
        .filter(|id| !id.is_empty())
        .cloned()
        .collect();
    (!ids.is_empty()).then_some(ids)
}

/// Connections one waiting session takes. The endpoint id is no secret, so
/// someone else on the LAN may claim it too; the session keeps the one that
/// speaks with its keys ([`Conn::switch_to`]).
const MAX_ARRIVALS: usize = 3;

/// A session's place in a [`Rendezvous`]: which one it is (a newer session
/// of the same phone may take its endpoint id), where its connections go and
/// how many went there.
struct Waiting {
    session: u64,
    arrivals: mpsc::Sender<TcpStream>,
    taken: usize,
}

/// Where sessions wait for the phone's new connection, by its endpoint id.
/// A session takes up to [`MAX_ARRIVALS`] connections that claim it.
#[derive(Clone, Default)]
pub struct Rendezvous(Arc<Mutex<HashMap<String, Waiting>>>);

/// A session waiting in a [`Rendezvous`]; dropping it stops waiting.
struct Expected {
    arrivals: mpsc::Receiver<TcpStream>,
    endpoint: String,
    session: u64,
    rendezvous: Rendezvous,
}

impl Expected {
    /// No connection claims this session from now on.
    fn stop(&self) {
        let mut waiting = self.rendezvous.lock();
        if waiting.get(&self.endpoint).is_some_and(|w| w.session == self.session) {
            waiting.remove(&self.endpoint);
        }
    }
}

impl Drop for Expected {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Rendezvous {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Waiting>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn expect(&self, endpoint: &str) -> Expected {
        static SESSIONS: AtomicU64 = AtomicU64::new(0);
        let session = SESSIONS.fetch_add(1, Ordering::Relaxed);
        let (tx, arrivals) = mpsc::channel();
        self.lock().insert(
            endpoint.to_owned(),
            Waiting {
                session,
                arrivals: tx,
                taken: 0,
            },
        );
        Expected {
            arrivals,
            endpoint: endpoint.to_owned(),
            session,
            rendezvous: self.clone(),
        }
    }

    /// Acks the introduction of the phone with one of the endpoint ids
    /// `ids` on `stream` and hands the stream to the session waiting for it;
    /// false (and nothing written) if none waits or it has
    /// [`MAX_ARRIVALS`] already.
    pub fn hand_over(&self, ids: &[String], stream: TcpStream) -> anyhow::Result<bool> {
        let arrivals = {
            let mut waiting = self.lock();
            let key = ids.iter().find(|id| !id.is_empty() && waiting.contains_key(*id)).cloned();
            match key.and_then(|k| waiting.get_mut(&k)) {
                Some(w) if w.taken < MAX_ARRIVALS => {
                    w.taken += 1;
                    w.arrivals.clone()
                }
                _ => return Ok(false),
            }
        };
        write_plain(&stream, client_introduction_ack())?;
        Ok(arrivals.send(stream).is_ok())
    }
}

/// Opens our access point for a phone that connects to `port`; it stays
/// open while the returned guard lives ([`crate::wifidirect::Group::open`]).
pub type OpenHotspot = Arc<dyn Fn(u16) -> OpenedHotspot + Send + Sync>;
/// The access point's credentials and the guard that keeps it open.
pub type OpenedHotspot = anyhow::Result<(crate::wifidirect::Credentials, Box<dyn Send>)>;

/// What a Bluetooth session can offer: our address (`None`: no network),
/// serve's port, where the phone's connection turns up, and an access point
/// of our own if the helper is installed.
#[derive(Clone)]
pub struct Offer {
    pub ip: Option<Ipv4Addr>,
    pub port: u16,
    pub rendezvous: Rendezvous,
    pub hotspot: Option<OpenHotspot>,
}

/// The mediums the phone can move to (its ConnectionRequest).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mediums {
    pub wifi_lan: bool,
    pub wifi_hotspot: bool,
}

/// A way to Wi-Fi, tried in this order.
#[derive(Debug, PartialEq, Eq)]
enum Path {
    /// The network we are on: works if the phone is on it too.
    Lan(Ipv4Addr),
    /// Our own access point: works without a shared network.
    Hotspot,
}

/// How long the phone has to join our access point and connect.
#[cfg(not(test))]
const HOTSPOT_ARRIVAL_TIMEOUT: Duration = Duration::from_secs(40);
#[cfg(test)]
const HOTSPOT_ARRIVAL_TIMEOUT: Duration = Duration::from_millis(2000);

/// The ways to try for a phone that can move to `mediums`.
fn paths(offer: &Offer, mediums: Mediums) -> Vec<Path> {
    let mut out = Vec::new();
    if let Some(ip) = offer.ip.filter(|_| mediums.wifi_lan) {
        out.push(Path::Lan(ip));
    }
    if offer.hotspot.is_some() && mediums.wifi_hotspot {
        out.push(Path::Hotspot);
    }
    out
}

/// An offered upgrade, until the switch is over or no way is left; then
/// the transfer stays where it is.
pub struct Upgrade {
    offer: Offer,
    endpoint: String,
    paths: std::collections::VecDeque<Path>,
    waiting: Option<(Expected, Instant)>,
    /// The access point is being opened (on a thread: it takes seconds).
    opening: Option<mpsc::Receiver<OpenedHotspot>>,
    /// The phone said it cannot take the current offer.
    refused: bool,
    /// A connection came and the switch began.
    switching: bool,
    /// Our access point, open until the transfer ends.
    hotspot: Option<Box<dyn Send>>,
}

impl Upgrade {
    /// Offers phone `endpoint` (which can move to `mediums`) to move `conn`
    /// to Wi-Fi (after accepting its transfer). `None` when there is no way.
    pub fn start(conn: &mut Conn, offer: &Offer, endpoint: &str, mediums: Mediums) -> anyhow::Result<Option<Upgrade>> {
        if endpoint.is_empty() {
            log::info!("quickshare: the phone has no endpoint id to find it by over Wi-Fi, the transfer stays on Bluetooth");
            return Ok(None);
        }
        let paths = paths(offer, mediums);
        if paths.is_empty() {
            log::info!("quickshare: no way to Wi-Fi with this phone ({mediums:?}), the transfer stays on Bluetooth");
            return Ok(None);
        }
        let mut upgrade = Upgrade {
            offer: offer.clone(),
            endpoint: endpoint.to_owned(),
            paths: paths.into(),
            waiting: None,
            opening: None,
            refused: false,
            switching: false,
            hotspot: None,
        };
        upgrade.next(conn)?;
        Ok(Some(upgrade))
    }

    /// Offers the next way, if any is left.
    fn next(&mut self, conn: &mut Conn) -> anyhow::Result<()> {
        self.refused = false;
        match self.paths.pop_front() {
            Some(Path::Lan(ip)) => {
                let expected = self.offer.rendezvous.expect(&self.endpoint);
                conn.send_upgrade(upgrade_path_available_wifi_lan(ip, self.offer.port))?;
                log::info!("quickshare: offering {} to go on over Wi-Fi at {ip}:{}", self.endpoint, self.offer.port);
                self.waiting = Some((expected, Instant::now() + ARRIVAL_TIMEOUT));
            }
            Some(Path::Hotspot) => {
                let open = self.offer.hotspot.clone().expect("a hotspot path has an opener");
                let port = self.offer.port;
                let (tx, rx) = mpsc::channel();
                std::thread::Builder::new().name("quickshare-hotspot".into()).spawn(move || {
                    let _ = tx.send(open(port));
                })?;
                log::info!("quickshare: opening an access point for {}", self.endpoint);
                self.opening = Some(rx);
            }
            None => log::info!("quickshare: no other way to Wi-Fi, the transfer stays on Bluetooth"),
        }
        Ok(())
    }

    /// Moves `conn` to the connections that claim the phone as they come
    /// (see [`Conn::switch_to`]); goes on to the next way when the phone
    /// says no or does not come in time.
    pub fn poll(&mut self, conn: &mut Conn) -> anyhow::Result<()> {
        if let Some(opening) = &self.opening {
            match opening.try_recv() {
                Ok(Ok((ap, guard))) => {
                    self.opening = None;
                    self.hotspot = Some(guard);
                    let expected = self.offer.rendezvous.expect(&self.endpoint);
                    conn.send_upgrade(upgrade_path_available_wifi_hotspot(&ap, self.offer.port))?;
                    log::info!("quickshare: offering {} our access point {} ({} MHz)", self.endpoint, ap.ssid, ap.frequency);
                    self.waiting = Some((expected, Instant::now() + HOTSPOT_ARRIVAL_TIMEOUT));
                }
                Ok(Err(e)) => {
                    self.opening = None;
                    log::warn!("quickshare: no access point: {e:#}");
                    self.next(conn)?;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.opening = None;
                    self.next(conn)?;
                }
            }
            return Ok(());
        }
        let Some((expected, deadline)) = &self.waiting else {
            return Ok(());
        };
        while let Ok(stream) = expected.arrivals.try_recv() {
            if !self.switching {
                log::info!("quickshare: the transfer goes on over Wi-Fi");
            }
            conn.switch_to(Box::new(stream))?;
            self.switching = true;
        }
        if self.switching {
            if !conn.switching() {
                self.waiting = None;
            }
            return Ok(());
        }
        if !self.refused && Instant::now() < *deadline {
            return Ok(());
        }
        // Stop waiting first, then look once more: a hand-over that claimed
        // this session just before has acked the phone, so its connection is
        // taken (once the ack is written).
        expected.stop();
        let late = if self.refused {
            expected.arrivals.try_recv().ok()
        } else {
            expected.arrivals.recv_timeout(CLAIMED_WAIT).ok()
        };
        self.waiting = None;
        match late {
            Some(stream) => {
                log::info!("quickshare: the transfer goes on over Wi-Fi");
                conn.switch_to(Box::new(stream))?;
                self.switching = true;
            }
            None => {
                log::info!("quickshare: the phone did not connect that way");
                // The access point of a way that failed is no use.
                self.hotspot = None;
                self.next(conn)?;
            }
        }
        Ok(())
    }

    /// A bandwidth upgrade frame from the phone.
    pub fn on_frame(&mut self, f: &BandwidthUpgradeNegotiationFrame) {
        if f.event_type() == EventType::UpgradeFailure && self.waiting.is_some() && !self.switching {
            log::info!("quickshare: the phone cannot go on that way");
            self.refused = true;
        }
    }
}

/// Our IPv4 address for the phone: one of those mDNS announces (interfaces
/// that are up, not loopback or point-to-point), not link-local (the phone
/// refuses those), preferably on the interface of the default route.
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let interfaces = if_addrs::get_if_addrs().ok()?;
    let addrs: Vec<(String, Ipv4Addr)> = interfaces
        .into_iter()
        .filter(|i| i.is_oper_up() && !i.is_p2p() && !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(a) => Some((i.name, a.ip)),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    let routes = fs::read_to_string("/proc/net/route").unwrap_or_default();
    choose_ipv4(&addrs, &default_route_interfaces(&routes))
}

/// The interfaces with an IPv4 default route in `/proc/net/route`, lowest
/// metric first.
fn default_route_interfaces(table: &str) -> Vec<String> {
    const RTF_UP: u32 = 1;
    let mut routes: Vec<(u32, String)> = table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let (name, dest, flags, metric, mask) = (f.first()?, f.get(1)?, f.get(3)?, f.get(6)?, f.get(7)?);
            let flags = u32::from_str_radix(flags, 16).ok()?;
            let default = *dest == "00000000" && *mask == "00000000" && flags & RTF_UP != 0;
            default.then(|| Some((metric.parse().ok()?, (*name).to_owned())))?
        })
        .collect();
    routes.sort();
    routes.into_iter().map(|(_, name)| name).collect()
}

fn choose_ipv4(addrs: &[(String, Ipv4Addr)], preferred: &[String]) -> Option<Ipv4Addr> {
    let usable: Vec<&(String, Ipv4Addr)> = addrs
        .iter()
        .filter(|(_, ip)| !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified())
        .collect();
    preferred
        .iter()
        .find_map(|name| usable.iter().find(|(n, _)| n == name))
        .or_else(|| usable.first())
        .map(|(_, ip)| *ip)
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use crate::quickshare::conn::{Incoming, Link};
    use crate::quickshare::proto::connections::bandwidth_upgrade_negotiation_frame::ClientIntroduction;
    use crate::quickshare::{POLL, read_plain};
    use std::io::{self, Read, Write};
    use std::net::SocketAddrV4;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A link that counts what is written to it.
    pub(crate) struct Counted<L> {
        pub(crate) inner: L,
        pub(crate) written: Arc<AtomicUsize>,
    }

    impl<L: Link> Counted<L> {
        pub(crate) fn new(inner: L) -> (Counted<L>, Arc<AtomicUsize>) {
            let written = Arc::new(AtomicUsize::new(0));
            (
                Counted {
                    inner,
                    written: written.clone(),
                },
                written,
            )
        }
    }

    impl<L: Link> Read for Counted<L> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.inner.read(buf)
        }
    }

    impl<L: Link> Write for Counted<L> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = self.inner.write(buf)?;
            self.written.fetch_add(n, Ordering::SeqCst);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    impl<L: Link> Link for Counted<L> {
        fn reader(&self) -> io::Result<Box<dyn Read + Send>> {
            self.inner.reader()
        }

        fn close(&self) -> io::Result<()> {
            self.inner.close()
        }
    }

    /// The phone's part, as the responder: waits for the offer, connects to
    /// it, introduces itself as `endpoint`, takes the ack and moves `conn`
    /// over. Returns the byte count of the new connection once moved.
    /// The next UPGRADE_PATH_AVAILABLE on `conn` (what the phone gets).
    pub(crate) fn next_offer(conn: &mut Conn) -> UpgradePathInfo {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "no UPGRADE_PATH_AVAILABLE");
            if let Some(Incoming::Upgrade(f)) = conn.recv(POLL).unwrap()
                && f.event_type() == EventType::UpgradePathAvailable
            {
                return f.upgrade_path_info.unwrap();
            }
        }
    }

    /// The phone takes the Wi-Fi LAN offer.
    pub(crate) fn respond(conn: &mut Conn, endpoint: &str) -> Arc<AtomicUsize> {
        let info = next_offer(conn);
        assert_eq!(info.medium(), Medium::WifiLan);
        respond_to(conn, endpoint, info)
    }

    /// The phone connects where `info` says and moves `conn` there.
    pub(crate) fn respond_to(conn: &mut Conn, endpoint: &str, info: UpgradePathInfo) -> Arc<AtomicUsize> {
        let deadline = Instant::now() + Duration::from_secs(10);
        assert!(info.supports_client_introduction_ack());
        let addr = match info.medium() {
            Medium::WifiLan => {
                let socket = info.wifi_lan_socket.unwrap();
                let ip: [u8; 4] = socket.ip_address.unwrap().try_into().unwrap();
                SocketAddrV4::new(ip.into(), u16::try_from(socket.wifi_port.unwrap()).unwrap())
            }
            Medium::WifiHotspot => {
                let ap = info.wifi_hotspot_credentials.unwrap();
                assert!(ap.ssid().starts_with("vela-") && ap.password().len() >= 8, "{ap:?}");
                SocketAddrV4::new(ap.gateway().parse().unwrap(), u16::try_from(ap.port()).unwrap())
            }
            other => panic!("unexpected medium {other:?}"),
        };
        let tcp = TcpStream::connect(addr).unwrap();
        let introduction = BandwidthUpgradeNegotiationFrame {
            client_introduction: Some(ClientIntroduction {
                endpoint_id: Some(endpoint.into()),
                ..Default::default()
            }),
            ..event(EventType::ClientIntroduction)
        };
        write_plain(&tcp, v1(introduction)).unwrap();
        let ack = read_plain(&tcp).unwrap();
        assert_eq!(ack.bandwidth_upgrade_negotiation.unwrap().event_type(), EventType::ClientIntroductionAck);
        let (tcp, written) = Counted::new(tcp);
        conn.switch_to(Box::new(tcp)).unwrap();
        while conn.switching() {
            assert!(Instant::now() < deadline, "the switch did not end");
            assert_ne!(conn.recv(POLL).unwrap(), Some(Incoming::Disconnected));
        }
        written
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{self, Counted, respond};
    use super::*;
    use crate::quickshare::conn::Incoming;
    use crate::quickshare::endpoint::{DeviceKind, EndpointInfo};
    use crate::quickshare::proto::connections::connection_request_frame::Medium as RequestMedium;
    use crate::quickshare::proto::sharing;
    use crate::quickshare::{Decision, Event, Outcome, POLL, Step, close, inbound, outbound, sharing_frame, weave};
    use prost::Message;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    #[test]
    fn upgrade_path_available_wifi_lan() {
        let f = super::upgrade_path_available_wifi_lan(Ipv4Addr::new(192, 168, 1, 23), 47392);
        assert_eq!(f.event_type(), EventType::UpgradePathAvailable);
        // event 1; path info: WIFI_LAN, socket 192.168.1.23:47392, ack.
        let expected = [
            0x08, 0x01, 0x12, 0x10, 0x08, 0x05, 0x1a, 0x0a, 0x0a, 0x04, 192, 168, 1, 23, 0x10, 0xa0, 0xf2, 0x02, 0x48, 0x01,
        ];
        assert_eq!(f.encode_to_vec(), expected);
        let frame = v1(f);
        assert_eq!(frame.r#type(), v1_frame::FrameType::BandwidthUpgradeNegotiation);
    }

    #[test]
    fn client_introduction_names_the_phone() {
        let mut f = event(EventType::ClientIntroduction);
        assert_eq!(client_introduction(&v1(f.clone())), None, "no introduction");
        f.client_introduction = Some(Default::default());
        assert_eq!(client_introduction(&v1(f.clone())), None, "no endpoint id");
        f.client_introduction.as_mut().unwrap().last_endpoint_id = Some("LAST".into());
        assert_eq!(client_introduction(&v1(f.clone())), Some(vec!["LAST".into()]));
        f.client_introduction.as_mut().unwrap().endpoint_id = Some(String::new());
        assert_eq!(client_introduction(&v1(f.clone())), Some(vec!["LAST".into()]), "an empty id is none");
        f.client_introduction.as_mut().unwrap().endpoint_id = Some("PHON".into());
        assert_eq!(client_introduction(&v1(f)), Some(vec!["PHON".into(), "LAST".into()]));
        assert_eq!(client_introduction(&v1(event(EventType::UpgradePathAvailable))), None);
        let ack = client_introduction_ack().bandwidth_upgrade_negotiation.unwrap();
        assert_eq!(ack.event_type(), EventType::ClientIntroductionAck);
    }

    #[test]
    fn the_address_comes_from_the_default_route() {
        let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
                     wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
                     eth0\t00000000\t0100000A\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
                     wlan0\t0001A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n\
                     tun0\t00000000\t00000000\t0000\t0\t0\t50\t00000000\t0\t0\t0\n";
        assert_eq!(default_route_interfaces(table), ["eth0", "wlan0"], "by metric, routes that are up");
        let addrs = vec![
            ("docker0".to_owned(), Ipv4Addr::new(172, 17, 0, 1)),
            ("wlan0".to_owned(), Ipv4Addr::new(192, 168, 1, 23)),
            ("eth0".to_owned(), Ipv4Addr::new(169, 254, 3, 4)),
        ];
        let preferred = default_route_interfaces(table);
        assert_eq!(
            choose_ipv4(&addrs, &preferred),
            Some(Ipv4Addr::new(192, 168, 1, 23)),
            "eth0 only has link-local"
        );
        assert_eq!(choose_ipv4(&addrs, &[]), Some(Ipv4Addr::new(172, 17, 0, 1)), "no default route");
        assert_eq!(choose_ipv4(&addrs[2..], &preferred), None);
    }

    fn me() -> EndpointInfo {
        EndpointInfo {
            kind: DeviceKind::Phone,
            name: Some("Galaxy".into()),
        }
    }

    thread_local! {
        /// The access point opener `session` gives its offer.
        static HOTSPOT: std::cell::RefCell<Option<OpenHotspot>> = const { std::cell::RefCell::new(None) };
    }

    /// An opener whose access point is loopback; `open` counts the open
    /// ones (closed when the guard drops).
    fn fake_hotspot(fails: bool) -> (OpenHotspot, Arc<std::sync::atomic::AtomicIsize>) {
        struct Guard(Arc<std::sync::atomic::AtomicIsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let open = Arc::new(std::sync::atomic::AtomicIsize::new(0));
        let counter = open.clone();
        let opener: OpenHotspot = Arc::new(move |_port| {
            if fails {
                anyhow::bail!("no Wi-Fi card");
            }
            counter.fetch_add(1, Ordering::SeqCst);
            let ap = crate::wifidirect::Credentials {
                iface: "vela-ap".into(),
                ssid: "vela-Test".into(),
                passphrase: "secretsecret".into(),
                frequency: 2437,
                ip: Ipv4Addr::LOCALHOST,
            };
            Ok((ap, Box::new(Guard(counter.clone())) as Box<dyn Send>))
        });
        (opener, open)
    }

    #[test]
    fn the_ways_to_wifi_follow_what_the_phone_can_do() {
        let (hotspot, _) = fake_hotspot(false);
        let mut offer = Offer {
            ip: Some(Ipv4Addr::new(10, 0, 0, 2)),
            port: 1,
            rendezvous: Rendezvous::default(),
            hotspot: Some(hotspot),
        };
        let m = |wifi_lan, wifi_hotspot| Mediums { wifi_lan, wifi_hotspot };
        assert_eq!(paths(&offer, m(true, true)), [Path::Lan(Ipv4Addr::new(10, 0, 0, 2)), Path::Hotspot]);
        assert_eq!(paths(&offer, m(false, true)), [Path::Hotspot]);
        assert_eq!(paths(&offer, m(true, false)), [Path::Lan(Ipv4Addr::new(10, 0, 0, 2))]);
        offer.ip = None;
        assert_eq!(paths(&offer, m(true, true)), [Path::Hotspot], "no network: only our access point");
        offer.hotspot = None;
        assert!(paths(&offer, m(true, true)).is_empty());
    }

    /// The phone's handshake with `mediums` in its ConnectionRequest.
    fn with_mediums<T>(mediums: &[RequestMedium], f: impl FnOnce() -> T) -> T {
        crate::quickshare::outbound::MEDIUMS.set(Some(mediums.to_vec()));
        let r = f();
        crate::quickshare::outbound::MEDIUMS.set(None);
        r
    }

    #[test]
    fn a_phone_that_cannot_reach_us_on_the_lan_joins_our_access_point() {
        let dest = tempfile::tempdir().unwrap();
        let data = data(200 * 1024);
        let (opener, open) = fake_hotspot(false);
        HOTSPOT.set(Some(opener));
        let (ours, phone) = weave::fake::pair(185);
        let s = session(ours);
        HOTSPOT.set(None);
        let mut conn = with_mediums(&[RequestMedium::WifiLan, RequestMedium::WifiHotspot], || {
            offer_and_accept(phone, &s, &data, dest.path())
        });
        send_chunks(&mut conn, &data, 0, data.len() / 2);
        // The LAN offer first; the phone is elsewhere and says no.
        assert_eq!(fake::next_offer(&mut conn).medium(), Medium::WifiLan);
        conn.send_upgrade(event(EventType::UpgradeFailure)).unwrap();
        // Then our access point.
        let info = fake::next_offer(&mut conn);
        assert_eq!(info.medium(), Medium::WifiHotspot);
        assert_eq!(open.load(Ordering::SeqCst), 1);
        let over_tcp = fake::respond_to(&mut conn, "PHON", info);
        send_chunks(&mut conn, &data, data.len() / 2, data.len());
        conn.send_file_chunk(5, data.len() as i64, data.len() as i64, &[], true).unwrap();
        let (outcome, saved) = finished(&s.events);
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(std::fs::read(&saved[0]).unwrap(), data);
        assert!(over_tcp.load(Ordering::SeqCst) > data.len() / 2, "the second half went over the access point");
        close(&mut conn);
        s.thread.join().unwrap().unwrap();
        assert_eq!(open.load(Ordering::SeqCst), 0, "the access point is closed with the transfer");
    }

    #[test]
    fn without_an_access_point_the_transfer_stays_on_bluetooth() {
        let dest = tempfile::tempdir().unwrap();
        let data = data(100 * 1024);
        let (opener, _) = fake_hotspot(true);
        HOTSPOT.set(Some(opener));
        let (ours, phone) = weave::fake::pair(185);
        let s = session(ours);
        HOTSPOT.set(None);
        // The phone cannot use the LAN at all: only the access point is tried, and it fails.
        let mut conn = with_mediums(&[RequestMedium::WifiHotspot], || offer_and_accept(phone, &s, &data, dest.path()));
        send_chunks(&mut conn, &data, 0, data.len());
        conn.send_file_chunk(5, data.len() as i64, data.len() as i64, &[], true).unwrap();
        let (outcome, saved) = finished(&s.events);
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(std::fs::read(&saved[0]).unwrap(), data);
        close(&mut conn);
        s.thread.join().unwrap().unwrap();
    }

    struct Session {
        events: mpsc::Receiver<Event>,
        decisions: mpsc::Sender<Decision>,
        thread: thread::JoinHandle<anyhow::Result<()>>,
    }

    /// `handle_link` on our end of a Bluetooth pair, offering 127.0.0.1 on
    /// a listener whose connections go through `handle_tcp` (serve's path).
    fn session(ours: weave::WeaveLink) -> Session {
        let rendezvous = Rendezvous::default();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let r = rendezvous.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let (events, _) = mpsc::channel();
                let (_, decisions) = mpsc::channel();
                let r = r.clone();
                thread::spawn(move || inbound::handle_tcp(stream.unwrap(), 99, events, decisions, Arc::new(AtomicBool::new(false)), &r));
            }
        });
        let offer = Offer {
            ip: Some(Ipv4Addr::LOCALHOST),
            port,
            rendezvous,
            hotspot: HOTSPOT.with_borrow(Clone::clone),
        };
        let (tx, events) = mpsc::channel();
        let (decisions, rx) = mpsc::channel();
        let thread = thread::spawn(move || inbound::handle_link(ours, 1, tx, rx, Arc::new(AtomicBool::new(false)), Some(offer)));
        Session { events, decisions, thread }
    }

    fn finished(events: &mpsc::Receiver<Event>) -> (Outcome, Vec<std::path::PathBuf>) {
        loop {
            if let Event::Finished { outcome, saved, .. } = events.recv_timeout(Duration::from_secs(20)).unwrap() {
                return (outcome, saved);
            }
        }
    }

    /// The phone's handshake over Bluetooth, an introduction for one file of
    /// `data.len()` bytes and our accept.
    fn offer_and_accept(phone: impl crate::quickshare::conn::Link, s: &Session, data: &[u8], dest: &std::path::Path) -> Conn {
        let Step::Next(mut conn) = outbound::connect_link(phone, &me(), *b"PHON", &AtomicBool::new(false), |_| {}).unwrap() else {
            panic!("handshake ended early");
        };
        let intro = sharing::IntroductionFrame {
            file_metadata: vec![sharing::FileMetadata {
                name: Some("clip.bin".into()),
                payload_id: Some(5),
                size: Some(data.len() as i64),
                ..Default::default()
            }],
            ..Default::default()
        };
        conn.send_sharing(&sharing_frame(sharing::V1Frame {
            r#type: Some(sharing::v1_frame::FrameType::Introduction as i32),
            introduction: Some(intro),
            ..Default::default()
        }))
        .unwrap();
        loop {
            if let Event::Incoming(offer) = s.events.recv_timeout(Duration::from_secs(10)).unwrap() {
                assert_eq!(offer.device, "Galaxy");
                break;
            }
        }
        s.decisions.send(Decision::Accept(dest.to_path_buf())).unwrap();
        assert_eq!(outbound::await_response(&mut conn, &AtomicBool::new(false)).unwrap(), Step::Next(()));
        conn
    }

    fn send_chunks(conn: &mut Conn, data: &[u8], from: usize, to: usize) {
        for start in (from..to).step_by(16 * 1024) {
            let end = (start + 16 * 1024).min(to);
            conn.send_file_chunk(5, data.len() as i64, start as i64, &data[start..end], false).unwrap();
        }
    }

    fn data(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    #[test]
    fn upgrade_moves_session_to_tcp_keeping_counters() {
        let dest = tempfile::tempdir().unwrap();
        let data = data(300 * 1024);
        let (ours, phone) = weave::fake::pair(185);
        let (phone, over_ble) = Counted::new(phone);
        let s = session(ours);
        let mut conn = offer_and_accept(phone, &s, &data, dest.path());

        // Half over Bluetooth, then the upgrade, the rest over TCP. Every
        // frame on either link is encrypted with the one key pair and
        // sequence: a counter starting over would fail to decrypt.
        send_chunks(&mut conn, &data, 0, data.len() / 2);
        let ble_before = over_ble.load(Ordering::SeqCst);
        let over_tcp = respond(&mut conn, "PHON");
        let ble_switch = over_ble.load(Ordering::SeqCst) - ble_before;
        assert!(
            ble_switch < 4096,
            "only the switch frames (and keep-alives) went over Bluetooth meanwhile: {ble_switch}"
        );
        send_chunks(&mut conn, &data, data.len() / 2, data.len());
        conn.send_file_chunk(5, data.len() as i64, data.len() as i64, &[], true).unwrap();

        let (outcome, saved) = finished(&s.events);
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(std::fs::read(&saved[0]).unwrap(), data);
        assert!(over_tcp.load(Ordering::SeqCst) > data.len() / 2, "the second half went over TCP");
        assert_eq!(over_ble.load(Ordering::SeqCst) - ble_before, ble_switch, "and nothing more over Bluetooth");
        // Our DISCONNECTION comes over TCP.
        let deadline = Instant::now() + Duration::from_secs(5);
        while conn.recv(POLL).unwrap() != Some(Incoming::Disconnected) {
            assert!(Instant::now() < deadline, "no DISCONNECTION");
        }
        drop(conn);
        s.thread.join().unwrap().unwrap();
    }

    fn falls_back(say_no: bool) {
        let dest = tempfile::tempdir().unwrap();
        let data = data(100 * 1024);
        let (ours, phone) = weave::fake::pair(185);
        let s = session(ours);
        let mut conn = offer_and_accept(phone, &s, &data, dest.path());
        send_chunks(&mut conn, &data, 0, data.len() / 2);
        if say_no {
            conn.send_upgrade(event(EventType::UpgradeFailure)).unwrap();
        } else {
            // Past the time the phone had to connect.
            let until = Instant::now() + ARRIVAL_TIMEOUT * 2;
            while Instant::now() < until {
                conn.recv(POLL).unwrap();
            }
        }
        send_chunks(&mut conn, &data, data.len() / 2, data.len());
        conn.send_file_chunk(5, data.len() as i64, data.len() as i64, &[], true).unwrap();
        let (outcome, saved) = finished(&s.events);
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(std::fs::read(&saved[0]).unwrap(), data);
        close(&mut conn);
        s.thread.join().unwrap().unwrap();
    }

    #[test]
    fn without_wifi_the_transfer_finishes_over_bluetooth() {
        falls_back(false);
    }

    #[test]
    fn a_phone_that_cannot_switch_finishes_over_bluetooth() {
        falls_back(true);
    }

    /// Connections over loopback: the phone's end and ours.
    struct Connections(TcpListener);

    impl Connections {
        fn new() -> Connections {
            Connections(TcpListener::bind("127.0.0.1:0").unwrap())
        }

        fn accept(&self) -> (TcpStream, TcpStream) {
            let client = TcpStream::connect(self.0.local_addr().unwrap()).unwrap();
            client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            (client, self.0.accept().unwrap().0)
        }
    }

    fn ack(client: &TcpStream) {
        let ack = crate::quickshare::read_plain(client).unwrap();
        assert_eq!(ack.bandwidth_upgrade_negotiation.unwrap().event_type(), EventType::ClientIntroductionAck);
    }

    /// The phone's end gets nothing and is closed.
    fn no_ack(mut client: TcpStream) {
        assert_eq!(std::io::Read::read(&mut client, &mut [0; 8]).unwrap(), 0, "closed without an ack");
    }

    #[test]
    fn a_waiting_session_takes_a_few_connections() {
        // Whoever names the phone's endpoint id gets an ack; the session
        // keeps the one that speaks with the session keys (see Conn).
        let rendezvous = Rendezvous::default();
        let connections = Connections::new();
        let expected = rendezvous.expect("PHON");
        let phone = ["PHON".to_owned()];
        for _ in 0..MAX_ARRIVALS {
            let (client, server) = connections.accept();
            assert!(rendezvous.hand_over(&phone, server).unwrap());
            ack(&client);
        }
        let (extra, server) = connections.accept();
        assert!(!rendezvous.hand_over(&phone, server).unwrap(), "no more than {MAX_ARRIVALS}");
        no_ack(extra);
        assert_eq!(expected.arrivals.try_iter().count(), MAX_ARRIVALS);
    }

    #[test]
    fn another_phone_leaves_a_waiting_session_alone() {
        let rendezvous = Rendezvous::default();
        let connections = Connections::new();
        let expected = rendezvous.expect("PHON");
        let (other, server) = connections.accept();
        assert!(!rendezvous.hand_over(&["OTHR".to_owned()], server).unwrap());
        no_ack(other);
        let (empty, server) = connections.accept();
        assert!(!rendezvous.hand_over(&[String::new()], server).unwrap(), "an empty id matches nothing");
        no_ack(empty);
        assert!(expected.arrivals.try_recv().is_err());

        let (client, server) = connections.accept();
        assert!(rendezvous.hand_over(&["PHON".to_owned()], server).unwrap(), "still waiting");
        ack(&client);
        assert!(expected.arrivals.try_recv().is_ok());
    }

    #[test]
    fn no_upgrade_without_an_endpoint_id() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let keys = crate::quickshare::ukey2::Keys {
            auth: [0; 32],
            encrypt: [1; 32],
            decrypt: [2; 32],
            send_hmac: [3; 32],
            recv_hmac: [4; 32],
        };
        let mut conn = Conn::new(Box::new(ours), &keys).unwrap();
        let offer = Offer {
            ip: Some(Ipv4Addr::LOCALHOST),
            port: 1,
            rendezvous: Rendezvous::default(),
            hotspot: None,
        };
        let both = Mediums {
            wifi_lan: true,
            wifi_hotspot: true,
        };
        assert!(Upgrade::start(&mut conn, &offer, "", both).unwrap().is_none());
        assert!(offer.rendezvous.lock().is_empty(), "nothing registered");
        drop(conn);
        assert_eq!(std::io::Read::read(&mut peer, &mut [0; 8]).unwrap(), 0, "no offer went out");
    }

    #[test]
    fn only_a_waiting_session_gets_the_connection() {
        let rendezvous = Rendezvous::default();
        let connections = Connections::new();
        let accept = || connections.accept();
        let phone = ["PHON".to_owned()];
        let (mut client, server) = accept();
        assert!(!rendezvous.hand_over(&phone, server).unwrap());
        assert_eq!(std::io::Read::read(&mut client, &mut [0; 8]).unwrap(), 0, "closed without an ack");

        let expected = rendezvous.expect("PHON");
        let (client, server) = accept();
        assert!(rendezvous.hand_over(&["OTHR".to_owned(), "PHON".to_owned()], server).unwrap());
        let ack = crate::quickshare::read_plain(&client).unwrap();
        assert_eq!(ack.bandwidth_upgrade_negotiation.unwrap().event_type(), EventType::ClientIntroductionAck);
        assert!(expected.arrivals.try_recv().is_ok());

        drop(expected);
        let (_client, server) = accept();
        assert!(!rendezvous.hand_over(&phone, server).unwrap(), "no longer waiting");
    }
}
