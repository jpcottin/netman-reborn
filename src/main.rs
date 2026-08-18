//! Netman Reborn — Etherman (L2) + Interman (L3), moniteur passif.
//! Jalon 3 : serveur axum + WebSocket diffusant les deltas de graphe.

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};

use netman::agg;
use netman::capture::{self, CaptureStats};
use netman::model::packet::PacketMeta;
use netman::model::tables::Tables;
use netman::server::{self, AppState, IfaceState};
use netman::wsproto::IfaceInfo;

#[derive(Parser, Debug)]
#[command(
    name = "netman",
    about = "Etherman + Interman — passive network monitor"
)]
struct Cli {
    /// Capture interface: index or name substring (interactive prompt if omitted)
    #[arg(long)]
    iface: Option<String>,

    /// Replay a .pcap file instead of capturing live (offline mode)
    #[arg(long, value_name = "FILE", conflicts_with = "iface")]
    pcap_file: Option<PathBuf>,

    /// HTTP/WebSocket listen port
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// HTTP/WebSocket listen address; 0.0.0.0 or :: serves every interface,
    /// over both IPv4 and IPv6
    #[arg(long, default_value = "127.0.0.1")]
    listen: String,

    /// Fade timeout in seconds: nodes/edges unseen for this long are removed
    #[arg(long, default_value_t = 60)]
    fade: u64,

    /// Directory of frontend static files served at /
    #[arg(long, default_value = "static")]
    static_dir: String,
}

fn main() -> anyhow::Result<()> {
    netman::setup_npcap_dll_path();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "netman=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let stats = Arc::new(CaptureStats::default());
    let shutdown = Arc::new(AtomicBool::new(false));

    // Channel capture → agrégateur (découplage, invariant 2).
    let (meta_tx, meta_rx) = mpsc::channel::<PacketMeta>(agg::CHANNEL_CAPACITY);

    // Mode fichier : thread de rejeu simple. Mode live : contrôleur de
    // capture (permet la bascule d'interface depuis le navigateur).
    let mut offline_thread = None;
    let mut controller = None;
    let iface_state = Arc::new(Mutex::new(IfaceState::default()));

    if let Some(file) = cli.pcap_file.clone() {
        println!("Replaying {} (offline mode)...", file.display());
        let stats = Arc::clone(&stats);
        let shutdown = Arc::clone(&shutdown);
        let on_packet = capture::make_on_packet(Arc::clone(&stats), meta_tx.clone());
        offline_thread = Some(
            std::thread::Builder::new()
                .name("pcap-capture".into())
                .spawn(move || capture::run_file(&file, stats, shutdown, on_packet))
                .context("failed to spawn capture thread")?,
        );
    } else {
        let devices = capture::list_devices().context("cannot enumerate capture interfaces")?;
        let device = match &cli.iface {
            Some(wanted) => capture::find_device(&devices, wanted)?,
            None => prompt_device(&devices)?,
        };
        let ctl = Arc::new(capture::Controller::new(
            Arc::clone(&stats),
            meta_tx.clone(),
        ));
        let label = ctl.switch_to(&device.name)?;
        println!("Capturing on: {label} (promiscuous)");
        refresh_iface_state(&ctl, &iface_state);
        controller = Some(ctl);
    }
    drop(meta_tx); // les threads de capture détiennent leurs clones

    // Runtime tokio : agrégateur + serveur HTTP/WS.
    let runtime = tokio::runtime::Runtime::new().context("failed to start tokio runtime")?;
    let result = runtime.block_on(async_main(
        &cli,
        Arc::clone(&stats),
        meta_rx,
        controller.clone(),
        Arc::clone(&iface_state),
    ));

    // Arrêt : flag/stop pour la boucle pcap (réveillée par son timeout), puis
    // join hors runtime (jamais de join bloquant dans une tâche async).
    shutdown.store(true, Ordering::Relaxed);
    drop(runtime);
    if let Some(ctl) = controller {
        ctl.stop();
    }
    if let Some(thread) = offline_thread {
        match thread.join() {
            Ok(capture_result) => capture_result.context("capture failed")?,
            Err(_) => anyhow::bail!("capture thread panicked"),
        }
    }
    tracing::info!(
        frames = stats.frames.load(Ordering::Relaxed),
        kernel_drops = stats.kernel_drops.load(Ordering::Relaxed),
        chan_drops = stats.chan_drops.load(Ordering::Relaxed),
        "netman stopped"
    );
    result
}

