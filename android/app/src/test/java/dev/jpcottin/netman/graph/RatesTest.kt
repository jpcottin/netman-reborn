package dev.jpcottin.netman.graph

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Port fidèle des calculs de débit de `static/app.js`. */
class RatesTest {

    @Test
    fun ewma_below_dt_threshold_keeps_state() {
        val prev = EwmaState(rate = 500.0, prevBytes = 1000, prevTime = 10.0)
        val next = Rates.updateEwma(prev.rate, prev.prevBytes, prev.prevTime, 2000, 10.02)
        // dt = 0.02 < 0.05 : rien ne change.
        assertEquals(prev, next)
    }

    @Test
    fun ewma_converges_toward_instantaneous_rate() {
        // 1000 octets en 1 s = 1000 o/s instantané ; l'EWMA s'en approche.
        var st = EwmaState(0.0, 0, 0.0)
        repeat(20) { i ->
            st = Rates.updateEwma(st.rate, st.prevBytes, st.prevTime, 1000L * (i + 1), (i + 1).toDouble())
        }
        assertEquals(1000.0, st.rate, 50.0)
    }

    @Test
    fun decay_only_after_two_seconds_of_silence() {
        assertEquals(100.0, Rates.decay(100.0, lastTime = 5.0, now = 6.5), 1e-9) // < 2 s
        val decayed = Rates.decay(100.0, lastTime = 5.0, now = 8.0) // > 2 s
        assertTrue(decayed < 100.0)
        assertEquals(100.0 * Rates.RATE_DECAY, decayed, 1e-9)
    }

    @Test
    fun decay_leaves_tiny_rates_untouched() {
        assertEquals(0.5, Rates.decay(0.5, lastTime = 0.0, now = 100.0), 1e-9)
    }

    @Test
    fun avgRate_uses_window_with_half_second_floor() {
        assertEquals(1000.0, Rates.avgRate(current = 1000, first = 0, firstTime = 0.0, lastTime = 1.0), 1e-9)
        // Fenêtre < 0,5 s : plancher à 0,5 s.
        assertEquals(2000.0, Rates.avgRate(current = 1000, first = 0, firstTime = 0.0, lastTime = 0.1), 1e-9)
        // Compteur en recul (jamais) : borné à 0.
        assertEquals(0.0, Rates.avgRate(current = 10, first = 100, firstTime = 0.0, lastTime = 1.0), 1e-9)
    }

    @Test
    fun formatRate_picks_the_right_unit() {
        assertEquals("0 bit/s", Rates.formatRate(0.0))
        assertEquals("800 bit/s", Rates.formatRate(100.0)) // 100 o/s = 800 bit/s
        assertEquals("8.0 kbit/s", Rates.formatRate(1000.0))
        assertEquals("8.0 Mbit/s", Rates.formatRate(1_000_000.0))
        assertEquals("8.00 Gbit/s", Rates.formatRate(1_000_000_000.0))
    }
}
