package dev.jpcottin.netman.core

import dev.jpcottin.netman.graph.GraphStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/**
 * Cœur d'état côté application, singleton de processus (la capture survit à la
 * fermeture de l'IHM). Le callback Rust ne fait qu'ENFILER le JSON ; un unique
 * réducteur (thread principal) draine, décode et applique — aucun verrou entre
 * réducteur et rendu, la capture n'est jamais bloquée.
 */
object GraphRepository {
    /** Vues séparées, indexées par le champ `view` du protocole. */
    val appView = GraphStore()
    val interView = GraphStore()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** File non bornée : Kotlin ne doit pas perdre de deltas (le Rust coalesce
     *  déjà par tick, le volume est borné à quelques centaines/tick). */
    private val inbox = Channel<List<String>>(Channel.UNLIMITED)

    @Volatile
    var paused: Boolean = false

    private val _fadeSecs = MutableStateFlow(60L)
    val fadeSecs: StateFlow<Long> = _fadeSecs

    /** Incrémenté après chaque lot appliqué : seule source d'invalidation du
     *  rendu Compose (les panneaux lisent `frameTick` puis relisent le store). */
    private val _frameTick = MutableStateFlow(0L)
    val frameTick: StateFlow<Long> = _frameTick

    /** Protocoles rencontrés (peuple le filtre, comme le client web). */
    private val _seenProtos = MutableStateFlow<Set<String>>(emptySet())
    val seenProtos: StateFlow<Set<String>> = _seenProtos

    init {
        scope.launch {
            for (batch in inbox) {
                applyBatch(batch)
            }
        }
    }

    /** Appelé depuis le thread du callback Rust : enfile et rend la main. */
    fun offer(deltas: List<String>) {
        inbox.trySend(deltas)
    }

    /** Remplace l'état par un snapshot complet (reprise après pause/réouverture). */
    fun resetTo(snapshot: List<String>) {
        appView.clear()
        interView.clear()
        offer(snapshot)
    }

    private fun applyBatch(batch: List<String>) {
        val result = GraphReducer.apply(appView, interView, batch, _seenProtos.value, paused)
        result.fadeSecs?.let { _fadeSecs.value = it }
        if (result.seenProtos.size != _seenProtos.value.size) {
            _seenProtos.value = result.seenProtos
        }
        _frameTick.value += 1
    }
}
