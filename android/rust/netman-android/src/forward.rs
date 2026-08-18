//! Moteur de forwarding : la contrepartie obligée d'un VpnService — les
//! paquets détournés vers le tun doivent être RÉELLEMENT acheminés, sinon le
//! téléphone est hors ligne. `ipstack` termine les flux TCP/UDP côté tun ;
//! chaque flux est relayé vers une vraie socket, marquée `protect()` (elle
//! échappe au tun — ceinture et bretelles, `addDisallowedApplication` exclut
//! déjà l'application de son propre VPN).
//!
//! Le forwarding est ISOLÉ du chemin de capture : il consomme une copie des
//! paquets (voir `tun.rs`) et sa mort n'arrête ni la lecture ni les vues.
//!
//! Non géré pour l'instant : ICMP (echo) et les protocoles sans transport —
//! journalisés puis ignorés. Les vérifications de connectivité d'Android
//! passent en HTTP(S), le téléphone reste utilisable.

use std::io;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{ready, Context, Poll};

use ipstack::{IpStack, IpStackConfig, IpStackStream, IpStackTcpStream, IpStackUdpStream};
use netman::model::packet;
use netman::stats::CaptureStats;
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpSocket, UdpSocket};
use tokio::sync::mpsc;

use crate::NetmanCallbacks;

/// Délai d'inactivité des sessions UDP côté stack.
const UDP_TIMEOUT_SECS: u64 = 60;

/// Adaptateur « périphérique tun » pour `IpStack` :
/// - lecture = les paquets sortants copiés par le thread `tun-read` ;
/// - écriture = les réponses du réseau, TEE vers le chemin de capture (c'est
///   ainsi que le sens entrant est compté : le thread de lecture ne voit que
///   le sens sortant) puis écrites sur le tun (`O_NONBLOCK` + `AsyncFd`).
struct TunDevice {
    stack_rx: mpsc::Receiver<Vec<u8>>,
    write_fd: AsyncFd<SharedFd>,
    meta_tx: mpsc::Sender<netman::model::packet::PacketMeta>,
    stats: Arc<CaptureStats>,
}

/// Le descripteur tun est partagé entre le thread de lecture et ce module.
struct SharedFd(Arc<OwnedFd>);

