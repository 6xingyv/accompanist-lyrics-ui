package com.mocharealm.accompanist.sample.desktop

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.TooltipArea
import androidx.compose.foundation.background
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.sample.desktop.resources.Res
import com.mocharealm.accompanist.sample.desktop.resources.ic_pin_fill
import com.mocharealm.accompanist.sample.desktop.resources.ic_pin_slash
import org.jetbrains.compose.resources.painterResource

@Composable
internal fun DesktopTitleBar(
    platform: DesktopPlatform,
    leftInset: Float,
    rightInset: Float,
    hovered: Boolean,
    pinned: Boolean,
    togglePinned: () -> Unit,
    modifier: Modifier = Modifier,
) {
    BoxWithConstraints(modifier.fillMaxWidth().height(CAPTION_HEIGHT.dp)) {
        val bounds = captionPinBounds(platform, maxWidth.value, leftInset, rightInset)
        val titleStart = if (platform == DesktopPlatform.Mac) bounds.right + 8f else 12f
        val titleEnd = if (platform == DesktopPlatform.Mac) 12f else maxWidth.value - bounds.left + 8f
        val titlePadding = if (platform == DesktopPlatform.Mac)
            PaddingValues(horizontal = titleStart.dp)
        else PaddingValues(start = titleStart.dp, end = titleEnd.dp)
        Text("Accompanist", color = Color.White.copy(alpha = .7f), fontSize = 12.sp,
            maxLines = 1, overflow = TextOverflow.Ellipsis,
            textAlign = if (platform == DesktopPlatform.Mac) TextAlign.Center else TextAlign.Start,
            modifier = Modifier.align(Alignment.CenterStart).fillMaxWidth()
                .padding(titlePadding))
        AnimatedVisibility(hovered, enter = fadeIn(), exit = fadeOut(),
            modifier = Modifier.offset(bounds.left.dp, bounds.top.dp)
                .size(bounds.width.dp, bounds.height.dp)) {
            CaptionPinButton(platform, pinned, togglePinned)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun CaptionPinButton(platform: DesktopPlatform, pinned: Boolean, toggle: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val pressed by interaction.collectIsPressedAsState()
    val action = if (pinned) "Unpin window" else "Keep window on top"
    val shortcut = if (platform == DesktopPlatform.Mac) "⌘P" else "Ctrl+P"
    val shape = if (platform == DesktopPlatform.Windows) RoundedCornerShape(0.dp)
        else RoundedCornerShape(6.dp)
    TooltipArea(
        tooltip = {
            Box(Modifier.background(Color(0xFF292929), RoundedCornerShape(4.dp)).padding(8.dp)) {
                Text("$action ($shortcut)", color = Color.White, fontSize = 11.sp)
            }
        },
        delayMillis = 700,
        modifier = Modifier.fillMaxSize(),
    ) {
        Box(Modifier.fillMaxSize().clip(shape)
            .background(Color.White.copy(alpha = when {
                pressed -> .06f
                hovered -> .10f
                else -> 0f
            }))
            .hoverable(interaction)
            .toggleable(value = pinned, interactionSource = interaction, indication = null,
                role = Role.Button, onValueChange = { toggle() }),
            contentAlignment = Alignment.Center) {
            Icon(painterResource(if (pinned) Res.drawable.ic_pin_fill else Res.drawable.ic_pin_slash),
                contentDescription = action, tint = Color.White.copy(alpha = if (pressed) .6f else .9f),
                modifier = Modifier.size(if (platform == DesktopPlatform.Mac) 16.dp else 18.dp))
        }
    }
}
