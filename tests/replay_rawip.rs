//! Rejeu de la fixture en mode « tun » : les mêmes trames, décapsulées de
//! leur en-tête Ethernet, passées par `parse_ip_packet` (le chemin Android).
//!
//! Vérifie sur le bureau, sans matériel Android :
//! - la vue Interman est IDENTIQUE à celle du rejeu Ethernet (mêmes
//!   conversations, mêmes octets, mêmes protocoles dominants) ;
//! - la vue Etherman reste vide par construction (pas de couche 2) ;
//! - la vue Appman attribue les flux aux bons uids via une table
//!   d'attribution simulée (le rôle que tiendra `getConnectionOwnerUid`).

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use netman::capture::{self, CaptureStats};
use netman::model::apps::{AppView, FlowKey, Uid, UNKNOWN_UID};
use netman::model::packet::{self, TransportMeta};
use netman::model::tables::Tables;
use netman::wsproto::Delta;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample.pcap")
}

/// Retire l'en-tête Ethernet d'une trame de la fixture. `None` pour les
/// trames sans charge IP (ARP) — un tun ne les verrait jamais.
fn strip_ethernet(frame: &[u8]) -> Option<&[u8]> {
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    match ethertype {
        0x0806 => None,               // ARP : affaire L2, invisible d'un tun
        0x8100 => Some(&frame[18..]), // 802.1Q : 4 octets de tag en plus
        _ => Some(&frame[14..]),
    }
}

/// Clé de flux du point de vue du poste (= le « téléphone » du test).
fn flow_key(t: &TransportMeta, src: IpAddr, dst: IpAddr, outbound: bool) -> FlowKey {
    if outbound {
        FlowKey {
            ip_number: t.ip_number,
            local: src,
            local_port: t.src_port,
            remote: dst,
            remote_port: t.dst_port,
        }
    } else {
        FlowKey {
            ip_number: t.ip_number,
            local: dst,
            local_port: t.dst_port,
            remote: src,
            remote_port: t.src_port,
        }
    }
}

