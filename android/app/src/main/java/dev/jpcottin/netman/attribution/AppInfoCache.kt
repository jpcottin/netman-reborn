package dev.jpcottin.netman.attribution

import android.content.pm.PackageManager
import android.os.Process

/** Identité d'une application, mémorisée par uid (stable par démarrage). */
data class AppInfo(val label: String, val packageName: String)

/**
 * Cache uid → identité. Les uids système ont des noms fixes ; le reste passe
 * par PackageManager. Sans correspondance : « Unknown ».
 */
class AppInfoCache(private val pm: PackageManager) {
    private val cache = HashMap<Int, AppInfo>()

    @Synchronized
    fun get(uid: Int): AppInfo = cache.getOrPut(uid) { resolve(uid) }

    private fun resolve(uid: Int): AppInfo = when {
        uid < 0 -> AppInfo("Unknown", "")
        uid == 0 -> AppInfo("Root", "root")
        uid == Process.SYSTEM_UID -> AppInfo("Android System", "android")
        else -> {
            val packages = pm.getPackagesForUid(uid)
            val pkg = packages?.firstOrNull()
            if (pkg == null) {
                // netd (DNS) et services partagés retombent ici.
                AppInfo("System ($uid)", "")
            } else {
                val label = runCatching {
                    pm.getApplicationInfo(pkg, 0).let { pm.getApplicationLabel(it).toString() }
                }.getOrDefault(pkg)
                AppInfo(label, pkg)
            }
        }
    }
}
