use bevy_ecs::{reflect::ReflectResource, system::Resource};
use bevy_reflect::{FromReflect, Reflect};
use bevy_utils::{Duration, Instant};

use crate::clock::Clock;

/// A clock that tracks how much it has advanced (and how much real time has elapsed) since
/// its previous update and since its creation.
#[derive(Resource, Reflect, FromReflect, Debug, Clone)]
#[reflect(Resource)]
pub struct Time {
    startup: Instant,
    first_update: Option<Instant>,
    last_update: Option<Instant>,
    // pausing
    paused: bool,
    // scaling
    relative_speed: f64, // using `f64` instead of `f32` to minimize drift from rounding errors
    // wrapping
    wrap_seconds: u64,

    maximum_delta: Option<Duration>,
    fixed_accumulated: Duration,
    fixed_period: Duration,

    raw_clock: Clock,
    virtual_clock: Clock,
    fixed_clock: Clock,
    current_clock: Clock,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            startup: Instant::now(),
            first_update: None,
            last_update: None,
            paused: false,
            relative_speed: 1.0,
            wrap_seconds: 3600, // 1 hour
            maximum_delta: Some(Duration::from_millis(333)),
            fixed_accumulated: Duration::ZERO,
            fixed_period: Duration::from_secs_f32(1. / 60.), // XXX
            raw_clock: Clock::new(3600),
            virtual_clock: Clock::new(3600),
            fixed_clock: Clock::new(3600),
            current_clock: Clock::new(3600),
        }
    }
}

impl Time {
    /// Constructs a new `Time` instance with a specific startup `Instant`.
    pub fn new(startup: Instant) -> Self {
        Self {
            startup,
            ..Default::default()
        }
    }

    /// Updates the internal time measurements.
    ///
    /// Calling this method as part of your app will most likely result in inaccurate timekeeping,
    /// as the `Time` resource is ordinarily managed by the [`TimePlugin`](crate::TimePlugin).
    pub fn update(&mut self) {
        let now = Instant::now();
        self.update_with_instant(now);
    }

    /// Updates time with a specified [`Instant`].
    ///
    /// This method is provided for use in tests. Calling this method as part of your app will most
    /// likely result in inaccurate timekeeping, as the `Time` resource is ordinarily managed by the
    /// [`TimePlugin`](crate::TimePlugin).
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_time::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// # use bevy_utils::Duration;
    /// # fn main () {
    /// #     test_health_system();
    /// # }
    /// #[derive(Resource)]
    /// struct Health {
    ///     // Health value between 0.0 and 1.0
    ///     health_value: f32,
    /// }
    ///
    /// fn health_system(time: Res<Time>, mut health: ResMut<Health>) {
    ///     // Increase health value by 0.1 per second, independent of frame rate,
    ///     // but not beyond 1.0
    ///     health.health_value = (health.health_value + 0.1 * time.delta_seconds()).min(1.0);
    /// }
    ///
    /// // Mock time in tests
    /// fn test_health_system() {
    ///     let mut world = World::default();
    ///     let mut time = Time::default();
    ///     time.update();
    ///     world.insert_resource(time);
    ///     world.insert_resource(Health { health_value: 0.2 });
    ///
    ///     let mut schedule = Schedule::new();
    ///     schedule.add_systems(health_system);
    ///
    ///     // Simulate that 30 ms have passed
    ///     let mut time = world.resource_mut::<Time>();
    ///     let last_update = time.last_update().unwrap();
    ///     time.update_with_instant(last_update + Duration::from_millis(30));
    ///
    ///     // Run system
    ///     schedule.run(&mut world);
    ///
    ///     // Check that 0.003 has been added to the health value
    ///     let expected_health_value = 0.2 + 0.1 * 0.03;
    ///     let actual_health_value = world.resource::<Health>().health_value;
    ///     assert_eq!(expected_health_value, actual_health_value);
    /// }
    /// ```
    pub fn update_with_instant(&mut self, instant: Instant) {
        let raw_delta = instant - self.last_update.unwrap_or(self.startup);
        self.raw_clock.advance_by(raw_delta);
        let scaled_delta = if self.paused {
            Duration::ZERO
        } else if self.relative_speed != 1.0 {
            raw_delta.mul_f64(self.relative_speed)
        } else {
            // avoid rounding when at normal speed
            raw_delta
        };
        let delta = if let Some(maximum_delta) = self.maximum_delta {
            std::cmp::min(scaled_delta, maximum_delta)
        } else {
            scaled_delta
        };
        self.virtual_clock.advance_by(delta);
        self.fixed_accumulated += delta;

        if self.last_update.is_none() {
            self.first_update = Some(instant);
            // on first actual update, zero out delta so we do not get a big jump due to startup systems
            self.raw_clock.advance_by(Duration::ZERO);
            self.virtual_clock.advance_by(Duration::ZERO);
        }
        self.last_update = Some(instant);
        self.current_clock = self.virtual_clock;
    }

