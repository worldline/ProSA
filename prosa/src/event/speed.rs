use core::fmt;
use std::{
    cmp::Ordering,
    collections::VecDeque,
    thread::sleep,
    time::{Duration, Instant},
};

use tokio::sync::Notify;

/// Structure to define a transaction flow speed
///
/// ```
/// use std::time::{Instant, Duration};
/// use prosa::event::speed::Speed;
/// use std::thread::sleep;
///
/// const TPS: f64 = 25.0;
/// let mut speed = Speed::new(5);
///
/// // Send transactions
/// speed.time();
/// speed.time();
/// sleep(Duration::from_millis(40));
/// speed.time();
/// sleep(Duration::from_millis(40));
/// speed.time();
/// sleep(Duration::from_millis(40));
/// speed.time();
/// sleep(Duration::from_millis(40));
///
/// // Replace the first transaction time (size > 5)
/// speed.time();
/// let mut duration = speed.get_duration(TPS);     // 40 miliseconds for the duration to keep the same TPS rate
/// assert!(duration.as_millis() >= 36 && duration.as_millis() <= 44);
/// sleep(Duration::from_millis(40));
///
/// let mean_duration = speed.get_mean_duration();  // 40 miliseconds between each transactions
/// assert!(mean_duration.as_millis() >= 36 && mean_duration.as_millis() <= 44);
/// let speed_tps = speed.get_speed().round();      // 25 TPS
/// assert_eq!(TPS, speed_tps);
/// duration = speed.get_duration(TPS);             // 0 miliseconds for the duration to keep the same TPS rate
/// assert!(duration.as_millis() <= 4);
/// ```
#[derive(Debug, Clone)]
pub struct Speed {
    event_speeds: VecDeque<Instant>,
}

impl Speed {
    /// Create a new speed from number of desire sample (5 minimum)
    pub fn new(size_for_average: u16) -> Speed {
        let size = if size_for_average > 5 {
            size_for_average as usize
        } else {
            5usize
        };

        Speed {
            event_speeds: VecDeque::with_capacity(size),
        }
    }

    /// Add an event instant
    pub fn time_event(&mut self, instant: Instant) {
        if self.event_speeds.len() == self.event_speeds.capacity() {
            self.event_speeds.pop_back();
        }

        self.event_speeds.push_front(instant);
    }

    /// Add an event at the current instant time
    pub fn time(&mut self) {
        self.time_event(Instant::now())
    }

    /// Get the last event instant value or None if empty
    pub fn get_last_event(&self) -> Option<&Instant> {
        self.event_speeds.front()
    }

    /// Getter of the mean time between transaction
    ///
    /// <math><mfrac><mi><msub><mi>Σ</mi><mn>t</mn></msub></mi><mi><msub><mi>N</mi><mn>t</mn></msub></mi></mfrac> = mean</math>
    pub fn get_mean_duration(&self) -> Duration {
        let mut mean_duration = Duration::default();
        let mut instant = Instant::now();
        for event_speed in &self.event_speeds {
            mean_duration += instant.duration_since(*event_speed);
            instant = *event_speed;
        }

        mean_duration.div_f32(self.event_speeds.capacity() as f32)
    }

    /// Getter of the current speed of transaction flow
    ///
    /// <math><mfrac><mi>1000 × <msub><mi>N</mi><mn>t</mn></msub></mi><mi><msub><mi>Σ</mi><mn>t</mn></msub></mi></mfrac> = TPS</math>
    pub fn get_speed(&self) -> f64 {
        let mut sum_duration = Duration::default();
        let mut instant = Instant::now();
        for event_speed in &self.event_speeds {
            sum_duration += instant.duration_since(*event_speed);
            instant = *event_speed;
        }

        if !sum_duration.is_zero() {
            (1000 * self.event_speeds.capacity()) as f64 / sum_duration.as_millis() as f64
        } else {
            0.0
        }
    }