/// Rafraîchit l'état partagé interfaces disponibles + interface active.
fn refresh_iface_state(ctl: &capture::Controller, iface_state: &Arc<Mutex<IfaceState>>) {
    let interfaces = capture::list_devices()
        .map(|devices| {
            devices
                .iter()
                .map(|d| IfaceInfo {
                    id: d.name.clone(),
                    label: capture::device_label(d),
                })
                .collect()
        })
        .unwrap_or_default();
    if let Ok(mut state) = iface_state.lock() {
        state.current = ctl.current();
        state.interfaces = interfaces;
    }
}

async fn async_main(
    cli: &Cli,
    stats: Arc<CaptureStats>,
    meta_rx: mpsc::Receiver<PacketMeta>,
    controller: Option<Arc<capture::Controller>>,
    iface_state: Arc<Mutex<IfaceState>>,
) -> anyhow::Result<()> {
    let tables = Arc::new(Mutex::new(Tables::new()));
    let (deltas_tx, _) = broadcast::channel::<String>(agg::BROADCAST_CAPACITY);
    let fade_secs = Arc::new(AtomicU64::new(
        cli.fade
            .clamp(netman::server::FADE_MIN_SECS, netman::server::FADE_MAX_SECS),
    ));

    tokio::spawn(agg::aggregate_loop(
        meta_rx,
        Arc::clone(&tables),
        // Le bureau n'ajoute rien aux deux vues historiques.
        Arc::new(Mutex::new(agg::NoExt)),
        deltas_tx.clone(),
        Arc::clone(&stats),
        Arc::clone(&fade_secs),
    ));

    // Bascule d'interface pilotée depuis le navigateur (mode live seulement).
    let iface_tx = controller.map(|ctl| {
        let (iface_tx, iface_rx) = mpsc::channel::<String>(8);
        tokio::spawn(iface_switch_loop(
            iface_rx,
            ctl,
            Arc::clone(&iface_state),
            Arc::clone(&tables),
            deltas_tx.clone(),
        ));
        iface_tx
    });

    let state = AppState {
        tables,
        deltas_tx,
        fade_secs,
        iface_state,
        iface_tx,
    };
    let app = server::router(state, &cli.static_dir);
    let listeners = bind_listeners(&cli.listen, cli.port).await?;
    let bound: Vec<SocketAddr> = listeners
        .iter()
        .filter_map(|l| l.local_addr().ok())
        .collect();

    let urls = service_urls(&bound, local_addresses);
    match urls.as_slice() {
        [only] => println!("Serving on {only} (Ctrl-C to stop)"),
        several => {
            println!("Serving on (Ctrl-C to stop):");
            for url in several {
                println!("    {url}");
            }
        }
    }
    if cli.listen != "127.0.0.1" {
        println!("Reachable from the network — mind your firewall rules.");
    }

    // Un seul guetteur de Ctrl-C, diffusé à toutes les sockets d'écoute.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down...");
        let _ = shutdown_tx.send(true);
    });

    let mut servers = Vec::with_capacity(listeners.len());
    for listener in listeners {
        let app = app.clone();
        let mut rx = shutdown_rx.clone();
        servers.push(tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.changed().await;
                })
                .await
        }));
    }
    for server in servers {
        match server.await {
            Ok(result) => result.context("http server failed")?,
            Err(err) => anyhow::bail!("http server task failed: {err}"),
        }
    }
    Ok(())
}

