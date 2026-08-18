//! Test d'intégration sur l'hôte (macOS/Linux) : la session complète —
//! lecture « tun » (un pipe), agrégation, attribution, deltas par callback —
//! sans matériel Android. Les callbacks Kotlin sont simulés.
#![cfg(unix)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use etherparse::PacketBuilder;
use netman_android::{
    capture_stats, reset, snapshot, start, stop, AppLabel, NetmanCallbacks, NetmanError,
    SessionConfig,
};

/// Callbacks simulés : mémorisent les deltas, attribuent tout à l'uid 4242.
#[derive(Default)]
struct MockCallbacks {
    deltas: Mutex<Vec<String>>,
}

impl NetmanCallbacks for MockCallbacks {
    fn on_deltas(&self, mut deltas: Vec<String>) {
        if let Ok(mut all) = self.deltas.lock() {
            all.append(&mut deltas);
        }
    }
    fn protect_socket(&self, _fd: i32) -> bool {
        true
    }
    fn lookup_uid(
        &self,
        _ip_number: u8,
        _local: String,
        _local_port: u16,
        _remote: String,
        _remote_port: u16,
    ) -> i32 {
        4242
    }
    fn resolve_app(&self, uid: i32) -> Option<AppLabel> {
        Some(AppLabel {
            label: format!("MockApp {uid}"),
            package: "com.example.mock".into(),
        })
    }
}

impl MockCallbacks {
    fn wait_for(&self, needle: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(all) = self.deltas.lock() {
                if all.iter().any(|d| d.contains(needle)) {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }
}

/// Un datagramme IPv4/TCP nu, comme lu sur le tun (poste → 203.0.113.7:443).
fn raw_https_packet(payload_len: usize) -> Vec<u8> {
    let mut packet = Vec::new();
    PacketBuilder::ipv4([10, 111, 222, 1], [203, 0, 113, 7], 64)
        .tcp(51000, 443, 1, 64240)
        .write(&mut packet, &vec![0xAA; payload_len])
        .expect("build raw packet");
    packet
}

#[test]
fn full_session_over_a_socketpair() {
    // Un socketpair datagramme joue le rôle du tun : chaque write est un
    // datagramme (comme un tun), et les écritures du moteur de forwarding
    // (RST du stack, etc.) ont un débouché.
    let mut fds = [0i32; 2];
    // SAFETY : fds pointe vers deux c_int valides pendant l'appel.
    assert_eq!(
        unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, fds.as_mut_ptr()) },
        0
    );
    let (read_fd, write_fd) = (fds[0], fds[1]);

    let callbacks = Arc::new(MockCallbacks::default());
    let config = SessionConfig {
        tun_fd: read_fd,
        fade_secs: 60,
        mtu: 1500,
        tun_addrs: vec!["10.111.222.1".into(), "fd00:6e6d::1".into()],
        dns_servers: vec![],
    };
    start(config, Arc::clone(&callbacks) as Arc<dyn NetmanCallbacks>).expect("start");

    // Deux paquets, écrits séparément (un write = un read côté lecteur, qui
    // draine le pipe au fil de l'eau).
    let packet = raw_https_packet(400);
    for _ in 0..2 {
        // SAFETY : le tampon est valide et sa longueur exacte est transmise.
        let n = unsafe { libc::write(write_fd, packet.as_ptr().cast(), packet.len()) };
        assert_eq!(n as usize, packet.len());
        std::thread::sleep(Duration::from_millis(100));
    }

    // La vue Interman voit la conversation, la vue Appman l'attribue à 4242,
    // et le label demandé à la plateforme finit par arriver.
    let timeout = Duration::from_secs(5);
    assert!(
        callbacks.wait_for(r#""view":"inter","id":"203.0.113.7""#, timeout),
        "nœud Interman absent des deltas"
    );
    assert!(
        callbacks.wait_for(r#""view":"app","id":"app:4242""#, timeout),
        "nœud application absent des deltas"
    );
    assert!(
        callbacks.wait_for(r#""label":"MockApp 4242""#, timeout),
        "label d'application jamais résolu"
    );

    // Compteurs et snapshot cohérents. Au moins nos 2 paquets injectés ; le
    // moteur de forwarding peut en avoir ajouté (RST en réponse à un flux TCP
    // sans SYN) — leur présence prouve le tee du sens entrant.
    let stats = capture_stats().expect("stats");
    assert!(stats.frames >= 2, "frames = {}", stats.frames);
    assert_eq!(stats.parse_errors, 0);
    let snap = snapshot().expect("snapshot");
    assert!(snap[0].contains(r#""type":"config""#), "préambule config");
    assert!(snap.iter().any(|d| d.contains(r#""view":"app""#)));
    assert!(snap.iter().any(|d| d.contains(r#""view":"inter""#)));
    // Vue Etherman vide par construction : aucun delta "ether".
    assert!(snap.iter().all(|d| !d.contains(r#""view":"ether""#)));

    // Reset : notification explicite, puis tables vides au snapshot.
    reset().expect("reset");
    assert!(
        callbacks.wait_for(r#"{"type":"reset"}"#, timeout),
        "notification de reset absente"
    );
    let snap = snapshot().expect("snapshot after reset");
    assert!(snap.iter().all(|d| !d.contains("upsert_")));

    // Arrêt idempotent.
    stop().expect("stop");
    assert!(matches!(stop(), Err(NetmanError::NotRunning)));

    // SAFETY : write_fd nous appartient encore (le read_fd est fermé par la
    // session).
    unsafe { libc::close(write_fd) };
}