    /// Getter of the duration time it must wait since the last event to target the given TPS (Transaction Per Seconds) rate
    /// Consider an overhead to get a lasy duration to not overwhelmed a distant
    /// TPS should be superior to 0 otherwise it'll panic
    /// Duration equal 0 if the result is negative
    ///
    /// <math><mfrac><mi>1000 × <msub><mi>N</mi><mn>t</mn></msub></mi><mi>TPS</mi></mfrac> + overhead − <msub><mi>Σ</mi><mn>t</mn></msub> = duration</math>
    pub fn get_duration_overhead(&self, tps: f64, overhead: Option<Duration>) -> Duration {
        let mut sum_duration = Duration::default();
        let mut instant = Instant::now();
        for event_speed in &self.event_speeds {
            sum_duration += instant.duration_since(*event_speed);
            instant = *event_speed;
        }

        let duration =
            Duration::from_millis(((1000 * self.event_speeds.len()) as f64 / tps) as u64);
        if let Some(overhead) = overhead {
            duration
                .saturating_add(overhead)
                .saturating_sub(sum_duration)
        } else {
            duration.saturating_sub(sum_duration)
        }
    }

    /// Getter of the duration time it must wait since the last event to target the given TPS (Transaction Per Seconds) rate
    /// TPS should be superior to 0 otherwise it'll panic
    /// Duration equal 0 if the result is negative
    ///
    /// <math><mfrac><mi>1000 × <msub><mi>N</mi><mn>t</mn></msub></mi><mi>TPS</mi></mfrac> − <msub><mi>Σ</mi><mn>t</mn></msub> = duration</math>
    pub fn get_duration(&self, tps: f64) -> Duration {
        self.get_duration_overhead(tps, None)
    }
}

impl Default for Speed {
    /// Default speed with a sample size of 15
    fn default() -> Self {
        Speed {
            event_speeds: VecDeque::with_capacity(15),
        }
    }
}

impl PartialEq for Speed {
    fn eq(&self, other: &Self) -> bool {
        self.get_speed() == other.get_speed()
    }
}

impl PartialOrd for Speed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get_speed().partial_cmp(&other.get_speed())
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "{} TPS (mean {} ms)",
            self.get_speed(),
            self.get_mean_duration().as_millis()
        )
    }
}

/// Transaction regulator use to asynchronously regulate flow to fixed TPS
///
/// ```
/// use std::time::Duration;
/// use tokio::sync::mpsc;
/// use prosa::event::speed::Regulator;
///
/// async fn queue_regulation(regulator: &mut Regulator, tx: mpsc::Sender<u16>, mut rx: mpsc::Receiver<u16>) {
///     tokio::select! {
///         _ = rx.recv() => {
///             // Specify a response time if you have it to avoid spamming a sick receiver
///             regulator.notify_receive_transaction(Duration::default());
///         }
///         _ = regulator.tick() => {
///             // Can send a transaction
///             tx.send(1234);
///             regulator.notify_send_transaction();
///         }
///     };
/// }
/// ```
pub struct Regulator {
    /// Maximum TPS speed
    max_speed: f64,
    /// Threshold time before sending the next request if the distant respond a timeout (to not overload the distant)
    timeout_threshold: Duration,
    /// Maximum concurents request in parallel
    max_concurrents_send: u32,

    /// Speed of the regulator
    speed: Speed,
    /// Condition variable on concurents send
    concurent_notify: Notify,
    /// Current number of concurrents send
    current_concurrents_send: u32,
    /// Overhead when a timeout occur
    tick_overhead: Option<Duration>,
}

impl Regulator {
    /// Create a new regulator with:
    /// - Maximum TPS speed
    /// - Threshold time before sending the next request if the distant respond a timeout (to not overload the distant)
    /// - Maximum concurents request in parallel
    /// - Number of interval used to know TPS rate (15 by default)
    pub fn new(
        max_speed: f64,
        timeout_threshold: Duration,
        max_concurrents_send: u32,
        speed_interval: u16,
    ) -> Regulator {
        Regulator {
            max_speed,
            timeout_threshold,
            max_concurrents_send,

            speed: Speed::new(speed_interval),
            concurent_notify: Notify::new(),
            current_concurrents_send: 0,
            tick_overhead: None,
        }
    }

    /// Method to synchronize regulator sending rate
    pub async fn tick(&mut self) {
        #[allow(clippy::while_immutable_condition)]
        while self.current_concurrents_send >= self.max_concurrents_send {
            self.concurent_notify.notified().await;
        }

        let duration = self
            .speed
            .get_duration_overhead(self.max_speed, self.tick_overhead);
        if !duration.is_zero() {
            sleep(duration);
        } else {
            self.tick_overhead.take();
        }
    }