/// Applique les demandes de changement d'interface : stop + join de la boucle
/// pcap courante puis démarrage sur la nouvelle carte (hors runtime via
/// spawn_blocking), et rediffusion de l'état interfaces à tous les clients.
async fn iface_switch_loop(
    mut iface_rx: mpsc::Receiver<String>,
    ctl: Arc<capture::Controller>,
    iface_state: Arc<Mutex<IfaceState>>,
    tables: Arc<Mutex<Tables>>,
    deltas_tx: broadcast::Sender<String>,
) {
    while let Some(wanted) = iface_rx.recv().await {
        let ctl_for_switch = Arc::clone(&ctl);
        let switched = tokio::task::spawn_blocking(move || ctl_for_switch.switch_to(&wanted)).await;
        match switched {
            Ok(Ok(label)) => {
                tracing::info!(label, "capture switched by client");
                // Nouvelle interface ⇒ historique vierge (caches DNS gardés).
                server::reset_history(&tables, &deltas_tx);
            }
            Ok(Err(e)) => tracing::warn!(error = %e, "interface switch failed"),
            Err(e) => tracing::error!(error = %e, "interface switch task failed"),
        }
        // L'état diffusé reflète la réalité (échec ⇒ le sélecteur se recale).
        refresh_iface_state(&ctl, &iface_state);
        let message = match iface_state.lock() {
            Ok(state) => server::encode_info(&state.to_message()),
            Err(_) => None,
        };
        if let Some(msg) = message {
            let _ = deltas_tx.send(msg);
        }
    }
}

/// Sélection interactive : liste numérotée, choix au clavier.
fn prompt_device(devices: &[pcap::Device]) -> anyhow::Result<pcap::Device> {
    println!("Available capture interfaces:");
    for (i, dev) in devices.iter().enumerate() {
        println!("  [{i}] {}", capture::device_label(dev));
    }
    loop {
        print!("Interface number: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("failed to read interface choice")?;
        // Chaîne vide (et non pas ligne vide) : stdin est clos, insister
        // reviendrait à boucler indéfiniment.
        if line.is_empty() {
            anyhow::bail!("stdin closed before an interface was chosen");
        }
        match parse_choice(&line, devices.len()) {
            Choice::Selected(i) => return Ok(devices[i].clone()),
            Choice::Again => {}
            Choice::Invalid => println!("Invalid choice, expected 0..{}", devices.len() - 1),
        }
    }
}

/// Ce que le programme doit faire d'une saisie à l'invite de choix.
#[derive(Debug, PartialEq, Eq)]
enum Choice {
    /// Interface retenue, par son index.
    Selected(usize),
    /// Ré-inviter sans rien dire.
    Again,
    /// Saisie fautive : l'expliquer avant de ré-inviter.
    Invalid,
}

/// Interprète une saisie à l'invite de choix d'interface.
///
/// Une ligne vide donne [`Choice::Again`] plutôt qu'une erreur : c'est la
/// frappe accidentelle la plus courante — un retour chariot resté dans le
/// tampon du terminal, par exemple — et la sanctionner d'un message
/// d'erreur laisse croire à un refus alors que rien n'a été saisi.
fn parse_choice(line: &str, count: usize) -> Choice {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Choice::Again;
    }
    match trimmed.parse::<usize>() {
        Ok(i) if i < count => Choice::Selected(i),
        _ => Choice::Invalid,
    }
}

/// Ouvre les sockets d'écoute.
///
/// Une adresse non spécifiée (`0.0.0.0` ou `::`) signifie « toutes les
/// interfaces ». On ouvre alors une socket par famille, afin d'être joignable
/// aussi bien en IPv4 qu'en IPv6 : `bind("0.0.0.0:p")` seul n'écoute qu'en
/// IPv4.
///
/// La socket IPv6 est explicitement forcée en `V6ONLY`. Sans cela le résultat
/// dépendrait de la plateforme — `net.inet6.ip6.v6only` vaut 1 par défaut sur
/// BSD mais 0 sur Linux, où la socket IPv6 capterait aussi l'IPv4 et ferait
/// échouer l'autre `bind`.
///
/// L'ouverture IPv6 est au mieux : sur une machine sans pile IPv6 on continue
/// en IPv4 seul plutôt que d'échouer.
async fn bind_listeners(listen: &str, port: u16) -> anyhow::Result<Vec<TcpListener>> {
    match listen.parse::<IpAddr>() {
        Ok(wildcard) if wildcard.is_unspecified() => {
            let v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
                .await
                .with_context(|| format!("cannot listen on 0.0.0.0:{port}"))?;
            match bind_v6_only(port) {
                Ok(v6) => Ok(vec![v4, v6]),
                Err(err) => {
                    tracing::warn!(%err, "IPv6 listen socket unavailable, serving IPv4 only");
                    Ok(vec![v4])
                }
            }
        }
        _ => {
            let addr = format!("{listen}:{port}");
            let listener = TcpListener::bind(&addr)
                .await
                .with_context(|| format!("cannot listen on {addr}"))?;
            Ok(vec![listener])
        }
    }
}