    pub fn expend_fixed(&mut self) -> bool {
        if let Some(new_value) = self.fixed_accumulated.checked_sub(self.fixed_period) {
            // reduce accumulated and increase elapsed by period
            self.fixed_accumulated = new_value;
            self.fixed_clock.advance_by(self.fixed_period);
            self.current_clock = self.fixed_clock;
            true
        } else {
            // no more periods left in accumulated
            self.current_clock = self.virtual_clock;
            false
        }
    }

    /// Returns the [`Instant`] the clock was created.
    ///
    /// This usually represents when the app was started.
    #[inline]
    pub fn startup(&self) -> Instant {
        self.startup
    }

    /// Returns the [`Instant`] when [`update`](#method.update) was first called, if it exists.
    ///
    /// This usually represents when the first app update started.
    #[inline]
    pub fn first_update(&self) -> Option<Instant> {
        self.first_update
    }

    /// Returns the [`Instant`] when [`update`](#method.update) was last called, if it exists.
    ///
    /// This usually represents when the current app update started.
    #[inline]
    pub fn last_update(&self) -> Option<Instant> {
        self.last_update
    }

    /// Returns how much time has advanced since the last [`update`](#method.update), as a [`Duration`].
    #[inline]
    pub fn delta(&self) -> Duration {
        self.current_clock.delta
    }

    /// Returns how much time has advanced since the last [`update`](#method.update), as [`f32`] seconds.
    #[inline]
    pub fn delta_seconds(&self) -> f32 {
        self.current_clock.delta_seconds
    }

    /// Returns how much time has advanced since the last [`update`](#method.update), as [`f64`] seconds.
    #[inline]
    pub fn delta_seconds_f64(&self) -> f64 {
        self.current_clock.delta_seconds_f64
    }