    /// Indicate that a new transaction have been sent
    pub fn notify_send_transaction(&mut self) {
        self.speed.time();
        self.current_concurrents_send += 1;
    }

    /// Indicate that we receive a response to a sended transaction
    pub fn notify_receive_transaction(&mut self, response_time: Duration) {
        if response_time > self.timeout_threshold {
            self.tick_overhead = Some(response_time - self.timeout_threshold);
        } else {
            self.tick_overhead = None;
        }

        self.current_concurrents_send -= 1;
        self.concurent_notify.notify_one();
    }

    /// Getter of the current speed of transaction flow
    pub fn get_speed(&self) -> f64 {
        self.speed.get_speed()
    }
}

impl Default for Regulator {
    /// Default regulator
    /// - Send maximum 5 TPS
    /// - With a timeout threshold of 5 second
    /// - A maximum of one concurent request in parallel
    fn default() -> Self {
        Regulator {
            max_speed: 5.0,
            timeout_threshold: Duration::from_secs(5),
            max_concurrents_send: 1,

            speed: Speed::default(),
            concurent_notify: Notify::new(),
            current_concurrents_send: 0,
            tick_overhead: None,
        }
    }
}

impl fmt::Display for Regulator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            " - Tps                            : {} / {}",
            self.speed.get_speed(),
            self.max_speed
        )?;
        writeln!(
            f,
            " - Timeout Threshold              : {} ms",
            self.timeout_threshold.as_millis()
        )?;
        writeln!(
            f,
            " - Maximum concurents transactions: {}",
            self.max_concurrents_send
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    const TPS: f64 = 25.0;

    #[test]
    fn speed_test() {
        let mut speed = Speed::new(5);
        assert_eq!(None, speed.get_last_event());
        assert_eq!(Speed::default(), speed);
        assert!(Speed::default() <= speed);
        assert_eq!("0 TPS (mean 0 ms)\n", speed.to_string().as_str());

        // Send transactions
        speed.time();
        assert!(speed.get_last_event().is_some());
        for _ in 1..=4 {
            speed.time();
            sleep(Duration::from_millis(40));
        }

        // Replace the first transaction time (size > 5)
        speed.time();
        let mut duration = speed.get_duration(TPS); // 40 miliseconds for the duration to keep the same TPS rate
        assert!(duration.as_millis() >= 36 && duration.as_millis() <= 44);
        sleep(Duration::from_millis(40));

        let mean_duration = speed.get_mean_duration(); // 40 miliseconds between each transactions
        assert!(mean_duration.as_millis() >= 36 && mean_duration.as_millis() <= 44);
        let speed_tps = speed.get_speed().round(); // 25 TPS
        assert_eq!(TPS, speed_tps);
        duration = speed.get_duration(TPS); // 0 miliseconds for the duration to keep the same TPS rate
        assert!(duration.as_millis() <= 4);
    }

    #[tokio::test]
    async fn regulator_test() {
        let mut regulator = Regulator::new(TPS, Duration::from_secs(3), 1, 5);
        assert_eq!(0f64, regulator.get_speed());
        assert_eq!(" - Tps                            : 0 / 5\n - Timeout Threshold              : 5000 ms\n - Maximum concurents transactions: 1\n", Regulator::default().to_string().as_str());
        assert_eq!(" - Tps                            : 0 / 25\n - Timeout Threshold              : 3000 ms\n - Maximum concurents transactions: 1\n", regulator.to_string().as_str());

        for _ in 1..=5 {
            regulator.notify_send_transaction();
            regulator.notify_receive_transaction(Duration::from_millis(10));
            sleep(Duration::from_millis(40));
        }

        let mut initial_time = Instant::now();
        regulator.tick().await;
        regulator.notify_send_transaction();
        assert!(initial_time.elapsed() <= Duration::from_millis(1));

        assert!(timeout(Duration::from_millis(400), regulator.tick())
            .await
            .is_err());
        regulator.notify_receive_transaction(Duration::from_millis(10));

        for _ in 1..=5 {
            regulator.notify_send_transaction();
            regulator.notify_receive_transaction(Duration::from_millis(10));
            sleep(Duration::from_millis(10));
        }

        initial_time = Instant::now();
        regulator.tick().await;
        assert!(initial_time.elapsed() >= Duration::from_millis(100));
    }
}
