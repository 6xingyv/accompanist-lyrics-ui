package com.mocharealm.accompanist.sample.desktop

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.setValue
import com.jetbrains.JBR
import com.jetbrains.WindowDecorations
import java.awt.AWTEvent
import java.awt.Component
import java.awt.Toolkit
import java.awt.event.AWTEventListener
import java.awt.event.MouseEvent
import java.awt.event.WindowAdapter
import java.awt.event.WindowEvent
import javax.swing.JFrame
import javax.swing.SwingUtilities

/** JBR owns title-bar dragging, double-click, system controls, snap and border resizing. */
internal class JbrWindowDecoration(
    private val frame: JFrame,
    private val service: WindowDecorations,
    private val onHover: (Boolean) -> Unit,
) : AutoCloseable {
    private val titleBar = service.createCustomTitleBar().apply {
        height = CAPTION_HEIGHT
        putProperty("controls.dark", true)
    }
    var leftInset by mutableFloatStateOf(if (desktopPlatform() == DesktopPlatform.Mac) 80f else 0f)
        private set
    var rightInset by mutableFloatStateOf(if (desktopPlatform() == DesktopPlatform.Windows) 138f else 0f)
        private set
    private var hovered = false
    private val mouseEvents = AWTEventListener { event ->
        if (event !is MouseEvent || event.id == MouseEvent.MOUSE_WHEEL) return@AWTEventListener
        val component = event.source as? Component ?: return@AWTEventListener
        if (SwingUtilities.getWindowAncestor(component) !== frame && component !== frame)
            return@AWTEventListener
        val point = SwingUtilities.convertPoint(component, event.point, frame.contentPane)
        val inCaption = point.y >= 0 && point.y < CAPTION_HEIGHT && point.x >= 0 && point.x < frame.contentPane.width
        hover(inCaption)
        if (event.id != MouseEvent.MOUSE_EXITED) {
            // Compose's Skia layer has listeners over its entire surface. Explicitly
            // mark only the Pin button as client content inside the native caption.
            val bounds = captionPinBounds(desktopPlatform(), frame.contentPane.width.toFloat(),
                leftInset, rightInset)
            val inPin = bounds.contains(point.x.toFloat(), point.y.toFloat())
            titleBar.forceHitTest(!inCaption || inPin)
        }
    }
    private val focusEvents = object : WindowAdapter() {
        override fun windowLostFocus(event: WindowEvent) { hover(false) }
    }

    init {
        service.setCustomTitleBar(frame, titleBar)
        updateInsets()
        titleBar.putProperty("controls.visible", false)
        Toolkit.getDefaultToolkit().addAWTEventListener(mouseEvents,
            AWTEvent.MOUSE_EVENT_MASK or AWTEvent.MOUSE_MOTION_EVENT_MASK)
        frame.addWindowFocusListener(focusEvents)
    }
    private fun updateInsets() {
        if (titleBar.leftInset > 0) leftInset = titleBar.leftInset
        if (titleBar.rightInset > 0) rightInset = titleBar.rightInset
    }
    private fun hover(value: Boolean) {
        if (hovered == value) return
        hovered = value
        titleBar.putProperty("controls.visible", value)
        if (value) updateInsets()
        onHover(value)
    }
    override fun close() {
        Toolkit.getDefaultToolkit().removeAWTEventListener(mouseEvents)
        frame.removeWindowFocusListener(focusEvents)
        service.setCustomTitleBar(frame, null)
    }
    companion object {
        fun install(frame: JFrame, onHover: (Boolean) -> Unit): JbrWindowDecoration? {
            val service = JBR.getWindowDecorations()
            // JBR exposes CustomTitleBar on Windows/macOS. Linux retains the window
            // manager's native decorations, including dragging and resizing.
            check(service != null || desktopPlatform() == DesktopPlatform.Linux) {
                "Custom windows require JetBrains Runtime with the WindowDecorations API"
            }
            return service?.let { JbrWindowDecoration(frame, it, onHover) }
        }
    }
}
