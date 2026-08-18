package dev.jpcottin.netman.vpn

/** État de la capture, observé par l'IHM. */
sealed interface VpnState {
    data object Idle : VpnState
    data object Starting : VpnState
    data class Running(
        val frames: Long = 0,
        val bytes: Long = 0,
        val chanDrops: Long = 0,
    ) : VpnState
    data object Revoked : VpnState
    data class Error(val message: String) : VpnState
}
