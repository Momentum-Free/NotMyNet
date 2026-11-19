use crate::{LinkKind, LinkSpeed, Phase, Sample, Settings, Snapshot, timeline::State};
use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    ptr::{null, null_mut},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    NetworkManagement::IpHelper::{
        ICMP_ECHO_REPLY, ICMPV6_ECHO_REPLY_LH, IP_DEST_HOST_UNREACHABLE, IP_DEST_NET_UNREACHABLE,
        IP_DEST_PORT_UNREACHABLE, IP_DEST_PROHIBITED, IP_DEST_UNREACHABLE, IP_REQ_TIMED_OUT,
        IP_SUCCESS, IP_TTL_EXPIRED_REASSEM, IP_TTL_EXPIRED_TRANSIT, Icmp6CreateFile,
        Icmp6SendEcho2, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho,
    },
    NetworkManagement::{
        IpHelper::{
            ConvertInterfaceIndexToLuid, GetBestInterfaceEx, GetIfEntry2, IF_TYPE_ETHERNET_CSMACD,
            IF_TYPE_IEEE80211, MIB_IF_ROW2,
        },
        Ndis::{
            IfOperStatusUp, MediaConnectStateDisconnected, NET_LUID_LH, NdisPhysicalMedium802_3,
            NdisPhysicalMediumNative802_11, NdisPhysicalMediumUnspecified,
            NdisPhysicalMediumWirelessLan,
        },
    },
    Networking::WinSock::{
        AF_INET, AF_INET6, IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, SOCKADDR_IN, SOCKADDR_IN6,
        SOCKADDR_IN6_0,
    },
};

static LOOKUPS: AtomicUsize = AtomicUsize::new(0);
static REQUESTS: AtomicUsize = AtomicUsize::new(0);
static LINK_LOOKUPS: AtomicUsize = AtomicUsize::new(0);
// One target, up to 20 starts/s. Timeout is at most 5s; bound workers even across Stop/Start.
const MAX_IN_FLIGHT: usize = 104;

struct Permit(&'static AtomicUsize);
impl Permit {
    fn take(counter: &'static AtomicUsize, max: usize) -> Option<Self> {
        counter
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < max).then_some(n + 1)
            })
            .ok()
            .map(|_| Self(counter))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

enum Event {
    Stop,
    Done(u64, Sample),
    Link(Instant, Option<LinkSpeed>),
}

struct Shared {
    start: Option<Instant>,
    timeline: State,
    link: Option<(Instant, LinkSpeed)>,
}

pub struct Monitor {
    state: Arc<Mutex<Shared>>,
    events: mpsc::Sender<Event>,
    stopped: Arc<AtomicBool>,
}

