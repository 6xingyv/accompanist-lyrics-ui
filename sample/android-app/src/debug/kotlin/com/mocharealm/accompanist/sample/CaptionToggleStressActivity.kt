package com.mocharealm.accompanist.sample

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity

/** Empty debug-only instrumentation host; never opens the player or starts audio playback. */
class CaptionToggleStressActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }
}
