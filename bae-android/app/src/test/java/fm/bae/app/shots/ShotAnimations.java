package fm.bae.app.shots;

import org.robolectric.shadows.ShadowValueAnimator;

/** Controls the animator scale inside the Robolectric process drawing a shot. */
final class ShotAnimations extends ShadowValueAnimator {
    static void disable() {
        setDurationScale(0f);
    }
}
