//! Agrégateur : consomme les `PacketMeta` par lots, maintient les tables,
//! diffuse les deltas des entrées modifiées à chaque tick.
//!
//! Extrait de `main.rs` pour être partagé entre le binaire bureau (deltas vers
//! le WebSocket axum) et la bibliothèque Android (deltas vers un callback) :
//! la boucle est identique, seuls la source des paquets et le puits des
//! deltas changent. Le point d'extension [`AggExt`] permet à une plateforme
//! d'ajouter une agrégation supplémentaire (la vue « Appman » d'Android) sans
//! toucher au comportement du bureau ([`NoExt`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc};

use crate::model::packet::PacketMeta;
use crate::model::tables::Tables;
use crate::resolve;
use crate::stats::CaptureStats;
use crate::wsproto::{self, Delta};

/// Capacité du channel capture→agrégateur. Plein ⇒ on jette (invariant 5).
pub const CHANNEL_CAPACITY: usize = 65536;
/// Capacité du broadcast des deltas (ring ; les clients lents « laggent »).
pub const BROADCAST_CAPACITY: usize = 4096;
/// Période du tick de diffusion.
pub const TICK: Duration = Duration::from_millis(250);

/// Agrégation supplémentaire branchée sur la même boucle (invariant 1 : les
/// vues sont des projections du même flux, jamais une seconde capture).
///
/// Les méthodes sont appelées par l'agrégateur sous son propre rythme :
/// `ingest` pour chaque paquet du lot, `tick` à chaque tick (drain des deltas
/// modifiés + vieillissement, comme `Tables`). `snapshot`/`reset` sont
/// exposées pour les clients qui arrivent ou repartent de zéro.
pub trait AggExt: Send + 'static {
    fn ingest(&mut self, meta: &PacketMeta, now: Instant);
    fn tick(&mut self, now: Instant, max_age: Duration) -> Vec<Delta>;
    fn snapshot(&self) -> Vec<Delta>;
    fn reset(&mut self);
    /// Nom d'hôte résolu (PTR) : la vue Appman affiche les mêmes hôtes
    /// qu'Interman, elle profite des mêmes résolutions.
    fn host_resolved(&mut self, _ip: std::net::IpAddr, _name: &str) {}
}

/// Extension vide : le bureau n'ajoute rien aux deux tables historiques.
pub struct NoExt;

impl AggExt for NoExt {
    fn ingest(&mut self, _meta: &PacketMeta, _now: Instant) {}
    fn tick(&mut self, _now: Instant, _max_age: Duration) -> Vec<Delta> {
        Vec::new()
    }
    fn snapshot(&self) -> Vec<Delta> {
        Vec::new()
    }
    fn reset(&mut self) {}
}

/// Boucle d'agrégation. Tourne sur le runtime tokio ; ne bloque jamais la
/// capture (elle consomme un channel alimenté en `try_send`).
pub async fn aggregate_loop<E: AggExt>(
    mut meta_rx: mpsc::Receiver<PacketMeta>,
    tables: Arc<Mutex<Tables>>,
    ext: Arc<Mutex<E>>,
    deltas_tx: broadcast::Sender<String>,
    stats: Arc<CaptureStats>,
    fade_secs: Arc<AtomicU64>,
) {
    let mut buf: Vec<PacketMeta> = Vec::with_capacity(4096);
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut capture_done = false;
    let mut last_log = Instant::now();

    // Résolveur PTR : demandes déclenchées une seule fois par IP (les échecs
    // d'envoi — file pleine — seront retentés au tick suivant).
    let (ptr_results_tx, mut ptr_results_rx) =
        mpsc::channel::<(std::net::IpAddr, String)>(resolve::dns::REQUEST_QUEUE);
    let ptr_req_tx = resolve::dns::spawn(ptr_results_tx);
    let mut ptr_requested: std::collections::HashSet<std::net::IpAddr> =
        std::collections::HashSet::new();

    loop {
        tokio::select! {
            biased;
            received = meta_rx.recv_many(&mut buf, 4096), if !capture_done => {
                if received == 0 {
                    // Fin de capture (fichier rejoué) : on continue à servir.
                    capture_done = true;
                    continue;
                }
                let Ok(mut tables) = tables.lock() else { return };
                let Ok(mut ext) = ext.lock() else { return };
                let now = Instant::now();
                for meta in buf.drain(..) {
                    tables.ingest(&meta, now);
                    ext.ingest(&meta, now);
                }
            }
            resolved = ptr_results_rx.recv() => {
                let Some((ip, name)) = resolved else { return };
                {
                    let Ok(mut ext) = ext.lock() else { return };
                    ext.host_resolved(ip, &name);
                }
                let Ok(mut tables) = tables.lock() else { return };
                tables.set_l3_label(ip, name);
            }
            _ = tick.tick() => {
                let max_age = Duration::from_secs(fade_secs.load(Ordering::Relaxed));
                let (mut deltas, dirty_ips) = {
                    let Ok(mut tables) = tables.lock() else { return };
                    let dirty_ips = tables.dirty_l3_node_ips();
                    let mut deltas = tables.drain_deltas();
                    // Vieillissement : suppressions explicites (invariant 7).
                    deltas.extend(tables.fade_sweep(Instant::now(), max_age));
                    (deltas, dirty_ips)
                };
                {
                    let Ok(mut ext) = ext.lock() else { return };
                    deltas.extend(ext.tick(Instant::now(), max_age));
                }
                for ip in dirty_ips {
                    if resolve::dns::is_resolvable(&ip)
                        && !ptr_requested.contains(&ip)
                        && ptr_req_tx.try_send(ip).is_ok()
                    {
                        ptr_requested.insert(ip);
                    }
                }
                for delta in &deltas {
                    if let Some(msg) = wsproto::encode_delta(delta) {
                        // Erreur = aucun client connecté : sans importance.
                        let _ = deltas_tx.send(msg);
                    }
                }
                if last_log.elapsed() >= Duration::from_secs(10) {
                    last_log = Instant::now();
                    let (l2, l3) = {
                        let Ok(tables) = tables.lock() else { return };
                        (tables.l2.len(), tables.l3.len())
                    };
                    tracing::info!(
                        frames = stats.frames.load(Ordering::Relaxed),
                        l2_convs = l2,
                        l3_convs = l3,
                        clients = deltas_tx.receiver_count(),
                        chan_drops = stats.chan_drops.load(Ordering::Relaxed),
                        "aggregator status"
                    );
                }
            }
        }
    }
}
