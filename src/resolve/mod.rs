//! Résolutions best-effort (invariant 4) : jamais dans le chemin de capture,
//! jamais bloquantes pour l'agrégateur.

pub mod dns;
// La base OUI (3,1 Mo embarqués) ne sert qu'aux labels L2 — vue inexistante
// sur Android (pas d'Ethernet sur un tun) : on l'écarte du build.
#[cfg(not(target_os = "android"))]
pub mod oui;

use crate::model::packet::Mac;

/// Label affiché d'un nœud L2 : vendeur OUI si la base est embarquée,
/// sinon l'adresse brute.
#[cfg(not(target_os = "android"))]
pub fn mac_label(mac: &Mac) -> String {
    oui::label(mac)
}

#[cfg(target_os = "android")]
pub fn mac_label(mac: &Mac) -> String {
    mac.to_string()
}
