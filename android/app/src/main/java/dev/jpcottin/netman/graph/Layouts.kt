package dev.jpcottin.netman.graph

import kotlin.math.PI
import kotlin.math.max

/** Position calculée d'un nœud (repère graphe, y vers le bas comme sigma). */
data class Point(val x: Float, val y: Float)

/** Cercle guide (repère graphe). */
data class GuideCircle(val cx: Float, val cy: Float, val r: Float)

data class LayoutResult(
    val positions: Map<String, Point>,
    val guides: List<GuideCircle>,
)

/**
 * Placement déterministe — port fidèle de `static/app.js` (ForceAtlas2 câblé
 * mais inutilisé). Fonctions pures : cœur des tests JVM.
 */
object Layouts {
    private const val TWO_PI = (2.0 * PI).toFloat()
    private const val START = (-PI / 2.0).toFloat() // premier nœud à midi

    /** Appman (et Etherman) : tous les nœuds sur un grand cercle, r = 100. */
    fun circle(ids: List<String>, radius: Float = 100f): LayoutResult {
        val sorted = ids.sorted()
        val n = sorted.size
        val positions = HashMap<String, Point>(n)
        sorted.forEachIndexed { i, id ->
            val angle = TWO_PI * i / n + START
            positions[id] = Point(radius * cos(angle), radius * sin(angle))
        }
        val guides = if (n > 0) listOf(GuideCircle(0f, 0f, radius)) else emptyList()
        return LayoutResult(positions, guides)
    }

    /**
     * Interman : un cercle par réseau, réseaux répartis sur un anneau.
     * `networkKey` classe côté client (le serveur n'envoie pas de groupe).
     */
    fun networks(ids: List<String>): LayoutResult {
        if (ids.isEmpty()) return LayoutResult(emptyMap(), emptyList())

        val groups = LinkedHashMap<String, MutableList<String>>()
        for (id in ids) groups.getOrPut(networkKey(id)) { mutableListOf() }.add(id)
        val keys = groups.keys.sorted()

        val clusterRadii = keys.associateWith { clusterRadius(groups.getValue(it).size) }
        val maxRadius = clusterRadii.values.maxOrNull() ?: 0f
        val networkCount = keys.size
        val ringRadius = if (networkCount == 1) 0f
        else max(150f, networkCount * (2 * maxRadius + 50f) / TWO_PI)

        val positions = HashMap<String, Point>(ids.size)
        val guides = ArrayList<GuideCircle>()
        keys.forEachIndexed { ki, key ->
            val netAngle = TWO_PI * ki / networkCount + START
            val cx = ringRadius * cos(netAngle)
            val cy = ringRadius * sin(netAngle)
            val members = groups.getValue(key).sortedWith(::compareNodeIds)
            val r = clusterRadii.getValue(key)
            if (r > 0f) guides.add(GuideCircle(cx, cy, r))
            val m = members.size
            members.forEachIndexed { i, id ->
                if (r == 0f) {
                    positions[id] = Point(cx, cy)
                } else {
                    val a = TWO_PI * i / m + START
                    positions[id] = Point(cx + r * cos(a), cy + r * sin(a))
                }
            }
        }
        return LayoutResult(positions, guides)
    }

    fun clusterRadius(count: Int): Float =
        if (count == 1) 0f else max(16f, count * 10f / TWO_PI)

    /** Clé de réseau d'un id de nœud (IP ou "app:<uid>" — ce dernier n'arrive
     *  jamais ici, la vue Appman utilise [circle]). */
    fun networkKey(id: String): String {
        if (id.contains(':')) {
            // IPv6 : /64 sur les 4 premiers groupes.
            val groups = expandIpv6(id)
            return "v6 " + groups.take(4).joinToString(":") + "::/64"
        }
        val o = id.split('.').mapNotNull { it.toIntOrNull() }
        if (o.size != 4) return "other"
        return when {
            o[0] < 128 -> "${o[0]}.0.0.0/8"
            o[0] < 192 -> "${o[0]}.${o[1]}.0.0/16"
            o[0] < 224 -> "${o[0]}.${o[1]}.${o[2]}.0/24"
            else -> "multicast & reserved"
        }
    }

    /** Tri des membres : IPv4 numérique octet par octet, sinon lexicographique. */
    fun compareNodeIds(a: String, b: String): Int {
        val oa = a.split('.').mapNotNull { it.toIntOrNull() }
        val ob = b.split('.').mapNotNull { it.toIntOrNull() }
        if (oa.size == 4 && ob.size == 4) {
            for (i in 0 until 4) {
                val c = oa[i].compareTo(ob[i])
                if (c != 0) return c
            }
            return 0
        }
        return a.compareTo(b)
    }

    /** Développe le `::` d'une IPv6 en 8 groupes hexadécimaux. */
    fun expandIpv6(ip: String): List<String> {
        val bare = ip.substringBefore('%') // enlève l'éventuelle zone
        val parts = bare.split("::", limit = 2)
        val head = parts[0].split(':').filter { it.isNotEmpty() }
        val tail = if (parts.size == 2) parts[1].split(':').filter { it.isNotEmpty() } else emptyList()
        return if (parts.size == 2) {
            val fill = List(max(0, 8 - head.size - tail.size)) { "0" }
            (head + fill + tail).map { it.ifEmpty { "0" } }
        } else {
            head.map { it.ifEmpty { "0" } }
        }
    }

    // Trig sans importer kotlin.math partout.
    private fun cos(a: Float): Float = kotlin.math.cos(a)
    private fun sin(a: Float): Float = kotlin.math.sin(a)
}
