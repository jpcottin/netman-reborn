package dev.jpcottin.netman.graph

import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.roundToLong

/**
 * Débits calculés côté client — port fidèle de `static/app.js`. Le cœur
 * n'envoie que des compteurs absolus ; le débit (EWMA) et les moyennes sont
 * dérivés ici. Fonctions pures : testées sur la JVM.
 */
object Rates {
    const val RATE_TAU = 3.0
    val RATE_DECAY = exp(-1.0 / RATE_TAU)

    /**
     * Met à jour l'EWMA d'un débit (octets/s) à partir des compteurs absolus.
     * Reproduit le garde `dt >= 0.05` : sous ce seuil, on garde l'état.
     */
    fun updateEwma(
        prevRate: Double,
        prevBytes: Long,
        prevTime: Double,
        bytes: Long,
        now: Double,
    ): EwmaState {
        val dt = now - prevTime
        if (dt < 0.05) return EwmaState(prevRate, prevBytes, prevTime)
        val inst = max(0L, bytes - prevBytes).toDouble() / dt
        val alpha = 1.0 - exp(-dt / RATE_TAU)
        val rate = prevRate + alpha * (inst - prevRate)
        return EwmaState(rate, bytes, now)
    }

    /** Décroissance des arêtes silencieuses (> 2 s), 1×/s. */
    fun decay(rate: Double, lastTime: Double, now: Double): Double =
        if (rate > 1.0 && now - lastTime > 2.0) rate * RATE_DECAY else rate

    /** Débit moyen sur la fenêtre premier→dernier paquet (min 0,5 s). */
    fun avgRate(current: Long, first: Long, firstTime: Double, lastTime: Double): Double =
        max(0L, current - first).toDouble() / max(lastTime - firstTime, 0.5)

    /** Formatage en bits/s → Gbit/s (mêmes seuils que le client web). */
    fun formatRate(bytesPerSec: Double): String {
        val bps = bytesPerSec * 8.0
        return when {
            bps < 1000 -> "${bps.roundToLong()} bit/s"
            bps < 1_000_000 -> "${round1(bps / 1000)} kbit/s"
            bps < 1_000_000_000 -> "${round1(bps / 1_000_000)} Mbit/s"
            else -> "${round2(bps / 1_000_000_000)} Gbit/s"
        }
    }

    private fun round1(x: Double): String = String.format("%.1f", x)
    private fun round2(x: Double): String = String.format("%.2f", x)
    private fun log2(x: Double): Double = ln(x) / ln(2.0)
}

data class EwmaState(val rate: Double, val prevBytes: Long, val prevTime: Double)
