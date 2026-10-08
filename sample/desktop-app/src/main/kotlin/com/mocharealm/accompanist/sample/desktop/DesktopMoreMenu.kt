package com.mocharealm.accompanist.sample.desktop

import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import java.awt.Desktop

@Composable
internal fun DesktopMoreMenu(
    expanded: Boolean,
    dismiss: () -> Unit,
    rescanMedia: () -> Unit,
    rescanLyrics: () -> Unit,
    reloadConfiguration: () -> Unit,
    showConfigurationLocation: () -> Unit,
) {
    DropdownMenu(expanded, dismiss) {
        listOf(
            "Rescan media sessions" to rescanMedia,
            "Rescan lyrics" to rescanLyrics,
            "Reload configuration" to reloadConfiguration,
            "Open configuration file location" to showConfigurationLocation,
        ).forEach { (label, action) ->
            DropdownMenuItem(text = { Text(label) }, onClick = { dismiss(); action() })
        }
    }
}

@Composable
internal fun DesktopActionErrors(controller: DesktopController) {
    val state by controller.state.collectAsState()
    state.actionError?.let { message ->
        AlertDialog(onDismissRequest = controller::dismissActionError,
            title = { Text("Unable to complete action") },
            text = { Text(message) },
            confirmButton = {
                TextButton(onClick = controller::dismissActionError) { Text("OK") }
            })
    }
}

internal fun DesktopController.showConfigurationLocation() {
    try {
        val desktop = Desktop.getDesktop()
        val file = config.configFile.toAbsolutePath().toFile()
        if (desktop.isSupported(Desktop.Action.BROWSE_FILE_DIR)) desktop.browseFileDirectory(file)
        else desktop.open(file.parentFile)
    } catch (failure: Exception) {
        reportError(failure.message ?: "Unable to open configuration file location")
    }
}