    /// Returns how much time has advanced since [`startup`](#method.startup), as [`Duration`].
    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.current_clock.elapsed
    }

    /// Returns how much time has advanced since [`startup`](#method.startup), as [`f32`] seconds.
    ///
    /// **Note:** This is a monotonically increasing value. It's precision will degrade over time.
    /// If you need an `f32` but that precision loss is unacceptable,
    /// use [`elapsed_seconds_wrapped`](#method.elapsed_seconds_wrapped).
    #[inline]
    pub fn elapsed_seconds(&self) -> f32 {
        self.current_clock.elapsed_seconds
    }

    /// Returns how much time has advanced since [`startup`](#method.startup), as [`f64`] seconds.
    #[inline]
    pub fn elapsed_seconds_f64(&self) -> f64 {
        self.current_clock.elapsed_seconds_f64
    }

    /// Returns how much time has advanced since [`startup`](#method.startup) modulo
    /// the [`wrap_period`](#method.wrap_period), as [`Duration`].
    #[inline]
    pub fn elapsed_wrapped(&self) -> Duration {
        self.current_clock.elapsed_wrapped
    }

    /// Returns how much time has advanced since [`startup`](#method.startup) modulo
    /// the [`wrap_period`](#method.wrap_period), as [`f32`] seconds.
    ///
    /// This method is intended for applications (e.g. shaders) that require an [`f32`] value but
    /// suffer from the gradual precision loss of [`elapsed_seconds`](#method.elapsed_seconds).
    #[inline]
    pub fn elapsed_seconds_wrapped(&self) -> f32 {
        self.current_clock.elapsed_seconds_wrapped
    }

    /// Returns how much time has advanced since [`startup`](#method.startup) modulo
    /// the [`wrap_period`](#method.wrap_period), as [`f64`] seconds.
    #[inline]
    pub fn elapsed_seconds_wrapped_f64(&self) -> f64 {
        self.current_clock.elapsed_seconds_wrapped_f64
    }

    /// Returns how much real time has elapsed since the last [`update`](#method.update), as a [`Duration`].
    #[inline]
    pub fn raw_delta(&self) -> Duration {
        self.raw_clock.delta
    }

    /// Returns how much real time has elapsed since the last [`update`](#method.update), as [`f32`] seconds.
    #[inline]
    pub fn raw_delta_seconds(&self) -> f32 {
        self.raw_clock.delta_seconds
    }

    /// Returns how much real time has elapsed since the last [`update`](#method.update), as [`f64`] seconds.
    #[inline]
    pub fn raw_delta_seconds_f64(&self) -> f64 {
        self.raw_clock.delta_seconds_f64
    }

    /// Returns how much real time has elapsed since [`startup`](#method.startup), as [`Duration`].
    #[inline]
    pub fn raw_elapsed(&self) -> Duration {
        self.raw_clock.elapsed
    }

    /// Returns how much real time has elapsed since [`startup`](#method.startup), as [`f32`] seconds.
    ///
    /// **Note:** This is a monotonically increasing value. It's precision will degrade over time.
    /// If you need an `f32` but that precision loss is unacceptable,
    /// use [`raw_elapsed_seconds_wrapped`](#method.raw_elapsed_seconds_wrapped).
    #[inline]
    pub fn raw_elapsed_seconds(&self) -> f32 {
        self.raw_clock.elapsed_seconds
    }

    /// Returns how much real time has elapsed since [`startup`](#method.startup), as [`f64`] seconds.
    #[inline]
    pub fn raw_elapsed_seconds_f64(&self) -> f64 {
        self.raw_clock.elapsed_seconds_f64
    }

    /// Returns how much real time has elapsed since [`startup`](#method.startup) modulo
    /// the [`wrap_period`](#method.wrap_period), as [`Duration`].
    #[inline]
    pub fn raw_elapsed_wrapped(&self) -> Duration {
        self.raw_clock.elapsed_wrapped
    }

    /// Returns how much real time has elapsed since [`startup`](#method.startup) modulo
    /// the [`wrap_period`](#method.wrap_period), as [`f32`] seconds.
    ///
    /// This method is intended for applications (e.g. shaders) that require an [`f32`] value but
    /// suffer from the gradual precision loss of [`raw_elapsed_seconds`](#method.raw_elapsed_seconds).
    #[inline]
    pub fn raw_elapsed_seconds_wrapped(&self) -> f32 {
        self.raw_clock.elapsed_seconds_wrapped
    }

    /// Returns how much real time has elapsed since [`startup`](#method.startup) modulo
    /// the [`wrap_period`](#method.wrap_period), as [`f64`] seconds.
    #[inline]
    pub fn raw_elapsed_seconds_wrapped_f64(&self) -> f64 {
        self.raw_clock.elapsed_seconds_wrapped_f64
    }

    /// Returns the modulus used to calculate [`elapsed_wrapped`](#method.elapsed_wrapped) and
    /// [`raw_elapsed_wrapped`](#method.raw_elapsed_wrapped).
    ///
    /// **Note:** The default modulus is one hour.
    #[inline]
    pub fn wrap_period(&self) -> Duration {
        Duration::from_secs(self.wrap_seconds)
    }

    /// Sets the modulus used to calculate [`elapsed_wrapped`](#method.elapsed_wrapped) and
    /// [`raw_elapsed_wrapped`](#method.raw_elapsed_wrapped).
    ///
    /// **Note:** This will not take effect until the next update.
    ///
    /// # Panics
    ///
    /// Panics if `wrap_period` is a zero-length duration.
    #[inline]
    pub fn set_wrap_period(&mut self, wrap_period: Duration) {
        assert!(!wrap_period.is_zero(), "division by zero");
        assert_eq!(wrap_period.subsec_nanos(), 0, "wrap period must be integral seconds");
        self.wrap_seconds = wrap_period.as_secs();
        self.raw_clock.wrap_seconds = self.wrap_seconds;
        self.virtual_clock.wrap_seconds = self.wrap_seconds;
        self.fixed_clock.wrap_seconds = self.wrap_seconds;
    }

    /// Returns the speed the clock advances relative to your system clock, as [`f32`].
    /// This is known as "time scaling" or "time dilation" in other engines.
    ///
    /// **Note:** This function will return zero when time is paused.
    #[inline]
    pub fn relative_speed(&self) -> f32 {
        self.relative_speed_f64() as f32
    }

    /// Returns the speed the clock advances relative to your system clock, as [`f64`].
    /// This is known as "time scaling" or "time dilation" in other engines.
    ///
    /// **Note:** This function will return zero when time is paused.
    #[inline]
    pub fn relative_speed_f64(&self) -> f64 {
        if self.paused {
            0.0
        } else {
            self.relative_speed
        }
    }

    /// Sets the speed the clock advances relative to your system clock, given as an [`f32`].
    ///
    /// For example, setting this to `2.0` will make the clock advance twice as fast as your system clock.
    ///
    /// **Note:** This does not affect the `raw_*` measurements.
    ///
    /// # Panics
    ///
    /// Panics if `ratio` is negative or not finite.
    #[inline]
    pub fn set_relative_speed(&mut self, ratio: f32) {
        self.set_relative_speed_f64(ratio as f64);
    }

    /// Sets the speed the clock advances relative to your system clock, given as an [`f64`].
    ///
    /// For example, setting this to `2.0` will make the clock advance twice as fast as your system clock.
    ///
    /// **Note:** This does not affect the `raw_*` measurements.
    ///
    /// # Panics
    ///
    /// Panics if `ratio` is negative or not finite.
    #[inline]
    pub fn set_relative_speed_f64(&mut self, ratio: f64) {
        assert!(ratio.is_finite(), "tried to go infinitely fast");
        assert!(ratio >= 0.0, "tried to go back in time");
        self.relative_speed = ratio;
    }

    /// Stops the clock, preventing it from advancing until resumed.
    ///
    /// **Note:** This does not affect the `raw_*` measurements.
    #[inline]
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resumes the clock if paused.
    #[inline]
    pub fn unpause(&mut self) {
        self.paused = false;
    }

    /// Returns `true` if the clock is currently paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn raw_clock(&self) -> &Clock {
        &self.raw_clock
    }

    pub fn virtual_clock(&self) -> &Clock {
        &self.virtual_clock
    }

    pub fn fixed_clock(&self) -> &Clock {
        &self.fixed_clock
    }

    pub fn current_clock(&self) -> &Clock {
        &self.current_clock
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::Time;
    use bevy_utils::{Duration, Instant};

    fn assert_float_eq(a: f32, b: f32) {
        assert!((a - b).abs() <= f32::EPSILON, "{a} != {b}");
    }

    #[test]
    fn update_test() {
        let start_instant = Instant::now();
        let mut time = Time::new(start_instant);

        // Ensure `time` was constructed correctly.
        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), None);
        assert_eq!(time.last_update(), None);
        assert_eq!(time.relative_speed(), 1.0);
        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.delta_seconds(), 0.0);
        assert_eq!(time.delta_seconds_f64(), 0.0);
        assert_eq!(time.raw_delta(), Duration::ZERO);
        assert_eq!(time.raw_delta_seconds(), 0.0);
        assert_eq!(time.raw_delta_seconds_f64(), 0.0);
        assert_eq!(time.elapsed(), Duration::ZERO);
        assert_eq!(time.elapsed_seconds(), 0.0);
        assert_eq!(time.elapsed_seconds_f64(), 0.0);
        assert_eq!(time.raw_elapsed(), Duration::ZERO);
        assert_eq!(time.raw_elapsed_seconds(), 0.0);
        assert_eq!(time.raw_elapsed_seconds_f64(), 0.0);

        // Update `time` and check results.
        // The first update to `time` normally happens before other systems have run,
        // so the first delta doesn't appear until the second update.
        let first_update_instant = Instant::now();
        time.update_with_instant(first_update_instant);

        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), Some(first_update_instant));
        assert_eq!(time.last_update(), Some(first_update_instant));
        assert_eq!(time.relative_speed(), 1.0);
        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.delta_seconds(), 0.0);
        assert_eq!(time.delta_seconds_f64(), 0.0);
        assert_eq!(time.raw_delta(), Duration::ZERO);
        assert_eq!(time.raw_delta_seconds(), 0.0);
        assert_eq!(time.raw_delta_seconds_f64(), 0.0);
        assert_eq!(time.elapsed(), first_update_instant - start_instant,);
        assert_eq!(
            time.elapsed_seconds(),
            (first_update_instant - start_instant).as_secs_f32(),
        );
        assert_eq!(
            time.elapsed_seconds_f64(),
            (first_update_instant - start_instant).as_secs_f64(),
        );
        assert_eq!(time.raw_elapsed(), first_update_instant - start_instant,);
        assert_eq!(
            time.raw_elapsed_seconds(),
            (first_update_instant - start_instant).as_secs_f32(),
        );
        assert_eq!(
            time.raw_elapsed_seconds_f64(),
            (first_update_instant - start_instant).as_secs_f64(),
        );

        // Update `time` again and check results.
        // At this point its safe to use time.delta().
        let second_update_instant = Instant::now();
        time.update_with_instant(second_update_instant);
        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), Some(first_update_instant));
        assert_eq!(time.last_update(), Some(second_update_instant));
        assert_eq!(time.relative_speed(), 1.0);
        assert_eq!(time.delta(), second_update_instant - first_update_instant);
        assert_eq!(
            time.delta_seconds(),
            (second_update_instant - first_update_instant).as_secs_f32(),
        );
        assert_eq!(
            time.delta_seconds_f64(),
            (second_update_instant - first_update_instant).as_secs_f64(),
        );
        assert_eq!(
            time.raw_delta(),
            second_update_instant - first_update_instant,
        );
        assert_eq!(
            time.raw_delta_seconds(),
            (second_update_instant - first_update_instant).as_secs_f32(),
        );
        assert_eq!(
            time.raw_delta_seconds_f64(),
            (second_update_instant - first_update_instant).as_secs_f64(),
        );
        assert_eq!(time.elapsed(), second_update_instant - start_instant,);
        assert_eq!(
            time.elapsed_seconds(),
            (second_update_instant - start_instant).as_secs_f32(),
        );
        assert_eq!(
            time.elapsed_seconds_f64(),
            (second_update_instant - start_instant).as_secs_f64(),
        );
        assert_eq!(time.raw_elapsed(), second_update_instant - start_instant,);
        assert_eq!(
            time.raw_elapsed_seconds(),
            (second_update_instant - start_instant).as_secs_f32(),
        );
        assert_eq!(
            time.raw_elapsed_seconds_f64(),
            (second_update_instant - start_instant).as_secs_f64(),
        );
    }

    #[test]
    fn wrapping_test() {
        let start_instant = Instant::now();

        let mut time = Time {
            startup: start_instant,
            wrap_seconds: 3,
            ..Default::default()
        };

        assert_eq!(time.elapsed_seconds_wrapped(), 0.0);

        time.update_with_instant(start_instant + Duration::from_secs(1));
        assert_float_eq(time.elapsed_seconds_wrapped(), 1.0);

        time.update_with_instant(start_instant + Duration::from_secs(2));
        assert_float_eq(time.elapsed_seconds_wrapped(), 2.0);

        time.update_with_instant(start_instant + Duration::from_secs(3));
        assert_float_eq(time.elapsed_seconds_wrapped(), 0.0);

        time.update_with_instant(start_instant + Duration::from_secs(4));
        assert_float_eq(time.elapsed_seconds_wrapped(), 1.0);
    }

    #[test]
    fn relative_speed_test() {
        let start_instant = Instant::now();
        let mut time = Time::new(start_instant);

        let first_update_instant = Instant::now();
        time.update_with_instant(first_update_instant);

        // Update `time` again and check results.
        // At this point its safe to use time.delta().
        let second_update_instant = Instant::now();
        time.update_with_instant(second_update_instant);
        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), Some(first_update_instant));
        assert_eq!(time.last_update(), Some(second_update_instant));
        assert_eq!(time.relative_speed(), 1.0);
        assert_eq!(time.delta(), second_update_instant - first_update_instant);
        assert_eq!(
            time.delta_seconds(),
            (second_update_instant - first_update_instant).as_secs_f32(),
        );
        assert_eq!(
            time.delta_seconds_f64(),
            (second_update_instant - first_update_instant).as_secs_f64(),
        );
        assert_eq!(
            time.raw_delta(),
            second_update_instant - first_update_instant,
        );
        assert_eq!(
            time.raw_delta_seconds(),
            (second_update_instant - first_update_instant).as_secs_f32(),
        );
        assert_eq!(
            time.raw_delta_seconds_f64(),
            (second_update_instant - first_update_instant).as_secs_f64(),
        );
        assert_eq!(time.elapsed(), second_update_instant - start_instant,);
        assert_eq!(
            time.elapsed_seconds(),
            (second_update_instant - start_instant).as_secs_f32(),
        );
        assert_eq!(
            time.elapsed_seconds_f64(),
            (second_update_instant - start_instant).as_secs_f64(),
        );
        assert_eq!(time.raw_elapsed(), second_update_instant - start_instant,);
        assert_eq!(
            time.raw_elapsed_seconds(),
            (second_update_instant - start_instant).as_secs_f32(),
        );
        assert_eq!(
            time.raw_elapsed_seconds_f64(),
            (second_update_instant - start_instant).as_secs_f64(),
        );

        // Make app time advance at 2x the rate of your system clock.
        time.set_relative_speed(2.0);

        // Update `time` again 1 second later.
        let elapsed = Duration::from_secs(1);
        let third_update_instant = second_update_instant + elapsed;
        time.update_with_instant(third_update_instant);

        // Since app is advancing 2x your system clock, expect time
        // to have advanced by twice the amount of real time elapsed.
        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), Some(first_update_instant));
        assert_eq!(time.last_update(), Some(third_update_instant));
        assert_eq!(time.relative_speed(), 2.0);
        assert_eq!(time.delta(), elapsed.mul_f32(2.0));
        assert_eq!(time.delta_seconds(), elapsed.mul_f32(2.0).as_secs_f32());
        assert_eq!(time.delta_seconds_f64(), elapsed.mul_f32(2.0).as_secs_f64());
        assert_eq!(time.raw_delta(), elapsed);
        assert_eq!(time.raw_delta_seconds(), elapsed.as_secs_f32());
        assert_eq!(time.raw_delta_seconds_f64(), elapsed.as_secs_f64());
        assert_eq!(
            time.elapsed(),
            second_update_instant - start_instant + elapsed.mul_f32(2.0),
        );
        assert_eq!(
            time.elapsed_seconds(),
            (second_update_instant - start_instant + elapsed.mul_f32(2.0)).as_secs_f32(),
        );
        assert_eq!(
            time.elapsed_seconds_f64(),
            (second_update_instant - start_instant + elapsed.mul_f32(2.0)).as_secs_f64(),
        );
        assert_eq!(
            time.raw_elapsed(),
            second_update_instant - start_instant + elapsed,
        );
        assert_eq!(
            time.raw_elapsed_seconds(),
            (second_update_instant - start_instant + elapsed).as_secs_f32(),
        );
        assert_eq!(
            time.raw_elapsed_seconds_f64(),
            (second_update_instant - start_instant + elapsed).as_secs_f64(),
        );
    }

    #[test]
    fn pause_test() {
        let start_instant = Instant::now();
        let mut time = Time::new(start_instant);

        let first_update_instant = Instant::now();
        time.update_with_instant(first_update_instant);

        assert!(!time.is_paused());
        assert_eq!(time.relative_speed(), 1.0);

        time.pause();

        assert!(time.is_paused());
        assert_eq!(time.relative_speed(), 0.0);

        let second_update_instant = Instant::now();
        time.update_with_instant(second_update_instant);
        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), Some(first_update_instant));
        assert_eq!(time.last_update(), Some(second_update_instant));
        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(
            time.raw_delta(),
            second_update_instant - first_update_instant,
        );
        assert_eq!(time.elapsed(), first_update_instant - start_instant);
        assert_eq!(time.raw_elapsed(), second_update_instant - start_instant);

        time.unpause();

        assert!(!time.is_paused());
        assert_eq!(time.relative_speed(), 1.0);

        let third_update_instant = Instant::now();
        time.update_with_instant(third_update_instant);
        assert_eq!(time.startup(), start_instant);
        assert_eq!(time.first_update(), Some(first_update_instant));
        assert_eq!(time.last_update(), Some(third_update_instant));
        assert_eq!(time.delta(), third_update_instant - second_update_instant);
        assert_eq!(
            time.raw_delta(),
            third_update_instant - second_update_instant,
        );
        assert_eq!(
            time.elapsed(),
            (third_update_instant - second_update_instant) + (first_update_instant - start_instant),
        );
        assert_eq!(time.raw_elapsed(), third_update_instant - start_instant);
    }
}
