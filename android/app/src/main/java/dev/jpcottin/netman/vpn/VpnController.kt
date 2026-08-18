package dev.jpcottin.netman.vpn

import android.content.Context
import android.net.VpnService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Passerelle IHM ↔ service, dans le même processus (pas de binder). Le service
 * écrit l'état, Compose l'observe.
 */
object VpnController {
    private val _state = MutableStateFlow<VpnState>(VpnState.Idle)
    val state: StateFlow<VpnState> = _state

    fun update(state: VpnState) {
        _state.value = state
    }

    /** `null` = consentement déjà donné ; sinon l'Intent à lancer. */
    fun consentIntent(context: Context) = VpnService.prepare(context)

    fun start(context: Context) {
        NetmanVpnService.start(context)
    }

    fun stop(context: Context) {
        NetmanVpnService.stop(context)
    }
}
