package dev.jpcottin.netman.core

import android.content.Context
import android.net.ConnectivityManager
import android.os.Build
import android.system.OsConstants
import dev.jpcottin.netman.attribution.AppInfoCache
import java.net.InetSocketAddress
import uniffi.netman_android.AppLabel
import uniffi.netman_android.NetmanCallbacks

/**
 * Implémentation Kotlin des callbacks attendus par le cœur Rust. Règles de
 * thread (voir la doc du trait côté Rust) :
 *  - `onDeltas` : n'enfile que (GraphRepository.offer), jamais d'UI ;
 *  - `protectSocket` : synchrone, bref (VpnService.protect) ;
 *  - `lookupUid` / `resolveApp` : appels binder, déjà hors chemin paquet
 *    (le Rust les émet depuis sa tâche résolveur bornée).
 */
class CoreBridge(
    context: Context,
    private val protect: (Int) -> Boolean,
) : NetmanCallbacks {

    private val appContext = context.applicationContext
    private val connectivity =
        appContext.getSystemService(ConnectivityManager::class.java)
    private val appInfo = AppInfoCache(appContext.packageManager)

    override fun onDeltas(deltas: List<String>) {
        GraphRepository.offer(deltas)
    }

    override fun protectSocket(fd: Int): Boolean = protect(fd)

    override fun lookupUid(
        ipNumber: UByte,
        local: String,
        localPort: UShort,
        remote: String,
        remotePort: UShort,
    ): Int {
        val cm = connectivity ?: return INVALID_UID
        val protocol = when (ipNumber.toInt()) {
            6 -> OsConstants.IPPROTO_TCP
            17 -> OsConstants.IPPROTO_UDP
            else -> return INVALID_UID
        }
        return runCatching {
            cm.getConnectionOwnerUid(
                protocol,
                InetSocketAddress(local, localPort.toInt()),
                InetSocketAddress(remote, remotePort.toInt()),
            )
        }.getOrDefault(INVALID_UID)
    }

    override fun resolveApp(uid: Int): AppLabel? {
        val info = appInfo.get(uid)
        return AppLabel(label = info.label, `package` = info.packageName)
    }

    companion object {
        // android.net.INVALID_UID (= -1) sans dépendre du niveau d'API.
        const val INVALID_UID = -1

        val getConnectionOwnerUidSupported: Boolean
            get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q
    }
}
