//! Compteurs de capture, partagés entre le thread de capture et l'affichage.
//!
//! Hors de `capture/` (spécifique pcap, absent du build Android) : la source
//! des paquets change selon la plateforme (pcap ou tun), les compteurs non.

use std::sync::atomic::AtomicU64;

/// Compteurs partagés entre le thread de capture et l'affichage.
/// Le thread de capture ne fait qu'incrémenter des atomiques : aucun lock.
#[derive(Debug, Default)]
pub struct CaptureStats {
    pub frames: AtomicU64,
    pub bytes: AtomicU64,
    /// Paquets perdus par le kernel/Npcap (compteur cumulatif `ps_drop`).
    pub kernel_drops: AtomicU64,
    /// Trames non décodables (pas Ethernet II, tronquées…).
    pub parse_errors: AtomicU64,
    /// Métadonnées jetées car le channel vers l'agrégateur était plein
    /// (invariant 5 : on jette plutôt que de bloquer la capture).
    pub chan_drops: AtomicU64,
}
