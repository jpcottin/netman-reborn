//! Contrat WebSocket (CLAUDE.md §6) — enum serde taguée sur `type`.
//!
//! Une mutation de graphe = un message. Le frontend applique, il ne
//! recalcule pas. Toute modification de ce schéma met à jour backend ET
//! frontend dans le même commit.

use serde::{Deserialize, Serialize};

/// Bornes du délai de fade réglable par les clients (politique du protocole,
/// commune au serveur WebSocket du bureau et à la FFI Android).
pub const FADE_MIN_SECS: u64 = 5;
pub const FADE_MAX_SECS: u64 = 3600;

/// Sérialise un delta une seule fois, prêt à diffuser (zéro-copie ensuite).
pub fn encode_delta(delta: &Delta) -> Option<String> {
    match serde_json::to_string(delta) {
        Ok(json) => Some(json),
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize delta");
            None
        }
    }
}

/// Encode un message d'information serveur → client.
pub fn encode_info(info: &ServerInfo) -> Option<String> {
    match serde_json::to_string(info) {
        Ok(json) => Some(json),
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize server info");
            None
        }
    }
}

/// Vue ciblée par un delta : graphe Etherman (L2), Interman (L3), ou —
/// Android uniquement — Appman (conversations application ↔ hôte distant).
/// Le client web ignore silencieusement les vues qu'il ne connaît pas.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Ether,
    Inter,
    App,
}

/// Mutation atomique d'un des deux graphes.
///
/// Les valeurs `bytes`/`packets` sont des **cumuls absolus** (pas des
/// incréments) : un upsert manqué est réparé par le suivant.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    UpsertNode {
        view: View,
        id: String,
        label: String,
        bytes: u64,
        /// Octets entrés dans l'hôte (il est destination).
        bytes_in: u64,
        /// Octets sortis de l'hôte (il est source).
        bytes_out: u64,
        packets: u64,
        proto: String,
    },
    UpsertEdge {
        view: View,
        id: String,
        source: String,
        target: String,
        bytes: u64,
        packets: u64,
        proto: String,
    },
    RemoveNode {
        view: View,
        id: String,
    },
    RemoveEdge {
        view: View,
        id: String,
    },
}

/// Message de contrôle client → serveur.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientCommand {
    /// Règle le délai de fade (nœuds/arêtes non revus depuis N s → retirés).
    SetFade { seconds: u64 },
    /// Bascule la capture sur une autre interface (id = nom NPF).
    SetInterface { id: String },
    /// Efface l'historique des conversations et nœuds (comme si aucun paquet
    /// n'avait été reçu) — les caches de résolution DNS sont conservés.
    Reset,
}

/// Interface de capture présentée au client.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct IfaceInfo {
    /// Identifiant stable (nom NPF sous Windows).
    pub id: String,
    /// Nom convivial (+ adresses).
    pub label: String,
}

