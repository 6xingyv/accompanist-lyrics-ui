package com.mocharealm.accompanist.sample.desktop

import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.ImageComposeScene
import androidx.compose.ui.semantics.*
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import java.io.File
import kotlin.test.*

class DesktopMoreMenuTest {
    @Test fun theMenuOffersOnlyTheFourDesktopActionsAndDismissesBeforeRunningThem() {
        var expanded by mutableStateOf(true)
        val called = mutableListOf<Int>()
        val scene = ImageComposeScene(420, 620) {
            AccompanistTheme(darkTheme = true) {
                val action = { index: Int ->
                    assertFalse(expanded, "Close the menu before opening native windows or refreshing")
                    called += index
                }
                DesktopMoreMenu(expanded, { expanded = false },
                    { action(0) }, { action(1) }, { action(2) }, { action(3) })
            }
        }
        var nanos = 0L
        fun settle() {
            repeat(20) {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667
                scene.render(nanos).close()
            }
        }
        fun descendants(node: SemanticsNode): List<SemanticsNode> =
            listOf(node) + node.children.flatMap(::descendants)
        fun actions(): List<SemanticsNode> = scene.semanticsOwners
            .flatMap { descendants(it.rootSemanticsNode) }
            .filter { SemanticsActions.OnClick in it.config }
        try {
            settle()
            val labels = actions().map { node ->
                node.config[SemanticsProperties.Text].joinToString { it.text }
            }
            assertEquals(listOf("Rescan media sessions", "Rescan lyrics", "Reload configuration",
                "Open configuration file location"), labels)
            scene.render(nanos).use { image ->
                File("build/menu-preview/desktop.png").also { file ->
                    file.parentFile.mkdirs()
                    image.encodeToData()!!.use { file.writeBytes(it.bytes) }
                }
            }
            repeat(4) { index ->
                expanded = true
                settle()
                assertTrue(assertNotNull(actions()[index].config[SemanticsActions.OnClick].action).invoke())
                assertEquals(index, called.last())
            }
        } finally { scene.close() }
    }
}
