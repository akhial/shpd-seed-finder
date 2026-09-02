// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

/**
 * The host's own parallelism, published by [JniBindings.availableWorkers]: the
 * ceiling of the worker selector and the count a device that never touched the
 * selector searches with.
 *
 * It is read once, lazily, from the native library every APK packages — the
 * same library JVM unit tests load through `buildHostJni` — exactly like
 * [EngineInfo]. The engine already clamps whatever a search is started with,
 * so this is the app's display ceiling, not a second policy: the value is only
 * ever coerced upwards to one, which the engine also guarantees.
 */
object SearchWorkers {
    val ceiling: Int by lazy { JniBindings.availableWorkers().coerceAtLeast(1) }
}
