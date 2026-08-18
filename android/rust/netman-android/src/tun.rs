//! Lecture du descripteur tun : un thread OS dédié, bloquant, découplé du
//! reste (invariant 2 — même gabarit que `capture::run_file` côté bureau).
//!
//! Android crée le tun en `IFF_TUN | IFF_NO_PI` : chaque `read()` rend
//! exactement un datagramme IP, sans préfixe. `poll()` avec timeout sert de
//! point de contrôle du flag d'arrêt, comme le timeout pcap du bureau.
//!
//! Chaque paquet lu est projeté deux fois (le « tee ») :
//! - parse → `meta_tx` : le chemin de capture, jamais bloqué ;
//! - copie → `stack_tx` : l'entrée du moteur de forwarding. File pleine ⇒ le
//!   paquet est perdu POUR LE STACK SEULEMENT (TCP retransmettra) — le
//!   forwarding ne ralentit jamais la capture (invariant 5).
//!
//! Code POSIX pur : les tests d'intégration sur l'hôte (macOS/Linux)
//! alimentent le même chemin via un `socketpair()`.

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use netman::model::packet::{self, PacketMeta};
use netman::stats::CaptureStats;
use tokio::sync::mpsc;

/// Période du poll : borne le délai de réaction à l'arrêt.
const POLL_TIMEOUT_MS: i32 = 500;
/// Un datagramme tun tient dans la MTU ; 64 Kio couvrent tout MTU réaliste.
const READ_BUF_LEN: usize = 65536;

/// Boucle de lecture. À exécuter sur un thread OS dédié (`tun-read`).
/// S'arrête quand `shutdown` passe à vrai ou que le descripteur se ferme.
/// Le descripteur est partagé avec le moteur de forwarding (qui écrit) ;
/// il se ferme quand le dernier des deux lâche son `Arc`.
pub fn run_tun(
    fd: Arc<OwnedFd>,
    stats: Arc<CaptureStats>,
    shutdown: Arc<AtomicBool>,
    meta_tx: mpsc::Sender<PacketMeta>,
    stack_tx: mpsc::Sender<Vec<u8>>,
) {
    let raw_fd = fd.as_raw_fd();
    let mut buf = vec![0u8; READ_BUF_LEN];

    loop {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }
        let mut pollfd = libc::pollfd {
            fd: raw_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY : pollfd pointe vers une structure valide pendant l'appel.
        let ready = unsafe { libc::poll(&mut pollfd, 1, POLL_TIMEOUT_MS) };
        if ready < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            tracing::error!(error = %err, "tun poll failed");
            return;
        }
        if ready == 0 {
            continue; // timeout : re-tester le flag d'arrêt
        }
        if pollfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
            && pollfd.revents & libc::POLLIN == 0
        {
            tracing::info!("tun closed, stopping reader");
            return;
        }
        // SAFETY : buf est valide et sa longueur exacte est transmise.
        let n = unsafe { libc::read(raw_fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted
                || err.kind() == std::io::ErrorKind::WouldBlock
            {
                continue;
            }
            tracing::error!(error = %err, "tun read failed");
            return;
        }
        if n == 0 {
            tracing::info!("tun EOF, stopping reader");
            return;
        }
        let data = &buf[..n as usize];
        stats.frames.fetch_add(1, Ordering::Relaxed);
        stats.bytes.fetch_add(n as u64, Ordering::Relaxed);

        // (a) chemin de capture : parse → try_send, jamais bloquant.
        match packet::parse_ip_packet(data, n as u32) {
            Some(meta) => {
                if meta_tx.try_send(meta).is_err() {
                    stats.chan_drops.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => {
                stats.parse_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        // (b) forwarding : copie du paquet vers le stack. Échec (file pleine,
        // moteur arrêté) = paquet perdu pour le forwarding seulement.
        let _ = stack_tx.try_send(data.to_vec());
    }
    // Le dernier Arc<OwnedFd> lâché ferme le descripteur (transféré par le
    // Kotlin via `ParcelFileDescriptor.detachFd()`).
}
