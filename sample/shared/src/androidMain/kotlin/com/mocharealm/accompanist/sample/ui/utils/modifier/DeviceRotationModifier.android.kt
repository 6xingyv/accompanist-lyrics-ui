package com.mocharealm.accompanist.sample.ui.utils.modifier

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext

actual fun Modifier.deviceRotation(
    rotationFactor: Float,
    cameraDistance: Float
): Modifier = composed {

    val context = LocalContext.current
    val sensorManager = remember {
        context.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    }
    val rotationSensor = remember {
        sensorManager.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
    }

    if (rotationSensor == null) {
        return@composed this
    }

    // Pitch and roll angles
    var pitch by remember { mutableFloatStateOf(0f) }
    var roll by remember { mutableFloatStateOf(0f) }

    DisposableEffect(sensorManager, rotationSensor) {
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent?) {
                if (event?.sensor?.type == Sensor.TYPE_ROTATION_VECTOR) {
                    // Get rotation matrix from rotation vector
                    val rotationMatrix = FloatArray(9)
                    SensorManager.getRotationMatrixFromVector(rotationMatrix, event.values)

                    // Get orientation angles from rotation matrix (azimuth, pitch, roll)
                    val orientationAngles = FloatArray(3)
                    SensorManager.getOrientation(rotationMatrix, orientationAngles)

                    // Convert radians to degrees and update state
                    // orientationAngles[1] is pitch angle, corresponding to X-axis rotation
                    // orientationAngles[2] is roll angle, corresponding to Y-axis rotation
                    pitch = Math.toDegrees(orientationAngles[1].toDouble()).toFloat()
                    roll = Math.toDegrees(orientationAngles[2].toDouble()).toFloat()
                }
            }

            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}
        }

        sensorManager.registerListener(
            listener,
            rotationSensor,
            SensorManager.SENSOR_DELAY_UI
        )

        onDispose {
            sensorManager.unregisterListener(listener)
        }
    }

    this.graphicsLayer {
        // Inverting pitch gives a more natural effect (when phone tilts down, UI top rotates away)
        rotationX = -pitch * rotationFactor
        rotationY = roll * rotationFactor

        // Set camera distance to enhance 3D parallax effect
        this.cameraDistance = cameraDistance * density
    }
}