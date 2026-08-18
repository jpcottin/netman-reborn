//! Vue « Appman » (Android) : conversations application ↔ hôte distant.
//!
//! Troisième projection du même flux de paquets (invariant 1) : un nœud par
//! application Android (uid), un nœud par hôte distant (mêmes ids que la vue
//! Interman), une arête par couple (application, hôte).
//!
//! L'attribution d'un flux à un uid (`getConnectionOwnerUid`) est un appel
//! binder, asynchrone et hors du chemin paquet (invariant 4). Tant que la
//! réponse n'est pas arrivée, les paquets du flux s'ACCUMULENT dans un tampon
//! `pending` — on n'attribue jamais au hasard. À la réponse (ou à
//! l'expiration), l'accumulateur est versé d'un bloc sous le bon uid (ou sous
//! « Unknown »).
//!
//! Module pur (std + wsproto) : compilé et testé sur toutes les plateformes,
//! seul le port Android l'alimente.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use super::packet::Proto;
use super::tables::{edge_delta, node_delta, ConvStats};
use crate::wsproto::{Delta, View};

/// Uid Android (i32 : `getConnectionOwnerUid` renvoie `INVALID_UID` = -1).
pub type Uid = i32;

/// Nœud-puits des flux non attribuables (pas de ports, uid introuvable,
/// résolution expirée). Première classe : il apparaît comme les autres.
pub const UNKNOWN_UID: Uid = -1;

/// Nombre maximal de flux en attente d'attribution ; au-delà, le plus ancien
/// est versé sous « Unknown » (borne mémoire, invariant 5 en esprit).
pub const PENDING_CAPACITY: usize = 512;

/// Clé de flux orientée : le poste local d'un côté, l'hôte distant de
/// l'autre (le lecteur tun connaît le sens de chaque paquet).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FlowKey {
    /// Numéro de protocole IP (6 = TCP, 17 = UDP).
    pub ip_number: u8,
    pub local: IpAddr,
    pub local_port: u16,
    pub remote: IpAddr,
    pub remote_port: u16,
}

/// Accumulateur d'un flux en attente d'attribution.
#[derive(Debug)]
struct PendingFlow {
    created: Instant,
    proto: Proto,
    bytes_out: u64,
    bytes_in: u64,
    packets_out: u64,
    packets_in: u64,
}

impl PendingFlow {
    fn new(now: Instant, proto: Proto) -> Self {
        PendingFlow {
            created: now,
            proto,
            bytes_out: 0,
            bytes_in: 0,
            packets_out: 0,
            packets_in: 0,
        }
    }

    fn add(&mut self, bytes: u64, outbound: bool) {
        if outbound {
            self.bytes_out += bytes;
            self.packets_out += 1;
        } else {
            self.bytes_in += bytes;
            self.packets_in += 1;
        }
    }
}

/// La vue Appman : tables (nœuds applications, nœuds hôtes, arêtes) + cache
/// d'attribution des flux + tampon des flux en attente. Même mécanique de
/// deltas que [`super::tables::Tables`] : dirty par tick, fade explicite,
/// snapshot complet, labels en cache de session.
#[derive(Default)]
pub struct AppView {
    edges: HashMap<(Uid, IpAddr), ConvStats>,
    app_nodes: HashMap<Uid, ConvStats>,
    host_nodes: HashMap<IpAddr, ConvStats>,
    /// Labels d'applications (uid → nom), fournis par la plateforme ;
    /// cache de session, survit au fade et au reset (comme les PTR).
    app_labels: HashMap<Uid, String>,
    /// Noms d'hôtes (reverse DNS), partagés sémantiquement avec Interman.
    host_labels: HashMap<IpAddr, String>,
    /// Flux déjà attribués : uid + dernier paquet vu (pour le ménage).
    flow_uids: HashMap<FlowKey, (Uid, Instant)>,
    pending: HashMap<FlowKey, PendingFlow>,
    dirty_edges: HashSet<(Uid, IpAddr)>,
    dirty_apps: HashSet<Uid>,
    dirty_hosts: HashSet<IpAddr>,
}

/// Id de nœud application dans le protocole : `app:<uid>` (jamais en
/// collision avec un id d'hôte, une IP ne commence pas par `app:`).
fn app_id(uid: Uid) -> String {
    format!("app:{uid}")
}