#[test]
fn replay_fixture_as_raw_ip() {
    netman::setup_npcap_dll_path();
    let path = fixture_path();
    assert!(
        path.exists(),
        "lancer d'abord tests/replay.rs (génère la fixture)"
    );

    // Les adresses du « téléphone » : tout paquet dont la source est l'une
    // d'elles est sortant (le lecteur tun connaît ce sens dans la réalité).
    let device: [IpAddr; 2] = ["10.0.0.1".parse().unwrap(), "fe80::1".parse().unwrap()];

    // Attribution simulée : (protocole, port distant) → uid.
    let browser: Uid = 10001;
    let resolver: Uid = 10002;
    let discovery: Uid = 10003;
    let canned: HashMap<(u8, u16), Uid> = HashMap::from([
        ((6, 443), browser),
        ((17, 53), resolver),
        ((17, 5353), discovery),
    ]);

    let now = Instant::now();
    let mut tables_eth = Tables::new(); // référence : chemin Ethernet
    let mut tables_raw = Tables::new(); // chemin tun (parse_ip_packet)
    let mut apps = AppView::new();

    let stats = Arc::new(CaptureStats::default());
    let shutdown = Arc::new(AtomicBool::new(false));
    capture::run_file(&path, Arc::clone(&stats), shutdown, |data, wire_len| {
        // Référence Ethernet.
        if let Some(meta) = packet::parse_frame(data, wire_len) {
            tables_eth.ingest(&meta, now);
        }
        // Chemin tun : même trame décapsulée. `wire_len` d'origine conservé
        // pour que les compteurs des deux chemins soient comparables.
        let Some(ip_packet) = strip_ethernet(data) else {
            return;
        };
        let meta = packet::parse_ip_packet(ip_packet, wire_len)
            .expect("chaque trame IP de la fixture doit se décoder nue");
        tables_raw.ingest(&meta, now);

        // Projection Appman, comme le fera l'extension Android.
        let l3 = meta.l3.expect("parse_ip_packet garantit l3");
        let outbound = device.contains(&l3.src);
        let remote = if outbound { l3.dst } else { l3.src };
        match &l3.transport {
            Some(t) => {
                let key = flow_key(t, l3.src, l3.dst, outbound);
                if let Some(request) = apps.ingest(key, meta.wire_len, l3.proto, outbound, now) {
                    // Résolveur simulé, réponse immédiate.
                    let uid = canned
                        .get(&(request.ip_number, request.remote_port))
                        .copied()
                        .unwrap_or(UNKNOWN_UID);
                    apps.attribute(&request, uid, now);
                }
            }
            None => apps.ingest_unattributable(remote, meta.wire_len, l3.proto, outbound, now),
        }
    })
    .expect("replay fixture");

    // --- Etherman : vide par construction côté tun.
    assert!(tables_raw.l2.is_empty() && tables_raw.l2_nodes.is_empty());

    // --- Interman : projection identique des deux chemins de décodage
    // (l'ARP, absent du tun, ne touche que la vue L2 : sans effet ici).
    assert_eq!(tables_raw.l3.len(), tables_eth.l3.len());
    assert_eq!(tables_raw.l3_nodes.len(), tables_eth.l3_nodes.len());
    for (key, eth_conv) in &tables_eth.l3 {
        let raw_conv = tables_raw
            .l3
            .get(key)
            .unwrap_or_else(|| panic!("conversation {key:?} absente du chemin tun"));
        assert_eq!(raw_conv.bytes, eth_conv.bytes);
        assert_eq!(raw_conv.packets, eth_conv.packets);
        assert_eq!(raw_conv.dominant_proto(), eth_conv.dominant_proto());
    }

    // --- Appman : 4 applications (les 3 attribuées + Unknown pour l'ICMP),
    // 3 hôtes distants, 4 arêtes.
    let snapshot = apps.snapshot_deltas();
    let node = |id: &str| {
        snapshot
            .iter()
            .find_map(|d| match d {
                Delta::UpsertNode {
                    id: nid,
                    bytes,
                    proto,
                    ..
                } if nid == id => Some((*bytes, proto.clone())),
                _ => None,
            })
            .unwrap_or_else(|| panic!("nœud {id} absent de la vue Appman"))
    };
    let nodes = snapshot
        .iter()
        .filter(|d| matches!(d, Delta::UpsertNode { .. }))
        .count();
    let edges = snapshot
        .iter()
        .filter(|d| matches!(d, Delta::UpsertEdge { .. }))
        .count();
    assert_eq!(nodes, 4 + 3, "4 applications + 3 hôtes");
    assert_eq!(edges, 4);

    let ip_a: IpAddr = "10.0.0.1".parse().unwrap();
    let ip_b: IpAddr = "10.0.0.2".parse().unwrap();
    let ip_c: IpAddr = "10.0.0.53".parse().unwrap();
    let v6a: IpAddr = "fe80::1".parse().unwrap();
    let v6m: IpAddr = "ff02::fb".parse().unwrap();
    let l3 = &tables_raw.l3;
    let conv = |a: IpAddr, b: IpAddr| &l3[&netman::model::tables::L3Key::new(a, b)];

    // Le navigateur porte l'HTTPS de la conversation A-B ; l'ICMP (non
    // attribuable) de la même conversation est chez Unknown.
    let (browser_bytes, browser_proto) = node("app:10001");
    let (unknown_bytes, unknown_proto) = node(&format!("app:{UNKNOWN_UID}"));
    assert_eq!(browser_proto, "HTTPS");
    assert_eq!(unknown_proto, "ICMP");
    assert_eq!(browser_bytes + unknown_bytes, conv(ip_a, ip_b).bytes);

    let (dns_bytes, dns_proto) = node("app:10002");
    assert_eq!(dns_proto, "DNS");
    assert_eq!(dns_bytes, conv(ip_a, ip_c).bytes);

    let (mdns_bytes, mdns_proto) = node("app:10003");
    assert_eq!(mdns_proto, "mDNS");
    assert_eq!(mdns_bytes, conv(v6a, v6m).bytes);

    // Les hôtes distants portent les mêmes ids que la vue Interman.
    node("10.0.0.2");
    node("10.0.0.53");
    node("ff02::fb");
}
