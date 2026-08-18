package dev.jpcottin.netman.vpn

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import dev.jpcottin.netman.MainActivity
import dev.jpcottin.netman.R
import dev.jpcottin.netman.core.CoreBridge
import dev.jpcottin.netman.core.GraphRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.netman_android.SessionConfig
import uniffi.netman_android.captureStats
import uniffi.netman_android.start
import uniffi.netman_android.stop

/**
 * Service de capture : établit le tun, en cède le descripteur au cœur Rust,
 * et tourne en avant-plan pour survivre à la fermeture de l'IHM. La
 * contrepartie d'un VpnService — acheminer le trafic — est assurée par le
 * moteur de forwarding Rust ; ici on ne fait que la configuration réseau.
 */
class NetmanVpnService : VpnService() {

    private var tun: ParcelFileDescriptor? = null
    private var bridge: CoreBridge? = null
    private var running = false
    private val scope = CoroutineScope(Dispatchers.Default + Job())

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                stopCapture()
                stopSelf()
                return START_NOT_STICKY
            }
            else -> startCapture()
        }
        return START_STICKY
    }

    private fun startCapture() {
        if (running) return
        VpnController.update(VpnState.Starting)

        val dnsServers = underlyingDnsServers()
        val builder = Builder()
            .setSession("Netman")
            .setMtu(MTU)
            .addAddress(TUN_V4, 32)
            .addAddress(TUN_V6, 128)
            .addRoute("0.0.0.0", 0)
            .addRoute("::", 0)
            // L'application est exclue de son propre VPN : ses sockets ET sa
            // résolution DNS (netd) empruntent le réseau réel — pas de boucle.
            .apply {
                dnsServers.forEach { addDnsServer(it) }
                runCatching { addDisallowedApplication(packageName) }
            }

        val pfd = try {
            builder.establish()
        } catch (e: Exception) {
            VpnController.update(VpnState.Error("establish failed: ${e.message}"))
            stopSelf()
            return
        }
        if (pfd == null) {
            VpnController.update(VpnState.Error("VPN not prepared"))
            stopSelf()
            return
        }
        tun = pfd

        startForegroundCompat()

        val bridge = CoreBridge(this) { fd -> protect(fd) }
        this.bridge = bridge
        val config = SessionConfig(
            tunFd = pfd.detachFd(),
            fadeSecs = GraphRepository.fadeSecs.value.toULong(),
            mtu = MTU.toUInt(),
            tunAddrs = listOf(TUN_V4, TUN_V6),
            dnsServers = dnsServers,
        )
        try {
            start(config, bridge)
        } catch (e: Exception) {
            VpnController.update(VpnState.Error("core start failed: ${e.message}"))
            stopSelf()
            return
        }
        running = true
        VpnController.update(VpnState.Running())
        pollStats()
    }

    /** Rafraîchit le compteur affiché ~1×/s tant que la capture tourne. */
    private fun pollStats() {
        scope.launch {
            while (running) {
                val s = runCatching { captureStats() }.getOrNull()
                if (s != null) {
                    VpnController.update(
                        VpnState.Running(
                            frames = s.frames.toLong(),
                            bytes = s.bytes.toLong(),
                            chanDrops = s.chanDrops.toLong(),
                        ),
                    )
                }
                delay(1000)
            }
        }
    }

    private fun stopCapture() {
        if (!running) return
        running = false
        runCatching { stop() }
        runCatching { tun?.close() }
        tun = null
        bridge = null
    }

    override fun onRevoke() {
        // L'utilisateur a activé un autre VPN ou coupé le nôtre.
        stopCapture()
        VpnController.update(VpnState.Revoked)
        stopForegroundCompat()
        stopSelf()
    }

    override fun onDestroy() {
        stopCapture()
        scope.coroutineContext[Job]?.cancel()
        if (VpnController.state.value is VpnState.Running) {
            VpnController.update(VpnState.Idle)
        }
        super.onDestroy()
    }

    /** Résolveurs du réseau sous-jacent, à re-servir au Builder. */
    private fun underlyingDnsServers(): List<String> {
        val cm = getSystemService(ConnectivityManager::class.java) ?: return FALLBACK_DNS
        val active = cm.activeNetwork ?: return FALLBACK_DNS
        val props = cm.getLinkProperties(active) ?: return FALLBACK_DNS
        val servers = props.dnsServers.mapNotNull { it.hostAddress }
        return servers.ifEmpty { FALLBACK_DNS }
    }

    private fun startForegroundCompat() {
        val channelId = ensureChannel()
        val notification = buildNotification(channelId)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIF_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED,
            )
        } else {
            startForeground(NOTIF_ID, notification)
        }
    }

    private fun stopForegroundCompat() {
        stopForeground(STOP_FOREGROUND_REMOVE)
    }

    private fun ensureChannel(): String {
        val nm = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Capture",
            NotificationManager.IMPORTANCE_LOW,
        )
        nm.createNotificationChannel(channel)
        return CHANNEL_ID
    }

    private fun buildNotification(channelId: String): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val stopIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, NetmanVpnService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, channelId)
            .setContentTitle("Netman is capturing")
            .setContentText("Passive monitor — your traffic only")
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentIntent(open)
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null, "Stop", stopIntent).build())
            .build()
    }

    companion object {
        const val ACTION_START = "dev.jpcottin.netman.START"
        const val ACTION_STOP = "dev.jpcottin.netman.STOP"
        private const val CHANNEL_ID = "capture"
        private const val NOTIF_ID = 1
        private const val MTU = 1500
        private const val TUN_V4 = "10.111.222.1"
        private const val TUN_V6 = "fd00:6e6d::1"
        private val FALLBACK_DNS = listOf("1.1.1.1", "8.8.8.8")

        fun start(context: Context) {
            val intent = Intent(context, NetmanVpnService::class.java)
                .setAction(ACTION_START)
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            val intent = Intent(context, NetmanVpnService::class.java)
                .setAction(ACTION_STOP)
            context.startService(intent)
        }
    }
}