/// Socket IPv6 d'écoute, `V6ONLY` armé pour cohabiter avec la socket IPv4.
fn bind_v6_only(port: u16) -> anyhow::Result<TcpListener> {
    use socket2::{Domain, Socket, Type};

    let socket = Socket::new(Domain::IPV6, Type::STREAM, None)?;
    socket.set_only_v6(true)?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)).into())?;
    socket.listen(1024)?;
    TcpListener::from_std(socket.into()).context("cannot adopt the IPv6 socket")
}

/// Adresses IP portées par les interfaces locales, telles que les rapporte la
/// couche de capture. Isolée de [`service_urls`] pour que celle-ci reste
/// testable sans dépendre de la configuration réseau de la machine.
fn local_addresses() -> Vec<IpAddr> {
    capture::list_devices()
        .map(|devices| {
            devices
                .iter()
                .flat_map(|dev| dev.addresses.iter().map(|a| a.addr))
                .collect()
        })
        .unwrap_or_default()
}

/// Vrai si l'adresse peut figurer telle quelle dans une URL.
///
/// Les adresses lien-local IPv6 sont écartées : elles exigent un identifiant
/// de zone (`%interface`) que les navigateurs ne gèrent pas de manière fiable,
/// et donneraient donc des URLs inutilisables.
fn usable_in_url(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !v4.is_unspecified() && !v4.is_broadcast(),
        IpAddr::V6(v6) => !v6.is_unspecified() && (v6.segments()[0] & 0xffc0) != 0xfe80,
    }
}

fn url_for(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v4) => format!("http://{v4}:{port}/"),
        // Une adresse IPv6 littérale doit être entre crochets dans une URL.
        IpAddr::V6(v6) => format!("http://[{v6}]:{port}/"),
    }
}

