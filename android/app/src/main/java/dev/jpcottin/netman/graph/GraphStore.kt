package dev.jpcottin.netman.graph

/** Nœud d'un graphe : compteurs cumulés + état de placement (rempli plus tard
 *  par le moteur de layout des panneaux Canvas). */
class NodeState(
    val id: String,
    var label: String,
    var bytes: Long = 0,
    var bytesIn: Long = 0,
    var bytesOut: Long = 0,
    var packets: Long = 0,
    var proto: String = "?",
    // Position/animation (renseignées par le layout ; jalons 7-8).
    var x: Float = 0f,
    var y: Float = 0f,
    var targetX: Float = 0f,
    var targetY: Float = 0f,
    var placed: Boolean = false,
)

/** Arête d'un graphe. Le débit (EWMA, octets/s) est calculé côté client au fil
 *  des upserts à partir des compteurs absolus — port de `static/app.js`. */
class EdgeState(
    val id: String,
    val source: String,
    val target: String,
    var bytes: Long = 0,
    var packets: Long = 0,
    var proto: String = "?",
    // État du débit lissé (EWMA) : voir Rates.updateEwma / Rates.decay.
    var rate: Double = 0.0,
    var prevBytes: Long = 0,
    var prevTime: Double = 0.0,
    var seeded: Boolean = false,
)

/** Un graphe (une vue) : nœuds + arêtes indexés par id. Muté uniquement par
 *  le réducteur (thread principal) — aucun verrou. */
class GraphStore {
    val nodes = LinkedHashMap<String, NodeState>()
    val edges = LinkedHashMap<String, EdgeState>()

    fun upsertNode(
        id: String,
        label: String,
        bytes: Long,
        bytesIn: Long,
        bytesOut: Long,
        packets: Long,
        proto: String,
    ) {
        val node = nodes.getOrPut(id) { NodeState(id, label) }
        node.label = label
        node.bytes = bytes
        node.bytesIn = bytesIn
        node.bytesOut = bytesOut
        node.packets = packets
        node.proto = proto
    }

    /** Crée un nœud-souche si une arête arrive avant ses extrémités. */
    private fun ensureEndpoint(id: String) {
        nodes.getOrPut(id) { NodeState(id, id) }
    }

    fun upsertEdge(
        id: String,
        source: String,
        target: String,
        bytes: Long,
        packets: Long,
        proto: String,
        now: Double = 0.0,
    ) {
        ensureEndpoint(source)
        ensureEndpoint(target)
        val edge = edges.getOrPut(id) { EdgeState(id, source, target) }
        // Débit lissé : amorcé au premier upsert (rate 0), puis EWMA sur les
        // deltas de compteurs absolus (garde dt >= 0,05 s dans updateEwma).
        if (!edge.seeded) {
            edge.seeded = true
            edge.prevBytes = bytes
            edge.prevTime = now
            edge.rate = 0.0
        } else {
            val st = Rates.updateEwma(edge.rate, edge.prevBytes, edge.prevTime, bytes, now)
            edge.rate = st.rate
            edge.prevBytes = st.prevBytes
            edge.prevTime = st.prevTime
        }
        edge.bytes = bytes
        edge.packets = packets
        edge.proto = proto
    }

    /** Décroissance 1 Hz des arêtes silencieuses (port du ticker de app.js). */
    fun decayRates(now: Double) {
        for (e in edges.values) e.rate = Rates.decay(e.rate, e.prevTime, now)
    }

    fun removeNode(id: String) {
        nodes.remove(id)
    }

    fun removeEdge(id: String) {
        edges.remove(id)
    }

    fun clear() {
        nodes.clear()
        edges.clear()
    }
}
