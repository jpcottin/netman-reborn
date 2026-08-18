//! Session de capture Android : possède le runtime tokio, le thread de
//! lecture tun et les tâches (agrégateur, résolveur d'uid, puits de deltas).
//!
//! Une seule session à la fois (invariant 1 : une capture). L'arrêt est
//! idempotent et joint le thread tun (réveillé par son timeout de poll).

use std::collections::HashSet;
use std::net::IpAddr;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use netman::agg;
use netman::model::apps::{FlowKey, Uid};
use netman::model::tables::Tables;
use netman::stats::CaptureStats;
use netman::wsproto::{self, ServerInfo};
use tokio::sync::{broadcast, mpsc, Semaphore};

use crate::appext::AppExt;
use crate::{AppLabel, NetmanCallbacks, NetmanError, SessionConfig, StatsSnapshot};

/// Profondeur de la file des demandes d'attribution (même esprit que
/// `resolve::dns::REQUEST_QUEUE`) ; pleine ⇒ le flux expirera vers Unknown.
const UID_REQUEST_QUEUE: usize = 1024;
/// Profondeur de la file tun → moteur de forwarding ; pleine ⇒ paquet perdu
/// pour le forwarding seulement (TCP retransmet), la capture ne bloque pas.
const STACK_QUEUE: usize = 1024;
/// Appels binder simultanés au plus (lookup_uid / resolve_app).
const CONCURRENT_LOOKUPS: usize = 4;

pub struct Session {
    runtime: tokio::runtime::Runtime,
    shutdown: Arc<AtomicBool>,
    tun_thread: Option<std::thread::JoinHandle<()>>,
    tables: Arc<Mutex<Tables>>,
    ext: Arc<Mutex<AppExt>>,
    fade_secs: Arc<AtomicU64>,
    deltas_tx: broadcast::Sender<String>,
    stats: Arc<CaptureStats>,
}

