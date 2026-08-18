//! netman-android — surface FFI (UniFFI, proc-macros, pas d'UDL).
//!
//! Les deltas traversent la frontière en JSON, forme figée par le contrat
//! `netman::wsproto` et ses tests (§6 : une seule source de vérité) — le
//! client Kotlin est un client du même protocole que le client web.
//!
//! Règles de thread pour l'implémentation Kotlin des callbacks :
//! - `on_deltas` : appelé depuis un worker tokio à chaque tick (≈ 4/s) ;
//!   ne faire que mettre en file (Channel), jamais toucher l'UI.
//! - `protect_socket` : synchrone, bref (VpnService.protect) ; appelé depuis
//!   `spawn_blocking` au moment d'ouvrir chaque socket sortante.
//! - `lookup_uid` / `resolve_app` : appels binder, potentiellement lents ;
//!   toujours sous `spawn_blocking`, bornés, jamais sur le chemin paquet.

use std::sync::{Arc, Mutex};

mod appext;
mod forward;
mod session;
mod tun;

use session::Session;

uniffi::setup_scaffolding!();

/// Session unique (invariant 1 : une capture à la fois).
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// Configuration d'une session de capture.
#[derive(uniffi::Record)]
pub struct SessionConfig {
    /// Descripteur tun détaché (`ParcelFileDescriptor.detachFd()`) — la
    /// propriété passe au Rust, qui le fermera.
    pub tun_fd: i32,
    /// Délai de fade initial (secondes), borné par le protocole.
    pub fade_secs: u64,
    /// MTU du tun (réservé au moteur de forwarding).
    pub mtu: u32,
    /// Adresses du tun (`Builder.addAddress`) : servent à orienter les flux.
    pub tun_addrs: Vec<String>,
    /// Résolveurs du réseau sous-jacent (réservé : repli hickory-resolver si
    /// `addDisallowedApplication` ne suffisait pas sur un constructeur).
    pub dns_servers: Vec<String>,
}

/// Identité d'une application résolue par la plateforme.
#[derive(uniffi::Record)]
pub struct AppLabel {
    pub label: String,
    pub package: String,
}

/// Compteurs de capture (notification, écran de démarrage).
#[derive(uniffi::Record)]
pub struct StatsSnapshot {
    pub frames: u64,
    pub bytes: u64,
    pub parse_errors: u64,
    pub chan_drops: u64,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum NetmanError {
    #[error("a capture session is already running")]
    AlreadyRunning,
    #[error("no capture session is running")]
    NotRunning,
    #[error("invalid configuration: {0}")]
    BadConfig(String),
    #[error("i/o error: {0}")]
    Io(String),
    #[error("internal state poisoned")]
    Poisoned,
}

/// Callbacks implémentés côté Kotlin (voir les règles de thread en tête de
/// module).
#[uniffi::export(with_foreign)]
pub trait NetmanCallbacks: Send + Sync {
    /// Un lot de deltas JSON par tick (~250 ms) : une traversée FFI.
    fn on_deltas(&self, deltas: Vec<String>);
    /// `VpnService.protect(fd)` — la socket échappe au tun (anti-boucle).
    fn protect_socket(&self, fd: i32) -> bool;
    /// `ConnectivityManager.getConnectionOwnerUid` ; négatif = inconnu.
    fn lookup_uid(
        &self,
        ip_number: u8,
        local: String,
        local_port: u16,
        remote: String,
        remote_port: u16,
    ) -> i32;
    /// PackageManager : uid → label + package. `None` si introuvable.
    fn resolve_app(&self, uid: i32) -> Option<AppLabel>;
}

/// Démarre la capture sur le descripteur tun fourni.
#[uniffi::export]
pub fn start(
    config: SessionConfig,
    callbacks: Arc<dyn NetmanCallbacks>,
) -> Result<(), NetmanError> {
    let mut guard = SESSION.lock().map_err(|_| NetmanError::Poisoned)?;
    if guard.is_some() {
        return Err(NetmanError::AlreadyRunning);
    }
    *guard = Some(Session::start(config, callbacks)?);
    Ok(())
}

/// Arrête la capture (idempotent : sans session, erreur `NotRunning`).
#[uniffi::export]
pub fn stop() -> Result<(), NetmanError> {
    let session = SESSION
        .lock()
        .map_err(|_| NetmanError::Poisoned)?
        .take()
        .ok_or(NetmanError::NotRunning)?;
    session.stop();
    Ok(())
}

/// Efface l'historique (caches de résolution conservés) ; un message
/// `{"type":"reset"}` part vers `on_deltas`.
#[uniffi::export]
pub fn reset() -> Result<(), NetmanError> {
    with_session(|s| {
        s.reset();
        Ok(())
    })
}

/// Règle le délai de fade (borné aux limites du protocole).
#[uniffi::export]
pub fn set_fade(seconds: u64) -> Result<(), NetmanError> {
    with_session(|s| {
        s.set_fade(seconds);
        Ok(())
    })
}

/// État complet (config + toutes vues) : resynchronisation d'une IHM qui
/// (re)vient — l'équivalent du préambule WebSocket du bureau.
#[uniffi::export]
pub fn snapshot() -> Result<Vec<String>, NetmanError> {
    with_session(|s| Ok(s.snapshot()))
}

/// Compteurs de capture courants.
#[uniffi::export]
pub fn capture_stats() -> Result<StatsSnapshot, NetmanError> {
    with_session(|s| Ok(s.stats()))
}

fn with_session<T>(f: impl FnOnce(&Session) -> Result<T, NetmanError>) -> Result<T, NetmanError> {
    let guard = SESSION.lock().map_err(|_| NetmanError::Poisoned)?;
    match guard.as_ref() {
        Some(session) => f(session),
        None => Err(NetmanError::NotRunning),
    }
}
