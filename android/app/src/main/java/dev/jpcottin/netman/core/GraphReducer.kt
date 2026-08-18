package dev.jpcottin.netman.core

import dev.jpcottin.netman.graph.GraphStore

/** Résultat de l'application d'un lot : nouvelle config/fade, protocoles vus,
 *  et si un reset a eu lieu (l'IHM s'en sert éventuellement). */
data class ReduceResult(
    val fadeSecs: Long?,
    val seenProtos: Set<String>,
    val sawReset: Boolean,
)

/**
 * Réduction pure des deltas sur deux vues — extraite de [GraphRepository] pour
 * être testable sans coroutine ni dispatcher. Aucune I/O, aucun état global.
 */
object GraphReducer {

    /**
     * Applique un lot de messages JSON. Mute [appView]/[interView] en place.
     * @param paused si vrai, les mutations de graphe sont ignorées (config et
     *   reset restent appliqués, comme le client web).
     */
    fun apply(
        appView: GraphStore,
        interView: GraphStore,
        batch: List<String>,
        seen: Set<String>,
        paused: Boolean,
    ): ReduceResult {
        var fade: Long? = null
        var sawReset = false
        val newSeen = seen.toMutableSet()

        for (text in batch) {
            when (val msg = DeltaCodec.decode(text)) {
                is DeltaMessage.Config -> fade = msg.fadeSecs
                is DeltaMessage.Reset -> {
                    appView.clear()
                    interView.clear()
                    sawReset = true
                }
                null -> {}
                else -> if (!paused) applyDelta(appView, interView, msg, newSeen)
            }
        }
        return ReduceResult(fade, newSeen, sawReset)
    }

    private fun applyDelta(
        appView: GraphStore,
        interView: GraphStore,
        msg: DeltaMessage,
        seen: MutableSet<String>,
    ) {
        when (msg) {
            is DeltaMessage.UpsertNode -> {
                store(appView, interView, msg.view)?.upsertNode(
                    msg.id, msg.label, msg.bytes, msg.bytesIn, msg.bytesOut, msg.packets, msg.proto,
                )
                registerProto(msg.proto, seen)
            }
            is DeltaMessage.UpsertEdge -> {
                store(appView, interView, msg.view)?.upsertEdge(
                    msg.id, msg.source, msg.target, msg.bytes, msg.packets, msg.proto,
                )
                registerProto(msg.proto, seen)
            }
            is DeltaMessage.RemoveNode -> store(appView, interView, msg.view)?.removeNode(msg.id)
            is DeltaMessage.RemoveEdge -> store(appView, interView, msg.view)?.removeEdge(msg.id)
            else -> {}
        }
    }

    private fun registerProto(proto: String, seen: MutableSet<String>) {
        if (proto.isNotBlank() && proto != "?") seen.add(proto)
    }

    private fun store(appView: GraphStore, interView: GraphStore, view: String): GraphStore? =
        when (view) {
            Views.APP -> appView
            Views.INTER -> interView
            else -> null // vue inconnue ignorée (cf. contrat)
        }
}