impl Session {
    pub fn start(
        config: SessionConfig,
        callbacks: Arc<dyn NetmanCallbacks>,
    ) -> Result<Session, NetmanError> {
        if config.tun_fd < 0 {
            return Err(NetmanError::BadConfig(format!(
                "invalid tun fd {}",
                config.tun_fd
            )));
        }
        let device_addrs: Vec<IpAddr> = config
            .tun_addrs
            .iter()
            .map(|s| {
                s.parse()
                    .map_err(|_| NetmanError::BadConfig(format!("invalid tun address '{s}'")))
            })
            .collect::<Result<_, _>>()?;
        if device_addrs.is_empty() {
            return Err(NetmanError::BadConfig("no tun address".into()));
        }
        // SAFETY : le fd vient de `ParcelFileDescriptor.detachFd()` (ou d'un
        // socketpair de test) — la propriété est transférée, Rust le fermera.
        let fd = unsafe { OwnedFd::from_raw_fd(config.tun_fd) };
        // O_NONBLOCK : indispensable aux écritures du forwarding (AsyncFd) ;
        // la boucle de lecture, gardée par poll(), le tolère.
        // SAFETY : fcntl sur un descripteur possédé, flags standards.
        unsafe {
            let flags = libc::fcntl(config.tun_fd, libc::F_GETFL);
            if flags < 0 || libc::fcntl(config.tun_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0
            {
                return Err(NetmanError::Io(std::io::Error::last_os_error().to_string()));
            }
        }
        let fd = Arc::new(fd);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("netman-core")
            .enable_all()
            .build()
            .map_err(|e| NetmanError::Io(e.to_string()))?;

        let stats = Arc::new(CaptureStats::default());
        let shutdown = Arc::new(AtomicBool::new(false));
        let tables = Arc::new(Mutex::new(Tables::new()));
        let fade_secs = Arc::new(AtomicU64::new(
            config
                .fade_secs
                .clamp(wsproto::FADE_MIN_SECS, wsproto::FADE_MAX_SECS),
        ));

        let (meta_tx, meta_rx) = mpsc::channel(agg::CHANNEL_CAPACITY);
        let (stack_tx, stack_rx) = mpsc::channel::<Vec<u8>>(STACK_QUEUE);
        let (deltas_tx, _) = broadcast::channel::<String>(agg::BROADCAST_CAPACITY);
        let (uid_req_tx, uid_req_rx) = mpsc::channel::<FlowKey>(UID_REQUEST_QUEUE);
        let (uid_res_tx, uid_res_rx) = mpsc::channel::<(FlowKey, Uid)>(UID_REQUEST_QUEUE);
        let (label_res_tx, label_res_rx) = mpsc::channel::<(Uid, String)>(256);

        let ext = Arc::new(Mutex::new(AppExt::new(
            device_addrs,
            uid_req_tx,
            uid_res_rx,
            label_res_rx,
        )));

        // Thread OS dédié à la lecture du tun (invariant 2).
        let tun_thread = {
            let fd = Arc::clone(&fd);
            let stats = Arc::clone(&stats);
            let shutdown = Arc::clone(&shutdown);
            let meta_tx = meta_tx.clone();
            std::thread::Builder::new()
                .name("tun-read".into())
                .spawn(move || crate::tun::run_tun(fd, stats, shutdown, meta_tx, stack_tx))
                .map_err(|e| NetmanError::Io(e.to_string()))?
        };

        // Moteur de forwarding : sa mort éventuelle n'affecte pas la capture.
        runtime.spawn(crate::forward::run_forwarder(
            fd,
            u16::try_from(config.mtu).unwrap_or(1500),
            stack_rx,
            meta_tx,
            Arc::clone(&stats),
            Arc::clone(&callbacks),
        ));

        runtime.spawn(agg::aggregate_loop(
            meta_rx,
            Arc::clone(&tables),
            Arc::clone(&ext),
            deltas_tx.clone(),
            Arc::clone(&stats),
            Arc::clone(&fade_secs),
        ));
        runtime.spawn(resolver_loop(
            uid_req_rx,
            uid_res_tx,
            label_res_tx,
            Arc::clone(&callbacks),
        ));
        runtime.spawn(sink_loop(deltas_tx.subscribe(), callbacks));

        // Configuration initiale, comme le préambule WebSocket du bureau.
        if let Some(msg) = wsproto::encode_info(&ServerInfo::Config {
            fade_secs: fade_secs.load(Ordering::Relaxed),
        }) {
            let _ = deltas_tx.send(msg);
        }

        Ok(Session {
            runtime,
            shutdown,
            tun_thread: Some(tun_thread),
            tables,
            ext,
            fade_secs,
            deltas_tx,
            stats,
        })
    }

    /// Arrêt : flag pour la boucle tun (réveillée par son poll ≤ 500 ms),
    /// join hors runtime, puis abandon des tâches tokio.
    pub fn stop(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.tun_thread.take() {
            if thread.join().is_err() {
                tracing::error!("tun reader thread panicked");
            }
        }
        // Les tâches (agrégateur, résolveur, puits) sont annulées sans join :
        // rien à vider proprement, les channels meurent avec elles.
        self.runtime.shutdown_background();
    }

    /// État complet : configuration puis snapshot des vues (préambule du
    /// bureau transposé) — pour une IHM qui (re)vient.
    pub fn snapshot(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(msg) = wsproto::encode_info(&ServerInfo::Config {
            fade_secs: self.fade_secs.load(Ordering::Relaxed),
        }) {
            out.push(msg);
        }
        if let Ok(tables) = self.tables.lock() {
            out.extend(
                tables
                    .snapshot_deltas()
                    .iter()
                    .filter_map(wsproto::encode_delta),
            );
        }
        if let Ok(ext) = self.ext.lock() {
            use netman::agg::AggExt;
            out.extend(ext.snapshot().iter().filter_map(wsproto::encode_delta));
        }
        out
    }

    /// Efface l'historique (caches de résolution conservés) et notifie.
    pub fn reset(&self) {
        use netman::agg::AggExt;
        if let Ok(mut tables) = self.tables.lock() {
            tables.reset();
        }
        if let Ok(mut ext) = self.ext.lock() {
            ext.reset();
        }
        if let Some(msg) = wsproto::encode_info(&ServerInfo::Reset) {
            let _ = self.deltas_tx.send(msg);
        }
    }

    /// Règle le fade (borné) et propage la configuration, comme le serveur.
    pub fn set_fade(&self, seconds: u64) {
        let clamped = seconds.clamp(wsproto::FADE_MIN_SECS, wsproto::FADE_MAX_SECS);
        self.fade_secs.store(clamped, Ordering::Relaxed);
        if let Some(msg) = wsproto::encode_info(&ServerInfo::Config { fade_secs: clamped }) {
            let _ = self.deltas_tx.send(msg);
        }
    }

    pub fn stats(&self) -> StatsSnapshot {
        StatsSnapshot {
            frames: self.stats.frames.load(Ordering::Relaxed),
            bytes: self.stats.bytes.load(Ordering::Relaxed),
            parse_errors: self.stats.parse_errors.load(Ordering::Relaxed),
            chan_drops: self.stats.chan_drops.load(Ordering::Relaxed),
        }
    }
}

/// Puits des deltas : relaie les lots vers le callback Kotlin. Un lot par
/// réveil (le broadcast est vidé d'un coup) ⇒ une traversée FFI par tick.
async fn sink_loop(mut rx: broadcast::Receiver<String>, cb: Arc<dyn NetmanCallbacks>) {
    loop {
        match rx.recv().await {
            Ok(first) => {
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                // Le callback doit seulement mettre en file (voir la doc du
                // trait) : appel direct, pas de spawn_blocking par lot.
                cb.on_deltas(batch);
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                // Consommateur trop lent : deltas sautés, les upserts
                // suivants réparent (valeurs absolues).
                tracing::warn!(skipped, "delta sink lagged");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Résolveur d'attribution : consomme les clés de flux, appelle Kotlin
/// (binder) sous `spawn_blocking`, borné par un sémaphore. Le label d'une
/// application n'est demandé qu'une fois par uid.
async fn resolver_loop(
    mut uid_req_rx: mpsc::Receiver<FlowKey>,
    uid_res_tx: mpsc::Sender<(FlowKey, Uid)>,
    label_res_tx: mpsc::Sender<(Uid, String)>,
    cb: Arc<dyn NetmanCallbacks>,
) {
    let semaphore = Arc::new(Semaphore::new(CONCURRENT_LOOKUPS));
    let labels_requested: Arc<Mutex<HashSet<Uid>>> = Arc::new(Mutex::new(HashSet::new()));

    while let Some(key) = uid_req_rx.recv().await {
        let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
            return;
        };
        let cb = Arc::clone(&cb);
        let uid_res_tx = uid_res_tx.clone();
        let label_res_tx = label_res_tx.clone();
        let labels_requested = Arc::clone(&labels_requested);
        tokio::spawn(async move {
            let _permit = permit;
            let outcome = tokio::task::spawn_blocking(move || {
                let uid = cb.lookup_uid(
                    key.ip_number,
                    key.local.to_string(),
                    key.local_port,
                    key.remote.to_string(),
                    key.remote_port,
                );
                let label: Option<AppLabel> = if uid >= 0 {
                    let first_time = labels_requested
                        .lock()
                        .map(|mut seen| seen.insert(uid))
                        .unwrap_or(false);
                    if first_time {
                        cb.resolve_app(uid)
                    } else {
                        None
                    }
                } else {
                    None
                };
                (uid, label)
            })
            .await;
            let Ok((uid, label)) = outcome else { return };
            let _ = uid_res_tx.send((key, uid)).await;
            if let Some(app) = label {
                let _ = label_res_tx.send((uid, app.label)).await;
            }
        });
    }
}