/// URLs auxquelles l'interface web est joignable, déduites des sockets
/// réellement ouvertes.
///
/// Une socket liée à une adresse non spécifiée écoute sur toutes les
/// interfaces de sa famille. Ce n'est pas une adresse de destination — aucun
/// navigateur ne sait joindre `0.0.0.0` — on l'étend donc en une URL par
/// adresse locale de la même famille.
fn service_urls(bound: &[SocketAddr], locals: impl Fn() -> Vec<IpAddr>) -> Vec<String> {
    let mut addrs: Vec<SocketAddr> = Vec::new();
    for sock in bound {
        if sock.ip().is_unspecified() {
            let want_v4 = sock.is_ipv4();
            addrs.extend(
                locals()
                    .into_iter()
                    .filter(|ip| ip.is_ipv4() == want_v4)
                    .filter(usable_in_url)
                    .map(|ip| SocketAddr::new(ip, sock.port())),
            );
        } else {
            addrs.push(*sock);
        }
    }
    if addrs.is_empty() {
        // Rien d'énumérable : au moins une URL utilisable localement.
        addrs = bound
            .iter()
            .map(|sock| {
                let loopback = if sock.is_ipv4() {
                    IpAddr::V4(Ipv4Addr::LOCALHOST)
                } else {
                    IpAddr::V6(Ipv6Addr::LOCALHOST)
                };
                SocketAddr::new(loopback, sock.port())
            })
            .collect();
    }
    addrs.sort();
    addrs.dedup();
    addrs.iter().map(|s| url_for(s.ip(), s.port())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(list: &[&str]) -> Vec<IpAddr> {
        list.iter()
            .map(|s| s.parse().expect("test address"))
            .collect()
    }

    fn sock(addr: &str) -> SocketAddr {
        addr.parse().expect("test socket address")
    }

    fn nothing() -> Vec<IpAddr> {
        Vec::new()
    }

    #[test]
    fn explicit_ipv4_is_rendered_as_is() {
        let urls = service_urls(&[sock("192.168.0.10:8080")], nothing);
        assert_eq!(urls, vec!["http://192.168.0.10:8080/"]);
    }

    #[test]
    fn explicit_ipv6_is_bracketed() {
        let urls = service_urls(&[sock("[2001:db8::1]:8080")], nothing);
        assert_eq!(urls, vec!["http://[2001:db8::1]:8080/"]);
    }

    #[test]
    fn wildcard_never_appears_in_a_url() {
        let locals = || ips(&["192.168.0.10", "2001:db8::1"]);
        let urls = service_urls(&[sock("0.0.0.0:8080"), sock("[::]:8080")], locals);
        for url in &urls {
            assert!(!url.contains("0.0.0.0"), "IPv4 wildcard leaked into {url}");
            assert!(!url.contains("[::]"), "IPv6 wildcard leaked into {url}");
        }
    }

    #[test]
    fn dual_stack_wildcard_lists_both_families() {
        let locals = || ips(&["192.168.0.10", "2001:db8::1"]);
        let urls = service_urls(&[sock("0.0.0.0:8080"), sock("[::]:8080")], locals);
        assert_eq!(
            urls,
            vec!["http://192.168.0.10:8080/", "http://[2001:db8::1]:8080/"]
        );
    }

    #[test]
    fn ipv4_wildcard_alone_ignores_v6_addresses() {
        // Une socket liée a 0.0.0.0 n'accepte pas d'IPv6 : annoncer une URL
        // IPv6 serait mensonger.
        let locals = || ips(&["192.168.0.10", "2001:db8::1"]);
        let urls = service_urls(&[sock("0.0.0.0:8080")], locals);
        assert_eq!(urls, vec!["http://192.168.0.10:8080/"]);
    }

    #[test]
    fn link_local_v6_is_skipped() {
        let locals = || ips(&["fe80::250:56ff:fe86:2dc0", "2001:db8::1"]);
        let urls = service_urls(&[sock("[::]:8080")], locals);
        assert_eq!(urls, vec!["http://[2001:db8::1]:8080/"]);
    }

    #[test]
    fn wildcard_falls_back_to_loopback_when_nothing_is_known() {
        assert_eq!(
            service_urls(&[sock("0.0.0.0:8080")], nothing),
            vec!["http://127.0.0.1:8080/"]
        );
        assert_eq!(
            service_urls(&[sock("[::]:8080")], nothing),
            vec!["http://[::1]:8080/"]
        );
    }

    #[test]
    fn duplicate_addresses_are_reported_once() {
        let locals = || ips(&["192.168.0.10", "192.168.0.10"]);
        assert_eq!(service_urls(&[sock("0.0.0.0:8080")], locals).len(), 1);
    }

    #[test]
    fn the_port_is_carried_over_to_every_url() {
        let locals = || ips(&["192.168.0.10"]);
        let urls = service_urls(&[sock("0.0.0.0:9000")], locals);
        assert_eq!(urls, vec!["http://192.168.0.10:9000/"]);
    }

    #[test]
    fn an_index_in_range_is_selected() {
        assert_eq!(parse_choice("0\n", 2), Choice::Selected(0));
        assert_eq!(parse_choice("1\n", 2), Choice::Selected(1));
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(parse_choice("  1  \n", 2), Choice::Selected(1));
    }

    #[test]
    fn an_empty_line_asks_again_without_complaining() {
        assert_eq!(parse_choice("\n", 2), Choice::Again);
        assert_eq!(parse_choice("   \n", 2), Choice::Again);
        assert_eq!(parse_choice("\t\r\n", 2), Choice::Again);
    }

    #[test]
    fn an_index_out_of_range_is_invalid() {
        assert_eq!(parse_choice("2\n", 2), Choice::Invalid);
        assert_eq!(parse_choice("99\n", 2), Choice::Invalid);
    }

    #[test]
    fn a_non_numeric_answer_is_invalid() {
        assert_eq!(parse_choice("vmx0\n", 2), Choice::Invalid);
        assert_eq!(parse_choice("-1\n", 2), Choice::Invalid);
        assert_eq!(parse_choice("1.5\n", 2), Choice::Invalid);
    }
}