/// Message d'information serveur → client, hors mutations de graphe.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerInfo {
    /// Configuration courante (envoyée à la connexion et à chaque changement).
    Config { fade_secs: u64 },
    /// Interfaces disponibles + interface active (vide en mode fichier).
    Interfaces {
        current: Option<String>,
        interfaces: Vec<IfaceInfo>,
    },
    /// L'historique vient d'être effacé : les clients vident leurs vues.
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Non-régression du contrat : la forme JSON exacte est figée.
    #[test]
    fn json_contract_shape() {
        let node = Delta::UpsertNode {
            view: View::Ether,
            id: "aa:bb:cc:dd:ee:ff".into(),
            label: "aa:bb:cc:dd:ee:ff".into(),
            bytes: 1500,
            bytes_in: 600,
            bytes_out: 900,
            packets: 10,
            proto: "IPv4".into(),
        };
        assert_eq!(
            serde_json::to_string(&node).unwrap(),
            r#"{"type":"upsert_node","view":"ether","id":"aa:bb:cc:dd:ee:ff","label":"aa:bb:cc:dd:ee:ff","bytes":1500,"bytes_in":600,"bytes_out":900,"packets":10,"proto":"IPv4"}"#
        );

        let edge = Delta::UpsertEdge {
            view: View::Inter,
            id: "10.0.0.1|10.0.0.2".into(),
            source: "10.0.0.1".into(),
            target: "10.0.0.2".into(),
            bytes: 42,
            packets: 1,
            proto: "DNS".into(),
        };
        assert_eq!(
            serde_json::to_string(&edge).unwrap(),
            r#"{"type":"upsert_edge","view":"inter","id":"10.0.0.1|10.0.0.2","source":"10.0.0.1","target":"10.0.0.2","bytes":42,"packets":1,"proto":"DNS"}"#
        );

        let rm = Delta::RemoveEdge {
            view: View::Inter,
            id: "10.0.0.1|10.0.0.2".into(),
        };
        assert_eq!(
            serde_json::to_string(&rm).unwrap(),
            r#"{"type":"remove_edge","view":"inter","id":"10.0.0.1|10.0.0.2"}"#
        );

        // Vue Appman (Android) : mêmes formes, `view` vaut "app", les nœuds
        // application portent l'id "app:<uid>".
        let app_node = Delta::UpsertNode {
            view: View::App,
            id: "app:10123".into(),
            label: "Firefox".into(),
            bytes: 2048,
            bytes_in: 1024,
            bytes_out: 1024,
            packets: 4,
            proto: "HTTPS".into(),
        };
        assert_eq!(
            serde_json::to_string(&app_node).unwrap(),
            r#"{"type":"upsert_node","view":"app","id":"app:10123","label":"Firefox","bytes":2048,"bytes_in":1024,"bytes_out":1024,"packets":4,"proto":"HTTPS"}"#
        );
        let app_edge = Delta::UpsertEdge {
            view: View::App,
            id: "app:10123|93.184.216.34".into(),
            source: "app:10123".into(),
            target: "93.184.216.34".into(),
            bytes: 2048,
            packets: 4,
            proto: "HTTPS".into(),
        };
        assert_eq!(
            serde_json::to_string(&app_edge).unwrap(),
            r#"{"type":"upsert_edge","view":"app","id":"app:10123|93.184.216.34","source":"app:10123","target":"93.184.216.34","bytes":2048,"packets":4,"proto":"HTTPS"}"#
        );
        assert_eq!(
            serde_json::to_string(&Delta::RemoveNode {
                view: View::App,
                id: "app:10123".into(),
            })
            .unwrap(),
            r#"{"type":"remove_node","view":"app","id":"app:10123"}"#
        );
    }

    #[test]
    fn control_messages_shape() {
        assert_eq!(
            serde_json::to_string(&ServerInfo::Config { fade_secs: 60 }).unwrap(),
            r#"{"type":"config","fade_secs":60}"#
        );
        let cmd: ClientCommand =
            serde_json::from_str(r#"{"type":"set_fade","seconds":120}"#).unwrap();
        assert_eq!(cmd, ClientCommand::SetFade { seconds: 120 });

        let cmd: ClientCommand = serde_json::from_str(r#"{"type":"reset"}"#).unwrap();
        assert_eq!(cmd, ClientCommand::Reset);
        assert_eq!(
            serde_json::to_string(&ServerInfo::Reset).unwrap(),
            r#"{"type":"reset"}"#
        );

        let cmd: ClientCommand =
            serde_json::from_str(r#"{"type":"set_interface","id":"\\Device\\NPF_{X}"}"#).unwrap();
        assert_eq!(
            cmd,
            ClientCommand::SetInterface {
                id: r"\Device\NPF_{X}".into()
            }
        );
        assert_eq!(
            serde_json::to_string(&ServerInfo::Interfaces {
                current: Some("npf1".into()),
                interfaces: vec![IfaceInfo {
                    id: "npf1".into(),
                    label: "Intel(R) Ethernet [192.168.0.2]".into()
                }],
            })
            .unwrap(),
            r#"{"type":"interfaces","current":"npf1","interfaces":[{"id":"npf1","label":"Intel(R) Ethernet [192.168.0.2]"}]}"#
        );
    }

    #[test]
    fn json_roundtrip() {
        let deltas = vec![
            Delta::UpsertNode {
                view: View::Inter,
                id: "fe80::1".into(),
                label: "fe80::1".into(),
                bytes: 0,
                bytes_in: 0,
                bytes_out: 0,
                packets: 0,
                proto: "mDNS".into(),
            },
            Delta::RemoveNode {
                view: View::Ether,
                id: "aa:bb:cc:dd:ee:ff".into(),
            },
        ];
        for d in deltas {
            let json = serde_json::to_string(&d).unwrap();
            let back: Delta = serde_json::from_str(&json).unwrap();
            assert_eq!(back, d);
        }
    }
}
