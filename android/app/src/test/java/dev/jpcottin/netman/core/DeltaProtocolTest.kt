package dev.jpcottin.netman.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Miroir du test de contrat `wsproto/mod.rs` : les formes JSON exactes émises
 * par le cœur Rust doivent se décoder ici sans perte.
 */
class DeltaProtocolTest {

    @Test
    fun decode_upsert_node_with_all_fields() {
        val json = """{"type":"upsert_node","view":"app","id":"app:10123","label":"Firefox",""" +
            """"bytes":2048,"bytes_in":1024,"bytes_out":1024,"packets":4,"proto":"HTTPS"}"""
        val msg = DeltaCodec.decode(json)
        assertTrue(msg is DeltaMessage.UpsertNode)
        msg as DeltaMessage.UpsertNode
        assertEquals("app", msg.view)
        assertEquals("app:10123", msg.id)
        assertEquals("Firefox", msg.label)
        assertEquals(2048, msg.bytes)
        assertEquals(1024, msg.bytesIn)
        assertEquals(1024, msg.bytesOut)
        assertEquals(4, msg.packets)
        assertEquals("HTTPS", msg.proto)
    }

    @Test
    fun decode_upsert_edge() {
        val json = """{"type":"upsert_edge","view":"inter","id":"10.0.0.1|10.0.0.2",""" +
            """"source":"10.0.0.1","target":"10.0.0.2","bytes":42,"packets":1,"proto":"DNS"}"""
        val msg = DeltaCodec.decode(json)
        assertTrue(msg is DeltaMessage.UpsertEdge)
        msg as DeltaMessage.UpsertEdge
        assertEquals("10.0.0.1", msg.source)
        assertEquals("10.0.0.2", msg.target)
        assertEquals("DNS", msg.proto)
    }

    @Test
    fun decode_remove_and_control_messages() {
        assertTrue(DeltaCodec.decode("""{"type":"remove_edge","view":"inter","id":"x|y"}""") is DeltaMessage.RemoveEdge)
        assertTrue(DeltaCodec.decode("""{"type":"remove_node","view":"app","id":"app:1"}""") is DeltaMessage.RemoveNode)
        assertTrue(DeltaCodec.decode("""{"type":"reset"}""") is DeltaMessage.Reset)
        val cfg = DeltaCodec.decode("""{"type":"config","fade_secs":60}""")
        assertEquals(60L, (cfg as DeltaMessage.Config).fadeSecs)
    }

    @Test
    fun unknown_type_and_garbage_decode_to_null() {
        // Le serveur peut émettre des messages que ce client ignore (ex.
        // "interfaces" du bureau) : décodage null, jamais d'exception.
        assertNull(DeltaCodec.decode("""{"type":"interfaces","current":null,"interfaces":[]}"""))
        assertNull(DeltaCodec.decode("not json at all"))
        assertNull(DeltaCodec.decode(""))
    }

    @Test
    fun unknown_extra_fields_are_ignored() {
        val json = """{"type":"remove_node","view":"app","id":"app:1","future_field":123}"""
        assertTrue(DeltaCodec.decode(json) is DeltaMessage.RemoveNode)
    }
}
