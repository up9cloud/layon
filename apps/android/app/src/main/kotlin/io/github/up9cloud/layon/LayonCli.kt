package io.github.up9cloud.layon

import android.content.Context
import android.net.wifi.WifiManager
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

data class Host(val id: String, val name: String, val os: String)

class CliException(message: String) : Exception(message)

/** Every action is a run of the bundled `layon` CLI with `--json`. */
class LayonCli(context: Context) {
    private val exe = File(context.applicationInfo.nativeLibraryDir, "liblayon.so").path
    private val store = File(context.filesDir, "client.json").path
    private val stderr = File(context.cacheDir, "layon-stderr.txt")
    private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)

    fun hosts(): List<Host> {
        val array = JSONArray(run("hosts"))
        return (0 until array.length()).map { host(array.getJSONObject(it)) }
    }

    fun pair(ticket: String, deviceName: String): Host =
        host(JSONObject(run("pair", ticket, "--name", deviceName)))

    fun isAwake(id: String): Boolean = try {
        run("info", id)
        true
    } catch (e: CliException) {
        false
    }

    fun sleep(id: String) = run("sleep", id)
    fun hibernate(id: String) = run("hibernate", id)
    fun wake(id: String) = run("wake", id)
    fun forget(id: String) = run("forget", id)

    private fun host(o: JSONObject) = Host(o.getString("id"), o.getString("name"), o.getString("os"))

    private fun run(vararg args: String): String {
        val lock = wifi.createMulticastLock("layon").apply {
            setReferenceCounted(false)
            acquire()
        }
        try {
            val process = ProcessBuilder(listOf(exe, "--json", "--file", store) + args)
                .redirectError(stderr)
                .start()
            val out = process.inputStream.bufferedReader().readText()
            if (process.waitFor() != 0) throw CliException(errorMessage(stderr.readText()))
            return out
        } finally {
            lock.release()
        }
    }

    private fun errorMessage(stderr: String): String {
        val line = stderr.lines().lastOrNull { it.startsWith("{") }
            ?: return stderr.trim().ifEmpty { "layon exited with an error" }
        return JSONObject(line).optString("error", line)
    }
}