impl Monitor {
    pub fn start(target: &str, settings: Settings) -> Result<Self, String> {
        let settings = settings.validate()?;
        let target = validate_target(target)?.to_owned();
        let lookup = Permit::take(&LOOKUPS, 2)
            .ok_or("Previous target lookups are still finishing. Try again shortly.")?;
        let state = Arc::new(Mutex::new(Shared {
            start: None,
            timeline: State::new(settings),
            link: None,
        }));
        let stopped = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stopped);
        let (events, receive) = mpsc::channel();
        let (state_worker, send) = (Arc::clone(&state), events.clone());
        thread::Builder::new()
            .name("icmp-schedule".into())
            .spawn(move || {
                let address = resolve(&target);
                drop(lookup);
                if matches!(receive.try_recv(), Ok(Event::Stop)) {
                    return;
                }
                let address = match address {
                    Ok(address) => address,
                    Err(error) => {
                        state_worker
                            .lock()
                            .unwrap()
                            .timeline
                            .set_phase(Phase::Failed(error));
                        return;
                    }
                };
                let start = Instant::now();
                {
                    let mut state = state_worker.lock().unwrap();
                    state.start = Some(start);
                    state.timeline.set_phase(Phase::Running);
                }
                let mut event = None;
                let mut link_due = (!address.ip().is_loopback()).then_some(start);
                let mut link_in_flight = false;
                loop {
                    let mut state = state_worker.lock().unwrap();
                    loop {
                        match event.take().or_else(|| receive.try_recv().ok()) {
                            Some(Event::Stop) => return,
                            Some(Event::Done(id, outcome)) => state.timeline.complete(id, outcome),
                            Some(Event::Link(sampled_at, link)) => {
                                link_in_flight = false;
                                state.link = link.map(|speed| (sampled_at, speed));
                            }
                            None => break,
                        }
                    }
                    let now = start.elapsed();
                    if stop_worker.load(Ordering::Acquire) {
                        return;
                    }
                    if state.timeline.next_tick() <= now {
                        let permit = Permit::take(&REQUESTS, MAX_IN_FLIGHT);
                        if let (Some(id), Some(permit)) =
                            (state.timeline.tick(now, permit.is_some()), permit)
                        {
                            let send = send.clone();
                            let stopped = Arc::clone(&stop_worker);
                            let send_before = state.timeline.send_before(id);
                            if thread::Builder::new()
                                .stack_size(128 * 1024)
                                .spawn(move || {
                                    let outcome = echo(address, settings.timeout_ms, || {
                                        !stopped.load(Ordering::Acquire)
                                            && start.elapsed() < send_before
                                    });
                                    let _ = send.send(Event::Done(id, outcome));
                                    drop(permit);
                                })
                                .is_err()
                            {
                                state.timeline.complete(
                                    id,
                                    Sample::LocalError("Unable to start ICMP request".into()),
                                );
                            }
                        }
                    }
                    if !link_in_flight && link_due.is_some_and(|at| at <= Instant::now()) {
                        link_due = Some(Instant::now() + Duration::from_secs(2));
                        if let Some(permit) = Permit::take(&LINK_LOOKUPS, 2) {
                            let send = send.clone();
                            let stopped = Arc::clone(&stop_worker);
                            link_in_flight = thread::Builder::new()
                                .name("icmp-link".into())
                                .spawn(move || {
                                    let sampled_at = Instant::now();
                                    let link = if stopped.load(Ordering::Acquire) {
                                        None
                                    } else {
                                        route_link_speed(address)
                                    };
                                    drop(permit);
                                    let _ = send.send(Event::Link(sampled_at, link));
                                })
                                .is_ok();
                        }
                    }
                    let wake = if link_in_flight { None } else { link_due }
                        .map_or(state.timeline.next_tick(), |at| {
                            at.duration_since(start).min(state.timeline.next_tick())
                        });
                    drop(state);
                    match receive.recv_timeout(wake.saturating_sub(start.elapsed())) {
                        Ok(message) => event = Some(message),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            })
            .map_err(|err| format!("Unable to start the monitor: {err}"))?;
        Ok(Self {
            state,
            events,
            stopped,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        let mut snapshot = state
            .timeline
            .snapshot(state.start.map_or(Duration::ZERO, |start| start.elapsed()));
        snapshot.link_speed = state
            .link
            .filter(|(at, _)| at.elapsed() < Duration::from_secs(6))
            .map(|(_, speed)| speed);
        snapshot
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.events.send(Event::Stop);
    }
}

fn validate_target(raw: &str) -> Result<&str, String> {
    let target = raw.trim();
    let hostname = target.strip_suffix('.').unwrap_or(target);
    if target.parse::<IpAddr>().is_ok()
        || (!hostname.is_empty()
            && hostname.len() <= 253
            && hostname.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
    {
        Ok(target)
    } else {
        Err("Enter one IP address or hostname (no URL or port).".into())
    }
}

fn resolve(target: &str) -> Result<SocketAddr, String> {
    let address = |addr: &SocketAddr| match addr.ip() {
        IpAddr::V4(ip) => !ip.is_unspecified() && !ip.is_broadcast() && !ip.is_multicast(),
        IpAddr::V6(ip) => {
            !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unicast_link_local()
                && ip.to_ipv4_mapped().is_none()
        }
    };
    if let Ok(ip) = target.parse::<IpAddr>() {
        let parsed = SocketAddr::new(ip, 0);
        return address(&parsed)
            .then_some(parsed)
            .ok_or_else(|| "Use a unicast address (scoped IPv6 is not supported).".into());
    }
    (target, 0)
        .to_socket_addrs()
        .map_err(|err| format!("Hostname lookup failed: {err}"))?
        .find(address)
        .ok_or_else(|| "Hostname did not return a supported unicast address.".into())
}

fn route_link_speed(address: SocketAddr) -> Option<LinkSpeed> {
    if address.ip().is_loopback() {
        return None;
    }
    let mut row = MIB_IF_ROW2 {
        InterfaceLuid: best_interface(address)?,
        ..Default::default()
    };
    // The row is writable and initialized with the selected interface LUID.
    if unsafe { GetIfEntry2(&mut row) } != 0 {
        return None;
    }
    let speed = link_from_row(&row)?;
    // The route can switch during the query. Drop a result for a displaced adapter.
    let current = best_interface(address)?;
    (unsafe { current.Value == row.InterfaceLuid.Value }).then_some(speed)
}

fn best_interface(address: SocketAddr) -> Option<NET_LUID_LH> {
    let mut index = 0;
    // The family-tagged socket address and output index live through the synchronous call.
    let status = unsafe {
        match address {
            SocketAddr::V4(addr) => {
                let dest = SOCKADDR_IN {
                    sin_family: AF_INET,
                    sin_addr: IN_ADDR {
                        S_un: IN_ADDR_0 {
                            S_addr: u32::from_ne_bytes(addr.ip().octets()),
                        },
                    },
                    ..Default::default()
                };
                GetBestInterfaceEx((&raw const dest).cast(), &mut index)
            }
            SocketAddr::V6(addr) => {
                let dest = sockaddr_v6(addr);
                GetBestInterfaceEx((&raw const dest).cast(), &mut index)
            }
        }
    };
    if status != 0 || index == 0 {
        return None;
    }
    let mut luid = NET_LUID_LH::default();
    (unsafe { ConvertInterfaceIndexToLuid(index, &mut luid) } == 0).then_some(luid)
}

#[allow(non_upper_case_globals)] // Constants are spelled as in the Windows SDK.
fn link_from_row(row: &MIB_IF_ROW2) -> Option<LinkSpeed> {
    // HardwareInterface is the first bit of the SDK's interface flags. Do not report a VPN or
    // virtual interface's nominal link as the physical Ethernet or Wi-Fi negotiation rate.
    const HARDWARE: u8 = 1;
    const NOT_MEDIA_CONNECTED: u8 = 1 << 4;
    const ENDPOINT: u8 = 1 << 7;
    let flags = row.InterfaceAndOperStatusFlags._bitfield;
    if flags & HARDWARE == 0
        || flags & (NOT_MEDIA_CONNECTED | ENDPOINT) != 0
        || row.OperStatus != IfOperStatusUp
        || row.MediaConnectState == MediaConnectStateDisconnected
    {
        return None;
    }
    let kind = match row.PhysicalMediumType {
        NdisPhysicalMediumNative802_11 | NdisPhysicalMediumWirelessLan => LinkKind::Wifi,
        NdisPhysicalMedium802_3 => LinkKind::Ethernet,
        NdisPhysicalMediumUnspecified => match row.Type {
            IF_TYPE_IEEE80211 => LinkKind::Wifi,
            IF_TYPE_ETHERNET_CSMACD => LinkKind::Ethernet,
            _ => LinkKind::Other,
        },
        _ => LinkKind::Other,
    };
    let known = |rate| (rate != 0 && rate != u64::MAX).then_some(rate);
    let speed = LinkSpeed {
        rx_bps: known(row.ReceiveLinkSpeed),
        tx_bps: known(row.TransmitLinkSpeed),
        kind,
    };
    (speed.rx_bps.is_some() || speed.tx_bps.is_some()).then_some(speed)
}

fn sockaddr_v6(addr: std::net::SocketAddrV6) -> SOCKADDR_IN6 {
    SOCKADDR_IN6 {
        sin6_family: AF_INET6,
        sin6_addr: IN6_ADDR {
            u: IN6_ADDR_0 {
                Byte: addr.ip().octets(),
            },
        },
        Anonymous: SOCKADDR_IN6_0 {
            sin6_scope_id: addr.scope_id(),
        },
        ..Default::default()
    }
}

struct EchoHandle(HANDLE);
impl Drop for EchoHandle {
    fn drop(&mut self) {
        unsafe {
            IcmpCloseHandle(self.0);
        }
    }
}

fn echo(address: SocketAddr, timeout: u32, can_send: impl Fn() -> bool) -> Sample {
    if !can_send() {
        return Sample::Skipped;
    }
    // Each bounded worker owns its handle, payload and aligned reply buffer until its synchronous
    // IP Helper call returns. There is no borrowed buffer or callback across the Windows boundary.
    unsafe {
        let h = if address.is_ipv4() {
            IcmpCreateFile()
        } else {
            Icmp6CreateFile()
        };
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            return Sample::LocalError(format!(
                "Could not open Windows ICMP handle ({})",
                GetLastError()
            ));
        }
        let handle = EchoHandle(h);
        let payload = *b"NotMyNet";
        // More than both SDK reply structs + payload + reserved ICMP/error/status space.
        let mut buffer = [0u64; 128];
        let ptr = buffer.as_mut_ptr().cast();
        let len = size_of_val(&buffer) as u32;
        let (status, rtt) = match address {
            SocketAddr::V4(addr) => {
                if !can_send() {
                    return Sample::Skipped;
                }
                let n = IcmpSendEcho(
                    handle.0,
                    u32::from_ne_bytes(addr.ip().octets()),
                    payload.as_ptr().cast(),
                    payload.len() as u16,
                    null(),
                    ptr,
                    len,
                    timeout,
                );
                if n == 0 {
                    return error(GetLastError());
                }
                let reply = &*ptr.cast::<ICMP_ECHO_REPLY>();
                (reply.Status, reply.RoundTripTime)
            }
            SocketAddr::V6(addr) => {
                let source = SOCKADDR_IN6 {
                    sin6_family: AF_INET6,
                    ..Default::default()
                };
                let destination = sockaddr_v6(addr);
                if !can_send() {
                    return Sample::Skipped;
                }
                let n = Icmp6SendEcho2(
                    handle.0,
                    null_mut(),
                    None,
                    null(),
                    &source,
                    &destination,
                    payload.as_ptr().cast(),
                    payload.len() as u16,
                    null(),
                    ptr,
                    len,
                    timeout,
                );
                if n == 0 {
                    return error(GetLastError());
                }
                let reply = &*ptr.cast::<ICMPV6_ECHO_REPLY_LH>();
                (reply.Status, reply.RoundTripTime)
            }
        };
        if status == IP_SUCCESS {
            Sample::Reply(rtt)
        } else {
            error(status)
        }
    }
}

fn error(code: u32) -> Sample {
    match code {
        IP_REQ_TIMED_OUT => Sample::Error("Reply timed out".into()),
        IP_DEST_HOST_UNREACHABLE
        | IP_DEST_NET_UNREACHABLE
        | IP_DEST_PORT_UNREACHABLE
        | IP_DEST_PROHIBITED
        | IP_DEST_UNREACHABLE => Sample::Error("Destination unreachable".into()),
        IP_TTL_EXPIRED_TRANSIT | IP_TTL_EXPIRED_REASSEM => {
            Sample::Error("ICMP hop limit exceeded".into())
        }
        other => Sample::LocalError(format!("Windows ICMP error {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_network_outcomes_count_as_failed_probes() {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            IP_GENERAL_FAILURE, IP_NO_RESOURCES,
        };
        for code in [
            IP_REQ_TIMED_OUT,
            IP_DEST_NET_UNREACHABLE,
            IP_DEST_UNREACHABLE,
            IP_TTL_EXPIRED_TRANSIT,
        ] {
            assert!(matches!(error(code), Sample::Error(_)));
        }
        for code in [IP_NO_RESOURCES, IP_GENERAL_FAILURE] {
            assert!(matches!(error(code), Sample::LocalError(_)));
        }
    }

    #[test]
    fn accepts_only_one_hostname_or_address() {
        for target in ["127.0.0.1", "::1", "localhost", "example.test."] {
            assert_eq!(validate_target(target), Ok(target));
        }
        for target in [
            "",
            "https://example.test",
            "192.0.2.1:80",
            "a..b",
            "a b",
            "-a",
            "ff02::1%3",
        ] {
            assert!(validate_target(target).is_err(), "{target}");
        }
        for target in ["0.0.0.0", "::", "ff02::1", "255.255.255.255"] {
            assert!(resolve(target).is_err(), "{target}");
        }
    }

    #[test]
    fn native_50_ms_cadence_loopback() {
        let settings = Settings {
            interval_ms: 50,
            timeout_ms: 1_000,
            ..Settings::default()
        };
        let monitor = Monitor::start("127.0.0.1", settings).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let snapshot = loop {
            let s = monitor.snapshot();
            if s.session_elapsed >= Duration::from_secs(1) || Instant::now() >= deadline {
                break s;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let slots = (snapshot.session_elapsed.as_millis() / 50 + 1) as u64;
        eprintln!(
            "50 ms at {:?}: {} successful loopback replies in {slots} nominal slots",
            snapshot.session_elapsed, snapshot.total_replies
        );
        assert!(
            slots >= 21 && snapshot.total_replies <= slots && snapshot.total_replies + 3 >= slots,
            "{snapshot:?}"
        );
    }

    #[test]
    fn adapter_rate_mapping_is_directional_and_excludes_virtual_links() {
        let mut row = MIB_IF_ROW2 {
            Type: IF_TYPE_ETHERNET_CSMACD,
            OperStatus: IfOperStatusUp,
            ReceiveLinkSpeed: 2_500_000_000,
            TransmitLinkSpeed: 1_000_000_000,
            ..Default::default()
        };
        assert_eq!(link_from_row(&row), None);
        row.InterfaceAndOperStatusFlags._bitfield = 1;
        assert_eq!(
            link_from_row(&row),
            Some(LinkSpeed {
                rx_bps: Some(2_500_000_000),
                tx_bps: Some(1_000_000_000),
                kind: LinkKind::Ethernet
            })
        );
        for flag in [1 << 4, 1 << 7] {
            row.InterfaceAndOperStatusFlags._bitfield = 1 | flag;
            assert_eq!(link_from_row(&row), None);
        }
        row.InterfaceAndOperStatusFlags._bitfield = 1;
        row.PhysicalMediumType =
            windows_sys::Win32::NetworkManagement::Ndis::NdisPhysicalMediumBluetooth;
        assert_eq!(link_from_row(&row).unwrap().kind, LinkKind::Other);
        row.PhysicalMediumType = NdisPhysicalMediumNative802_11;
        row.TransmitLinkSpeed = u64::MAX;
        assert_eq!(
            link_from_row(&row),
            Some(LinkSpeed {
                rx_bps: Some(2_500_000_000),
                tx_bps: None,
                kind: LinkKind::Wifi
            })
        );
        row.ReceiveLinkSpeed = 0;
        assert_eq!(link_from_row(&row), None);
        row.ReceiveLinkSpeed = 1_000_000_000;
        row.MediaConnectState = MediaConnectStateDisconnected;
        assert_eq!(link_from_row(&row), None);
    }

    #[test]
    fn native_loopback_and_localhost() {
        for target in ["127.0.0.1", "::1", "localhost"] {
            let address = resolve(target).unwrap();
            assert!(
                address.ip().is_loopback(),
                "{target} resolved outside loopback"
            );
            assert_eq!(route_link_speed(address), None);
            let result = echo(address, 1000, || true);
            assert!(matches!(result, Sample::Reply(_)), "{target}: {result:?}");
            eprintln!("{target} ({address}): {result:?}");
        }
    }
}
