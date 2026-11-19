use crate::{Phase, Sample, Settings, Snapshot, timeline::State};
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
    Networking::WinSock::{AF_INET6, IN6_ADDR, IN6_ADDR_0, SOCKADDR_IN6, SOCKADDR_IN6_0},
};

static LOOKUPS: AtomicUsize = AtomicUsize::new(0);
static REQUESTS: AtomicUsize = AtomicUsize::new(0);
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
}

struct Shared {
    start: Option<Instant>,
    timeline: State,
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
                loop {
                    let mut state = state_worker.lock().unwrap();
                    loop {
                        match event.take().or_else(|| receive.try_recv().ok()) {
                            Some(Event::Stop) => return,
                            Some(Event::Done(id, outcome)) => state.timeline.complete(id, outcome),
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
                    let wake = state.timeline.next_tick();
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
        state
            .timeline
            .snapshot(state.start.map_or(Duration::ZERO, |start| start.elapsed()))
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
                let destination = SOCKADDR_IN6 {
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
                };
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
    fn native_loopback_and_localhost() {
        for target in ["127.0.0.1", "::1", "localhost"] {
            let address = resolve(target).unwrap();
            assert!(
                address.ip().is_loopback(),
                "{target} resolved outside loopback"
            );
            let result = echo(address, 1000, || true);
            assert!(matches!(result, Sample::Reply(_)), "{target}: {result:?}");
            eprintln!("{target} ({address}): {result:?}");
        }
    }
}
