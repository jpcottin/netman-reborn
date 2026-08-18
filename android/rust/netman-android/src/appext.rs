//! Branchement de la vue Appman sur l'agrégateur partagé (`netman::agg`).
//!
//! `AppExt` projette chaque `PacketMeta` sur la vue par application : le sens
//! (sortant/entrant) se déduit des adresses du tun, l'attribution uid passe
//! par des channels vers la tâche résolveur (les appels binder de Kotlin ne
//! sont jamais sur le chemin des paquets — invariant 4).

use std::net::IpAddr;
use std::time::{Duration, Instant};

use netman::model::apps::{AppView, FlowKey, Uid};
use netman::model::packet::PacketMeta;
use netman::wsproto::Delta;
use tokio::sync::mpsc;

/// Attente maximale d'une attribution : deux ticks d'agrégateur. Au-delà, le
/// flux part sous « Unknown » (une réponse tardive corrige l'avenir).
const PENDING_MAX_WAIT: Duration = Duration::from_millis(500);

pub struct AppExt {
    view: AppView,
    /// Adresses du tun : un paquet dont la source en fait partie est sortant.
    device_addrs: Vec<IpAddr>,
    /// Demandes d'attribution vers la tâche résolveur (bounded ; un échec
    /// d'envoi laisse le flux expirer vers « Unknown » — jamais bloquant).
    uid_req_tx: mpsc::Sender<FlowKey>,
    uid_res_rx: mpsc::Receiver<(FlowKey, Uid)>,
    label_res_rx: mpsc::Receiver<(Uid, String)>,
}

impl AppExt {
    pub fn new(
        device_addrs: Vec<IpAddr>,
        uid_req_tx: mpsc::Sender<FlowKey>,
        uid_res_rx: mpsc::Receiver<(FlowKey, Uid)>,
        label_res_rx: mpsc::Receiver<(Uid, String)>,
    ) -> Self {
        AppExt {
            view: AppView::new(),
            device_addrs,
            uid_req_tx,
            uid_res_rx,
            label_res_rx,
        }
    }
}

impl netman::agg::AggExt for AppExt {
    fn ingest(&mut self, meta: &PacketMeta, now: Instant) {
        let Some(l3) = &meta.l3 else { return };
        let outbound = self.device_addrs.contains(&l3.src);
        let remote = if outbound { l3.dst } else { l3.src };
        match &l3.transport {
            Some(t) => {
                let key = if outbound {
                    FlowKey {
                        ip_number: t.ip_number,
                        local: l3.src,
                        local_port: t.src_port,
                        remote: l3.dst,
                        remote_port: t.dst_port,
                    }
                } else {
                    FlowKey {
                        ip_number: t.ip_number,
                        local: l3.dst,
                        local_port: t.dst_port,
                        remote: l3.src,
                        remote_port: t.src_port,
                    }
                };
                if let Some(request) = self
                    .view
                    .ingest(key, meta.wire_len, l3.proto, outbound, now)
                {
                    // File pleine ⇒ demande perdue : le flux expirera vers
                    // « Unknown », on ne bloque jamais ici.
                    let _ = self.uid_req_tx.try_send(request);
                }
            }
            None => self
                .view
                .ingest_unattributable(remote, meta.wire_len, l3.proto, outbound, now),
        }
    }

    fn tick(&mut self, now: Instant, max_age: Duration) -> Vec<Delta> {
        // Réponses du résolveur d'abord : les flux réglés partent dans le
        // même lot de deltas.
        while let Ok((key, uid)) = self.uid_res_rx.try_recv() {
            self.view.attribute(&key, uid, now);
        }
        while let Ok((uid, label)) = self.label_res_rx.try_recv() {
            self.view.set_app_label(uid, label);
        }
        self.view.expire_pending(now, PENDING_MAX_WAIT);

        let mut deltas = self.view.drain_deltas();
        deltas.extend(self.view.fade_sweep(now, max_age));
        deltas
    }

    fn snapshot(&self) -> Vec<Delta> {
        self.view.snapshot_deltas()
    }

    fn reset(&mut self) {
        self.view.reset();
    }

    fn host_resolved(&mut self, ip: IpAddr, name: &str) {
        self.view.set_host_label(ip, name.to_string());
    }
}