impl AppView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paquet TCP/UDP (attribuable). Renvoie `Some(key)` si le flux est
    /// inconnu et vient d'entrer en attente : l'appelant doit alors demander
    /// son attribution (une seule fois par flux), hors du chemin paquet.
    pub fn ingest(
        &mut self,
        key: FlowKey,
        wire_len: u64,
        proto: Proto,
        outbound: bool,
        now: Instant,
    ) -> Option<FlowKey> {
        if let Some((uid, seen)) = self.flow_uids.get_mut(&key) {
            *seen = now;
            let uid = *uid;
            let (out, inb) = if outbound {
                (wire_len, 0)
            } else {
                (0, wire_len)
            };
            let (p_out, p_in) = if outbound { (1, 0) } else { (0, 1) };
            self.apply(uid, key.remote, proto, out, inb, p_out, p_in, now);
            return None;
        }

        if let Some(pending) = self.pending.get_mut(&key) {
            pending.add(wire_len, outbound);
            return None;
        }

        // Tampon plein : le doyen part sous « Unknown » plutôt que de croître.
        if self.pending.len() >= PENDING_CAPACITY {
            if let Some(oldest) = self
                .pending
                .iter()
                .min_by_key(|(_, p)| p.created)
                .map(|(k, _)| *k)
            {
                self.settle(&oldest, UNKNOWN_UID, now);
            }
        }
        let mut pending = PendingFlow::new(now, proto);
        pending.add(wire_len, outbound);
        self.pending.insert(key, pending);
        Some(key)
    }

    /// Paquet sans ports (ICMP…) : non attribuable, directement « Unknown ».
    pub fn ingest_unattributable(
        &mut self,
        remote: IpAddr,
        wire_len: u64,
        proto: Proto,
        outbound: bool,
        now: Instant,
    ) {
        let (out, inb) = if outbound {
            (wire_len, 0)
        } else {
            (0, wire_len)
        };
        let (p_out, p_in) = if outbound { (1, 0) } else { (0, 1) };
        self.apply(UNKNOWN_UID, remote, proto, out, inb, p_out, p_in, now);
    }

    /// Réponse du résolveur d'uid. Verse l'accumulateur du flux (s'il attend
    /// encore) et mémorise l'attribution pour les paquets suivants. Un uid
    /// négatif (INVALID_UID) vaut « Unknown ». Une réponse arrivée après
    /// expiration ne corrige que l'avenir : le déjà-compté reste où il est.
    pub fn attribute(&mut self, key: &FlowKey, uid: Uid, now: Instant) {
        let uid = if uid < 0 { UNKNOWN_UID } else { uid };
        self.settle(key, uid, now);
        self.flow_uids.insert(*key, (uid, now));
    }

    /// Expire les attributions en attente depuis plus de `max_wait` : leurs
    /// accumulateurs partent sous « Unknown » (réponse jamais arrivée).
    pub fn expire_pending(&mut self, now: Instant, max_wait: Duration) {
        let expired: Vec<FlowKey> = self
            .pending
            .iter()
            .filter(|(_, p)| now.duration_since(p.created) > max_wait)
            .map(|(k, _)| *k)
            .collect();
        for key in expired {
            self.settle(&key, UNKNOWN_UID, now);
            self.flow_uids.insert(key, (UNKNOWN_UID, now));
        }
    }

    /// Verse l'accumulateur d'un flux en attente sous `uid` (no-op sinon).
    fn settle(&mut self, key: &FlowKey, uid: Uid, now: Instant) {
        if let Some(p) = self.pending.remove(key) {
            self.apply(
                uid,
                key.remote,
                p.proto,
                p.bytes_out,
                p.bytes_in,
                p.packets_out,
                p.packets_in,
                now,
            );
        }
    }

    /// Accumulation effective dans l'arête et les deux nœuds.
    /// Perspectives : l'application émet `bytes_out` ; l'hôte distant, lui,
    /// a émis ce que le poste a reçu (`bytes_in`).
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        uid: Uid,
        remote: IpAddr,
        proto: Proto,
        bytes_out: u64,
        bytes_in: u64,
        packets_out: u64,
        packets_in: u64,
        now: Instant,
    ) {
        let bytes = bytes_out + bytes_in;
        let packets = packets_out + packets_in;

        let edge = self
            .edges
            .entry((uid, remote))
            .or_insert_with(|| ConvStats::new(now));
        edge.add_many(bytes, packets, proto, now);
        self.dirty_edges.insert((uid, remote));

        let app = self
            .app_nodes
            .entry(uid)
            .or_insert_with(|| ConvStats::new(now));
        app.add_many(bytes, packets, proto, now);
        app.tx_bytes += bytes_out;
        app.rx_bytes += bytes_in;
        self.dirty_apps.insert(uid);

        let host = self
            .host_nodes
            .entry(remote)
            .or_insert_with(|| ConvStats::new(now));
        host.add_many(bytes, packets, proto, now);
        host.tx_bytes += bytes_in;
        host.rx_bytes += bytes_out;
        self.dirty_hosts.insert(remote);
    }

    /// Label d'application (fourni par la plateforme, PackageManager côté
    /// Android). Même mécanique que `Tables::set_l3_label` : re-dirty si le
    /// nœud est affiché, cache conservé sinon.
    pub fn set_app_label(&mut self, uid: Uid, label: String) {
        if self.app_nodes.contains_key(&uid) {
            self.dirty_apps.insert(uid);
        }
        self.app_labels.insert(uid, label);
    }

    /// Nom d'hôte résolu (reverse DNS) — partagé avec la vue Interman.
    pub fn set_host_label(&mut self, ip: IpAddr, label: String) {
        if self.host_nodes.contains_key(&ip) {
            self.dirty_hosts.insert(ip);
        }
        self.host_labels.insert(ip, label);
    }

    fn app_label(&self, uid: Uid) -> String {
        self.app_labels.get(&uid).cloned().unwrap_or_else(|| {
            if uid == UNKNOWN_UID {
                "Unknown".to_string()
            } else {
                format!("uid {uid}")
            }
        })
    }

    fn host_label(&self, ip: &IpAddr) -> String {
        self.host_labels
            .get(ip)
            .cloned()
            .unwrap_or_else(|| ip.to_string())
    }

    /// Deltas des entrées modifiées depuis le dernier appel (nœuds avant
    /// arêtes, comme `Tables::drain_deltas`).
    pub fn drain_deltas(&mut self) -> Vec<Delta> {
        let mut out = Vec::with_capacity(
            self.dirty_apps.len() + self.dirty_hosts.len() + self.dirty_edges.len(),
        );
        for uid in std::mem::take(&mut self.dirty_apps) {
            if let Some(stats) = self.app_nodes.get(&uid) {
                out.push(node_delta(
                    View::App,
                    app_id(uid),
                    self.app_label(uid),
                    stats,
                ));
            }
        }
        for ip in std::mem::take(&mut self.dirty_hosts) {
            if let Some(stats) = self.host_nodes.get(&ip) {
                out.push(node_delta(
                    View::App,
                    ip.to_string(),
                    self.host_label(&ip),
                    stats,
                ));
            }
        }
        for (uid, ip) in self.dirty_edges.drain() {
            if let Some(stats) = self.edges.get(&(uid, ip)) {
                out.push(edge_delta(View::App, app_id(uid), ip.to_string(), stats));
            }
        }
        out
    }

    /// Vieillissement (invariant 7) : arêtes puis nœuds, suppressions
    /// explicites. Fait aussi le ménage du cache d'attribution.
    pub fn fade_sweep(&mut self, now: Instant, max_age: Duration) -> Vec<Delta> {
        let mut out = Vec::new();
        let stale = |stats: &ConvStats| now.duration_since(stats.last_seen) > max_age;

        let stale_edges: Vec<(Uid, IpAddr)> = self
            .edges
            .iter()
            .filter(|(_, s)| stale(s))
            .map(|(k, _)| *k)
            .collect();
        for key in stale_edges {
            self.edges.remove(&key);
            self.dirty_edges.remove(&key);
            out.push(Delta::RemoveEdge {
                view: View::App,
                id: super::tables::edge_id(&app_id(key.0), &key.1.to_string()),
            });
        }

        let stale_apps: Vec<Uid> = self
            .app_nodes
            .iter()
            .filter(|(_, s)| stale(s))
            .map(|(k, _)| *k)
            .collect();
        for uid in stale_apps {
            self.app_nodes.remove(&uid);
            self.dirty_apps.remove(&uid);
            out.push(Delta::RemoveNode {
                view: View::App,
                id: app_id(uid),
            });
        }
        let stale_hosts: Vec<IpAddr> = self
            .host_nodes
            .iter()
            .filter(|(_, s)| stale(s))
            .map(|(k, _)| *k)
            .collect();
        for ip in stale_hosts {
            self.host_nodes.remove(&ip);
            self.dirty_hosts.remove(&ip);
            out.push(Delta::RemoveNode {
                view: View::App,
                id: ip.to_string(),
            });
        }

        // Flux attribués plus revus depuis `max_age` : le cache s'allège au
        // même rythme que l'affichage.
        self.flow_uids
            .retain(|_, (_, seen)| now.duration_since(*seen) <= max_age);
        out
    }

    /// État complet sous forme de deltas (client qui (re)arrive).
    pub fn snapshot_deltas(&self) -> Vec<Delta> {
        let mut out =
            Vec::with_capacity(self.app_nodes.len() + self.host_nodes.len() + self.edges.len());
        for (uid, stats) in &self.app_nodes {
            out.push(node_delta(
                View::App,
                app_id(*uid),
                self.app_label(*uid),
                stats,
            ));
        }
        for (ip, stats) in &self.host_nodes {
            out.push(node_delta(
                View::App,
                ip.to_string(),
                self.host_label(ip),
                stats,
            ));
        }
        for ((uid, ip), stats) in &self.edges {
            out.push(edge_delta(View::App, app_id(*uid), ip.to_string(), stats));
        }
        out
    }

    /// Efface l'historique en conservant les caches (labels d'applications,
    /// noms d'hôtes, attribution des flux) — mêmes règles que `Tables::reset`.
    pub fn reset(&mut self) {
        self.edges.clear();
        self.app_nodes.clear();
        self.host_nodes.clear();
        self.pending.clear();
        self.dirty_edges.clear();
        self.dirty_apps.clear();
        self.dirty_hosts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTTPS: Proto = Proto::Named("HTTPS");
    const ICMP: Proto = Proto::Named("ICMP");

    fn key(remote: &str, remote_port: u16) -> FlowKey {
        FlowKey {
            ip_number: 6,
            local: "10.111.222.1".parse().unwrap(),
            local_port: 51000,
            remote: remote.parse().unwrap(),
            remote_port,
        }
    }

    fn find_node<'a>(deltas: &'a [Delta], want: &str) -> Option<&'a Delta> {
        deltas
            .iter()
            .find(|d| matches!(d, Delta::UpsertNode { id, .. } if id == want))
    }

    #[test]
    fn pending_flow_buffers_until_attributed() {
        let mut view = AppView::new();
        let now = Instant::now();
        let k = key("93.184.216.34", 443);

        // Premier paquet : demande d'attribution, une seule fois.
        assert_eq!(view.ingest(k, 100, HTTPS, true, now), Some(k));
        assert_eq!(view.ingest(k, 400, HTTPS, false, now), None);

        // Rien n'est affiché tant que l'attribution n'est pas connue.
        assert!(view.drain_deltas().is_empty());

        // Attribution : tout l'accumulé est versé d'un bloc.
        view.attribute(&k, 10123, now);
        let deltas = view.drain_deltas();
        assert_eq!(deltas.len(), 3, "nœud app + nœud hôte + arête");
        match find_node(&deltas, "app:10123").unwrap() {
            Delta::UpsertNode {
                bytes,
                bytes_in,
                bytes_out,
                packets,
                proto,
                ..
            } => {
                assert_eq!(
                    (*bytes, *bytes_in, *bytes_out, *packets),
                    (500, 400, 100, 2)
                );
                assert_eq!(proto, "HTTPS");
            }
            _ => unreachable!(),
        }
        // Perspective inversée pour l'hôte distant.
        match find_node(&deltas, "93.184.216.34").unwrap() {
            Delta::UpsertNode {
                bytes_in,
                bytes_out,
                ..
            } => assert_eq!((*bytes_in, *bytes_out), (100, 400)),
            _ => unreachable!(),
        }

        // Paquets suivants : attribution en cache, application directe.
        assert_eq!(view.ingest(k, 50, HTTPS, true, now), None);
        let deltas = view.drain_deltas();
        match find_node(&deltas, "app:10123").unwrap() {
            Delta::UpsertNode { bytes, .. } => assert_eq!(*bytes, 550),
            _ => unreachable!(),
        }
    }

    #[test]
    fn expired_pending_goes_to_unknown_but_future_is_corrected() {
        let mut view = AppView::new();
        let t0 = Instant::now();
        let k = key("93.184.216.34", 443);

        view.ingest(k, 100, HTTPS, true, t0);
        view.expire_pending(t0 + Duration::from_secs(1), Duration::from_millis(500));

        let deltas = view.drain_deltas();
        match find_node(&deltas, &app_id(UNKNOWN_UID)).unwrap() {
            Delta::UpsertNode { label, bytes, .. } => {
                assert_eq!(label, "Unknown");
                assert_eq!(*bytes, 100);
            }
            _ => unreachable!(),
        }

        // Réponse tardive : le passé reste sous Unknown, l'avenir est correct.
        view.attribute(&k, 10123, t0 + Duration::from_secs(2));
        view.ingest(k, 60, HTTPS, true, t0 + Duration::from_secs(2));
        let deltas = view.drain_deltas();
        match find_node(&deltas, "app:10123").unwrap() {
            Delta::UpsertNode { bytes, .. } => assert_eq!(*bytes, 60),
            _ => unreachable!(),
        }
    }

    #[test]
    fn pending_overflow_evicts_oldest_to_unknown() {
        let mut view = AppView::new();
        let t0 = Instant::now();

        for i in 0..PENDING_CAPACITY {
            let k = key("203.0.113.7", 1000 + i as u16);
            view.ingest(k, 10, HTTPS, true, t0 + Duration::from_millis(i as u64));
        }
        assert!(view.drain_deltas().is_empty(), "tout est en attente");

        // Un flux de plus : le doyen (port 1000) est versé sous Unknown.
        let extra = key("203.0.113.7", 9999);
        view.ingest(extra, 10, HTTPS, true, t0 + Duration::from_secs(1));
        let deltas = view.drain_deltas();
        assert!(find_node(&deltas, &app_id(UNKNOWN_UID)).is_some());

        // Le doyen évincé garde une attribution tardive fonctionnelle : son
        // arrivée n'a pas cassé les autres flux en attente.
        view.attribute(
            &key("203.0.113.7", 1001),
            10042,
            t0 + Duration::from_secs(1),
        );
        let deltas = view.drain_deltas();
        assert!(find_node(&deltas, "app:10042").is_some());
    }

    #[test]
    fn unattributable_packets_go_straight_to_unknown() {
        let mut view = AppView::new();
        let now = Instant::now();
        view.ingest_unattributable("10.0.0.2".parse().unwrap(), 60, ICMP, true, now);

        let deltas = view.drain_deltas();
        assert_eq!(deltas.len(), 3);
        match find_node(&deltas, &app_id(UNKNOWN_UID)).unwrap() {
            Delta::UpsertNode { proto, .. } => assert_eq!(proto, "ICMP"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn app_label_re_dirties_and_survives_fade_and_reset() {
        let mut view = AppView::new();
        let t0 = Instant::now();
        let k = key("93.184.216.34", 443);
        view.ingest(k, 100, HTTPS, true, t0);
        view.attribute(&k, 10123, t0);
        view.drain_deltas();

        view.set_app_label(10123, "Firefox".into());
        let deltas = view.drain_deltas();
        assert_eq!(deltas.len(), 1, "seul le nœud renommé repart");
        match &deltas[0] {
            Delta::UpsertNode { label, .. } => assert_eq!(label, "Firefox"),
            _ => unreachable!(),
        }

        // Fade complet : suppressions explicites, arêtes avant nœuds.
        let deltas = view.fade_sweep(t0 + Duration::from_secs(120), Duration::from_secs(60));
        assert_eq!(deltas.len(), 3, "1 arête + 2 nœuds");
        assert!(matches!(deltas[0], Delta::RemoveEdge { .. }));

        // Reset + retour du trafic : le label (cache) est réutilisé — mais le
        // cache d'attribution ayant été purgé par le fade, le flux repasse
        // par l'attente.
        view.reset();
        assert_eq!(view.ingest(k, 10, HTTPS, true, t0), Some(k));
        view.attribute(&k, 10123, t0);
        let deltas = view.drain_deltas();
        match find_node(&deltas, "app:10123").unwrap() {
            Delta::UpsertNode { label, bytes, .. } => {
                assert_eq!(label, "Firefox");
                assert_eq!(*bytes, 10, "compteurs repartis de zéro");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn snapshot_returns_full_state() {
        let mut view = AppView::new();
        let now = Instant::now();
        let k = key("93.184.216.34", 443);
        view.ingest(k, 100, HTTPS, true, now);
        view.attribute(&k, 10123, now);
        view.drain_deltas();

        assert!(view.drain_deltas().is_empty());
        assert_eq!(view.snapshot_deltas().len(), 3);
    }
}