impl AsRawFd for SharedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl AsyncRead for TunDevice {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.stack_rx.poll_recv(cx) {
            Poll::Ready(Some(pkt)) => {
                let n = pkt.len().min(buf.remaining());
                buf.put_slice(&pkt[..n]);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())), // EOF : session arrêtée
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for TunDevice {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Tee du sens entrant vers le chemin de capture, AVANT l'écriture :
        // même comptabilité que le sens sortant (frames/bytes/chan_drops).
        self.stats
            .frames
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stats
            .bytes
            .fetch_add(buf.len() as u64, std::sync::atomic::Ordering::Relaxed);
        match packet::parse_ip_packet(buf, buf.len() as u32) {
            Some(meta) => {
                if self.meta_tx.try_send(meta).is_err() {
                    self.stats
                        .chan_drops
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            None => {
                self.stats
                    .parse_errors
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }

        loop {
            let mut guard = ready!(self.write_fd.poll_write_ready(cx))?;
            match guard.try_io(|inner| {
                // SAFETY : buf est valide et sa longueur exacte est transmise.
                let n = unsafe { libc::write(inner.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(())) // une écriture tun est atomique, rien à vider
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Boucle du moteur : construit le stack sur l'adaptateur puis relaie chaque
/// flux accepté. Tourne jusqu'à l'arrêt de la session (fermeture des
/// channels) ou une erreur du stack — la capture continue dans les deux cas.
pub async fn run_forwarder(
    fd: Arc<OwnedFd>,
    mtu: u16,
    stack_rx: mpsc::Receiver<Vec<u8>>,
    meta_tx: mpsc::Sender<netman::model::packet::PacketMeta>,
    stats: Arc<CaptureStats>,
    callbacks: Arc<dyn NetmanCallbacks>,
) {
    let write_fd = match AsyncFd::new(SharedFd(fd)) {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!(error = %e, "cannot register tun fd with the reactor");
            return;
        }
    };
    let device = TunDevice {
        stack_rx,
        write_fd,
        meta_tx,
        stats,
    };

    let mut config = IpStackConfig::default();
    config.mtu_unchecked(mtu);
    config.udp_timeout(std::time::Duration::from_secs(UDP_TIMEOUT_SECS));
    let mut stack = IpStack::new(config, device);

    loop {
        match stack.accept().await {
            Ok(IpStackStream::Tcp(tcp)) => {
                let cb = Arc::clone(&callbacks);
                tokio::spawn(handle_tcp(tcp, cb));
            }
            Ok(IpStackStream::Udp(udp)) => {
                let cb = Arc::clone(&callbacks);
                tokio::spawn(handle_udp(udp, cb));
            }
            Ok(IpStackStream::UnknownTransport(unknown)) => {
                tracing::debug!(proto = ?unknown.ip_protocol(), dst = %unknown.dst_addr(),
                    "unhandled transport, dropped");
            }
            Ok(IpStackStream::UnknownNetwork(pkt)) => {
                tracing::debug!(len = pkt.len(), "unparseable packet, dropped");
            }
            Err(e) => {
                tracing::info!(error = %e, "ip stack stopped");
                return;
            }
        }
    }
}

/// Marque la socket `protect()` (appel Kotlin, sous `spawn_blocking`).
/// Un échec n'est pas fatal : l'application est déjà exclue de son VPN.
async fn protect(fd: RawFd, cb: &Arc<dyn NetmanCallbacks>) {
    let cb = Arc::clone(cb);
    let protected = tokio::task::spawn_blocking(move || cb.protect_socket(fd))
        .await
        .unwrap_or(false);
    if !protected {
        tracing::debug!(
            fd,
            "protect_socket refused (relying on addDisallowedApplication)"
        );
    }
}

async fn handle_tcp(mut tcp: IpStackTcpStream, cb: Arc<dyn NetmanCallbacks>) {
    let peer = tcp.peer_addr();
    let connect = async {
        let socket = match peer {
            SocketAddr::V4(_) => TcpSocket::new_v4(),
            SocketAddr::V6(_) => TcpSocket::new_v6(),
        }?;
        protect(socket.as_raw_fd(), &cb).await;
        socket.connect(peer).await
    };
    match connect.await {
        Ok(mut remote) => {
            // Fin de flux, RST ou erreur : on referme les deux côtés.
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut remote).await;
        }
        Err(e) => {
            tracing::debug!(%peer, error = %e, "tcp connect failed");
        }
    }
    let _ = tcp.shutdown().await;
}

async fn handle_udp(mut udp: IpStackUdpStream, cb: Arc<dyn NetmanCallbacks>) {
    let peer = udp.peer_addr();
    let bind: SocketAddr = match peer {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().expect("adresse fixe"),
        SocketAddr::V6(_) => "[::]:0".parse().expect("adresse fixe"),
    };
    let socket = match UdpSocket::bind(bind).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(%peer, error = %e, "udp bind failed");
            return;
        }
    };
    protect(socket.as_raw_fd(), &cb).await;
    if let Err(e) = socket.connect(peer).await {
        tracing::debug!(%peer, error = %e, "udp connect failed");
        return;
    }

    // Relais dans les deux sens ; l'inactivité est tranchée par le timeout
    // UDP du stack (le flux côté tun rend alors 0 → fin de tâche).
    let mut up = vec![0u8; 65536];
    let mut down = vec![0u8; 65536];
    loop {
        tokio::select! {
            read = udp.read(&mut up) => match read {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if socket.send(&up[..n]).await.is_err() {
                        return;
                    }
                }
            },
            recv = socket.recv(&mut down) => match recv {
                Ok(n) => {
                    if udp.write_all(&down[..n]).await.is_err() {
                        return;
                    }
                }
                Err(_) => return,
            },
        }
    }
}
