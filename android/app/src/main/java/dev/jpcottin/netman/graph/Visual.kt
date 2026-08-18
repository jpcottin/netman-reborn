package dev.jpcottin.netman.graph

import androidx.compose.ui.graphics.Color
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min

/**
 * Correspondances visuelles — port fidèle de `static/app.js` (mapping fixe,
 * documenté, légende obligatoire). Fonctions pures : cœur des tests JVM.
 */
object Visual {

    /** Palette protocole (nœud opaque ; arête ~55 % via [EDGE_ALPHA]). */
    val PROTO_COLORS: Map<String, Color> = linkedMapOf(
        "IPv4" to Color(0xFF4C9BE8),
        "IPv6" to Color(0xFF8E6EE8),
        "ARP" to Color(0xFFF1C40F),
        "HTTP" to Color(0xFF2ECC71),
        "HTTPS" to Color(0xFF27AE60),
        "QUIC" to Color(0xFF1ABC9C),
        "DNS" to Color(0xFFF39C12),
        "mDNS" to Color(0xFFE67E22),
        "LLMNR" to Color(0xFFE67E22),
        "SSDP" to Color(0xFFD35400),
        "TCP" to Color(0xFF3498DB),
        "UDP" to Color(0xFF9B59B6),
        "ICMP" to Color(0xFFE74C3C),
        "ICMPv6" to Color(0xFFC0392B),
        "SSH" to Color(0xFF16A085),
        "SMB" to Color(0xFFE84C9B),
        "NetBIOS" to Color(0xFFE84C9B),
        "DHCP" to Color(0xFF7F8C8D),
        "DHCPv6" to Color(0xFF7F8C8D),
        "NTP" to Color(0xFF95A5A6),
        "IGMP" to Color(0xFFA04000),
    )

    val OTHER_COLOR = Color(0xFF697386)
    val DIMMED_COLOR = Color(0xFF2A3040)
    const val EDGE_ALPHA = 0.549f // 0x8c/0xff

    // Thème sombre (identique au client web).
    val BG = Color(0xFF0B0E14)
    val PANEL_BG = Color(0xFF10141D)
    val PANEL_BORDER = Color(0xFF232A38)
    val FG = Color(0xFFD5DBE5)
    val MUTED = Color(0xFF7A8496)
    val ACCENT = Color(0xFF4C9BE8)
    val GUIDE = Color(0x38A5AFBE) // rgba(165,175,190,0.22)

    fun protoColor(proto: String): Color = PROTO_COLORS[proto] ?: OTHER_COLOR

    fun edgeColor(proto: String): Color = protoColor(proto).copy(alpha = EDGE_ALPHA)

    /** Taille de nœud ∝ log2(octets cumulés), bornée [2, 18]. */
    fun nodeSize(bytes: Long): Float {
        val v = 2.0 + log2(1.0 + bytes.toDouble()) * 0.75
        return min(18.0, max(2.0, v)).toFloat()
    }

    /** Épaisseur d'arête ∝ log2(1 + débit/100), amplifiée par `scale`, bornée [.,12]. */
    fun edgeWidth(rate: Double, scale: Float): Float {
        val v = 0.4 + 0.35 * scale * log2(1.0 + rate / 100.0)
        return min(12.0, v).toFloat()
    }

    private fun log2(x: Double): Double = ln(x) / LN2
    private val LN2 = ln(2.0)
}
