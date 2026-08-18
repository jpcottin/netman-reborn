package dev.jpcottin.netman.ui

import dev.jpcottin.netman.graph.GraphStore

/** Une ligne de panneau : un nœud, prêt à afficher (dérivé du GraphStore). */
data class NodeRow(
    val id: String,
    val label: String,
    val bytes: Long,
    val proto: String,
    val edgeCount: Int,
)

/** Nœud figé pour le rendu Canvas (aucune référence au GraphStore mutable). */
data class VizNode(
    val id: String,
    val label: String,
    val bytes: Long,
    val bytesIn: Long,
    val bytesOut: Long,
    val proto: String,
)

/** Arête figée pour le rendu Canvas. */
data class VizEdge(
    val id: String,
    val source: String,
    val target: String,
    val bytes: Long,
    val proto: String,
)

/** Instantané immuable d'une vue, prêt à dessiner. `signature` change dès que
 *  l'ensemble des nœuds/arêtes change (déclenche un recalcul de layout). */
data class GraphSnapshot(
    val nodes: List<VizNode>,
    val edges: List<VizEdge>,
) {
    val signature: Int = run {
        var h = 1
        for (n in nodes) h = h * 31 + n.id.hashCode()
        h = h * 131 + edges.size
        for (e in edges) h = h * 31 + e.id.hashCode()
        h
    }
}

/** Projette un GraphStore en instantané de rendu. */
fun GraphStore.toSnapshot(): GraphSnapshot = GraphSnapshot(
    nodes = nodes.values.map {
        VizNode(it.id, it.label, it.bytes, it.bytesIn, it.bytesOut, it.proto)
    },
    edges = edges.values.map { VizEdge(it.id, it.source, it.target, it.bytes, it.proto) },
)

/** Projette un GraphStore en lignes triées par octets décroissants — la
 *  représentation « liste » des jalons 4-6, avant les panneaux Canvas. */
fun GraphStore.toRows(): List<NodeRow> {
    val degree = HashMap<String, Int>()
    for (e in edges.values) {
        degree[e.source] = (degree[e.source] ?: 0) + 1
        degree[e.target] = (degree[e.target] ?: 0) + 1
    }
    return nodes.values
        .map { NodeRow(it.id, it.label, it.bytes, it.proto, degree[it.id] ?: 0) }
        .sortedByDescending { it.bytes }
}
