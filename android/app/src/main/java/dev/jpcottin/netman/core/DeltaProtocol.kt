package dev.jpcottin.netman.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * Miroir Kotlin du contrat `netman::wsproto` (CLAUDE.md §6). Le cœur Rust
 * livre exactement ces messages en JSON via `on_deltas` ; le client APPLIQUE,
 * il ne recalcule pas. bytes/packets sont des cumuls absolus.
 *
 * Toute évolution du schéma touche backend, client web ET ce fichier dans le
 * même commit.
 */
@Serializable
sealed interface DeltaMessage {
    @Serializable
    @SerialName("upsert_node")
    data class UpsertNode(
        val view: String,
        val id: String,
        val label: String,
        val bytes: Long,
        @SerialName("bytes_in") val bytesIn: Long,
        @SerialName("bytes_out") val bytesOut: Long,
        val packets: Long,
        val proto: String,
    ) : DeltaMessage

    @Serializable
    @SerialName("upsert_edge")
    data class UpsertEdge(
        val view: String,
        val id: String,
        val source: String,
        val target: String,
        val bytes: Long,
        val packets: Long,
        val proto: String,
    ) : DeltaMessage

    @Serializable
    @SerialName("remove_node")
    data class RemoveNode(val view: String, val id: String) : DeltaMessage

    @Serializable
    @SerialName("remove_edge")
    data class RemoveEdge(val view: String, val id: String) : DeltaMessage

    @Serializable
    @SerialName("config")
    data class Config(@SerialName("fade_secs") val fadeSecs: Long) : DeltaMessage

    @Serializable
    @SerialName("reset")
    data object Reset : DeltaMessage
}

/** Les deux vues du port Android (Etherman n'existe pas sur un tun). */
object Views {
    const val APP = "app"
    const val INTER = "inter"
}

/** Décodeur unique, tolérant aux champs inconnus (évolutions futures). */
object DeltaCodec {
    private val json = Json {
        ignoreUnknownKeys = true
        classDiscriminator = "type"
    }

    fun decode(text: String): DeltaMessage? =
        runCatching { json.decodeFromString<DeltaMessage>(text) }.getOrNull()
}
