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

/** Arête d'un graphe. Le débit (EWMA) est calculé côté client au fil des
 *  upserts (jalon 7) ; ici on conserve les cumuls absolus. */
class EdgeState(
    val id: String,
    val source: String,
    val target: String,
    var bytes: Long = 0,
    var packets: Long = 0,
    var proto: String = "?",
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
    ) {
        ensureEndpoint(source)
        ensureEndpoint(target)
        val edge = edges.getOrPut(id) { EdgeState(id, source, target) }
        edge.bytes = bytes
        edge.packets = packets
        edge.proto = proto
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
