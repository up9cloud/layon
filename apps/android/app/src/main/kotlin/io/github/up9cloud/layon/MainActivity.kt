package io.github.up9cloud.layon

import android.app.AlertDialog
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.WindowInsets
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import java.util.concurrent.Executors

class MainActivity : ComponentActivity() {
    private lateinit var cli: LayonCli
    // One at a time: concurrent CLI runs would race on the same client file.
    private val worker = Executors.newSingleThreadExecutor()

    private lateinit var status: TextView
    private lateinit var hostList: LinearLayout
    private val pairButtons = mutableListOf<Button>()
    private val hostButtons = mutableListOf<Button>()

    private val scan = registerForActivityResult(ScanContract()) { result ->
        result.contents?.let(::pair)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        cli = LayonCli(this)
        setContentView(layout())
        refresh()
        if (savedInstanceState == null) intent?.let(::handleLink)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
    }

    override fun onDestroy() {
        worker.shutdown()
        super.onDestroy()
    }

    private fun handleLink(intent: Intent) {
        val link = intent.dataString ?: return
        if (link.startsWith("layon://pair/")) pair(link)
    }

    private fun layout(): View {
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), dp(16), dp(16), dp(16))
        }
        status = TextView(this).apply { textSize = 15f }
        val pairRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(button("Scan QR to pair", pairButtons) {
                scan.launch(
                    ScanOptions()
                        .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                        .setPrompt("Scan the code from layon pair")
                        .setBeepEnabled(false)
                        .setOrientationLocked(false),
                )
            })
            addView(button("Paste code", pairButtons) { askForCode() })
        }
        hostList = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(pairRow)
        column.addView(status)
        column.addView(hostList)

        return ScrollView(this).apply {
            addView(column)
            // targetSdk 35+ draws under the system bars.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                setOnApplyWindowInsetsListener { view, insets ->
                    val bars = insets.getInsets(WindowInsets.Type.systemBars())
                    view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                    insets
                }
            }
        }
    }

    private fun showHosts(hosts: List<Host>) {
        hostList.removeAllViews()
        hostButtons.clear()
        if (hosts.isEmpty()) {
            hostList.addView(TextView(this).apply {
                text = "No paired machine. Run `layon pair` on it and scan the code."
                setPadding(0, dp(24), 0, 0)
            })
            return
        }
        for (host in hosts) {
            val state = TextView(this).apply { text = "checking…" }
            hostList.addView(TextView(this).apply {
                text = "${host.name}  (${host.os})"
                textSize = 20f
                setPadding(0, dp(24), 0, 0)
            })
            hostList.addView(state)
            hostList.addView(LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.START
                addView(button("Sleep", hostButtons) { act("Putting ${host.name} to sleep…") { cli.sleep(host.id) } })
                addView(button("Hibernate", hostButtons) { act("Hibernating ${host.name}…") { cli.hibernate(host.id) } })
                addView(button("Wake", hostButtons) { act("Waking ${host.name}…") { cli.wake(host.id) } })
                addView(button("Forget", hostButtons) { confirmForget(host) })
            })
            worker.execute {
                val awake = cli.isAwake(host.id)
                runOnUiThread { state.text = if (awake) "awake" else "not answering" }
            }
        }
    }

    private fun refresh() {
        worker.execute {
            val result = runCatching { cli.hosts() }
            runOnUiThread {
                result.onSuccess(::showHosts).onFailure { status.text = it.message }
            }
        }
    }

    private fun pair(ticket: String) {
        act("Pairing…", done = "Paired.") { cli.pair(ticket.trim(), Build.MODEL) }
    }

    private fun act(doing: String, done: String = "Done.", action: () -> Unit) {
        status.text = doing
        (pairButtons + hostButtons).forEach { it.isEnabled = false }
        worker.execute {
            val result = runCatching(action)
            runOnUiThread {
                (pairButtons + hostButtons).forEach { it.isEnabled = true }
                status.text = result.fold({ done }, { it.message })
                refresh()
            }
        }
    }

    private fun askForCode() {
        val input = EditText(this).apply { hint = "layon://pair/…" }
        AlertDialog.Builder(this)
            .setTitle("Pairing code")
            .setView(input)
            .setPositiveButton("Pair") { _, _ -> pair(input.text.toString()) }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun confirmForget(host: Host) {
        AlertDialog.Builder(this)
            .setMessage("Forget ${host.name}? Pairing again needs its QR code.")
            .setPositiveButton("Forget") { _, _ -> act("Forgetting ${host.name}…") { cli.forget(host.id) } }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun button(label: String, group: MutableList<Button>, onClick: () -> Unit) =
        Button(this).apply {
            text = label
            isAllCaps = false
            setOnClickListener { onClick() }
            group += this
        }

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
