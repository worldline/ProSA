//! Module to help a processor keep a set of sockets in line with a configuration that can change
//! while it runs
//!
//! A processor that owns sockets has to answer three questions every time its configuration is
//! reloaded: which sockets should still exist, what changed for the ones that stay, and what to do
//! with the requests still addressed to the ones that go. Deciding *which* sockets should exist is
//! the processor's own business, because only it knows what makes two of its connections the same.
//! The rest is the same work in every protocol, and is what this module provides:
//!
//! - [`SocketControlSender`] / [`SocketControlReceiver`], to reach a socket out of band from the
//!   requests it serves. A bus queue only reaches a socket that is connected and reading it, so it
//!   cannot carry a decision meant for one that is busy serving or waiting to reconnect.
//! - [`Backoff`], so a peer that is down is not dialled at the speed of the machine.
//! - [`retire_queue`], so the requests addressed to a socket that will not come back are answered
//!   rather than dropped.
//!
#![doc = simple_mermaid::mermaid!("diagrams/pool.mmd")]

use std::{ops::Deref, time::Duration};

use tokio::sync::{mpsc, watch};

use crate::core::{
    msg::{InternalMsg, Msg as _, Tvf},
    service::ServiceError,
};

/// Delays a socket waits between two connection attempts, growing while the peer stays unreachable
///
/// ```
/// use std::time::Duration;
/// use prosa::io::pool::Backoff;
///
/// let backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(30));
///
/// // A socket that never failed connects without waiting
/// assert_eq!(Duration::ZERO, backoff.delay(0));
///
/// // Then the delay doubles on every consecutive failure, up to the maximum
/// assert_eq!(Duration::from_millis(500), backoff.delay(1));
/// assert_eq!(Duration::from_secs(1), backoff.delay(2));
/// assert_eq!(Duration::from_secs(30), backoff.delay(u32::MAX));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    base: Duration,
    max: Duration,
}

impl Backoff {
    /// Shortest a socket ever waits between two connection attempts
    ///
    /// A zero delay would do two things at once: no wait between two attempts, and every attempt
    /// counting as a working connection because it lasted at least [`Self::healthy_connection`].
    /// The retry count would then never grow, and a peer that refuses everything would be dialled
    /// at the speed of the machine.
    pub const MIN_DELAY: Duration = Duration::from_millis(10);

    /// Create a backoff that starts at `base` and never waits more than `max`
    ///
    /// Both are floored at [`Self::MIN_DELAY`], and `base` is capped at `max` so the first attempt
    /// is never slower than the maximum.
    pub fn new(base: Duration, max: Duration) -> Backoff {
        let max = max.max(Self::MIN_DELAY);
        Backoff {
            base: base.min(max).max(Self::MIN_DELAY),
            max,
        }
    }

    /// Getter of the delay of the first reconnection attempt
    pub fn base(&self) -> Duration {
        self.base
    }

    /// Getter of the maximum delay between two reconnection attempts
    pub fn max(&self) -> Duration {
        self.max
    }

    /// Delay to wait before the `retry`-th consecutive connection attempt
    ///
    /// `retry` is zero for a socket that never failed, which connects without waiting.
    pub fn delay(&self, retry: u32) -> Duration {
        if retry == 0 {
            return Duration::ZERO;
        }

        self.base
            .saturating_mul(1u32.checked_shl(retry - 1).unwrap_or(u32::MAX))
            .min(self.max)
    }

    /// How long a connection that served nothing must have lasted to count as a working one
    ///
    /// Equal to the base delay, so a socket never reconnects faster than it would after a refused
    /// connection. A peer that accepts and closes straight away looks the same as one that refuses
    /// to a caller, and reconnecting to either without waiting is a loop at the speed of the
    /// machine.
    pub fn healthy_connection(&self) -> Duration {
        self.base
    }

    /// Retry count after an attempt, reset when the connection worked
    pub fn next_retry(&self, retry: u32, worked: bool) -> u32 {
        if worked { 0 } else { retry.saturating_add(1) }
    }
}

impl Default for Backoff {
    fn default() -> Backoff {
        Backoff::new(Duration::from_millis(500), Duration::from_secs(30))
    }
}

/// What a processor decides for one of its sockets, out of band from the requests it serves
#[derive(Debug)]
struct SocketControl<C> {
    control: C,
    stopped: bool,
}

/// Open the control channel of a socket a processor is about to spawn
///
/// `control` carries whatever applies to the next connection attempt or the next message, so
/// changing it does not tear down a socket that is serving perfectly well. What defines the
/// connection itself does not belong in it: a socket that has to reach somewhere else is a
/// different socket, and the processor replaces it rather than telling it to move.
pub fn control_channel<C>(control: C) -> (SocketControlSender<C>, SocketControlReceiver<C>) {
    let (tx, rx) = watch::channel(SocketControl {
        control,
        stopped: false,
    });

    (SocketControlSender { tx }, SocketControlReceiver { rx })
}

/// Sending end of the control channel of a socket, held by the processor that owns it
#[derive(Debug)]
pub struct SocketControlSender<C> {
    tx: watch::Sender<SocketControl<C>>,
}

impl<C> SocketControlSender<C> {
    /// Replace what the socket reads on its next attempt or tick
    pub fn update<F>(&self, update: F)
    where
        F: FnOnce(&mut C),
    {
        self.tx.send_modify(|control| update(&mut control.control));
    }

    /// Retire the socket
    ///
    /// It answers what it has in flight and does not come back. A socket that is only waiting to
    /// reconnect is unknown to the processor's main loop, so this is the only thing that reaches
    /// it.
    pub fn stop(&self) {
        self.tx.send_modify(|control| control.stopped = true);
    }

    /// Answer `true` once the socket was retired
    pub fn is_stopped(&self) -> bool {
        self.tx.borrow().stopped
    }
}

/// Receiving end of the control channel of a socket, held by the socket itself
#[derive(Debug)]
pub struct SocketControlReceiver<C> {
    rx: watch::Receiver<SocketControl<C>>,
}

impl<C> Clone for SocketControlReceiver<C> {
    fn clone(&self) -> Self {
        SocketControlReceiver {
            rx: self.rx.clone(),
        }
    }
}

impl<C> SocketControlReceiver<C> {
    /// Read what the processor currently decides for this socket
    ///
    /// Reads without marking the value seen, so it never consumes the wake-up [`Self::stopped`] is
    /// waiting for. Holding the returned reference keeps the sender from updating the control, so
    /// read what is needed out of it and let it go.
    pub fn borrow(&self) -> SocketControlRef<'_, C> {
        SocketControlRef {
            control: self.rx.borrow(),
        }
    }

    /// Answer `true` once the processor retired the socket
    ///
    /// Reads without marking the value seen, for the same reason as [`Self::borrow`], so it is safe
    /// to guard a `select!` arm with.
    pub fn is_stopped(&self) -> bool {
        self.rx.borrow().stopped
    }

    /// Resolve once the processor retires the socket, and never otherwise
    ///
    /// A dropped sender counts as retired: a processor only ever lets the sending end go after it
    /// stopped the socket, and a closed channel makes the underlying `changed` return instantly
    /// forever, which as a `select!` arm would spin the socket rather than stop it.
    pub async fn stopped(&mut self) {
        while self.rx.changed().await.is_ok() {
            if self.rx.borrow().stopped {
                return;
            }
        }
    }
}

/// Reference to the control values of a socket, returned by [`SocketControlReceiver::borrow`]
#[derive(Debug)]
pub struct SocketControlRef<'a, C> {
    control: watch::Ref<'a, SocketControl<C>>,
}

impl<C> Deref for SocketControlRef<'_, C> {
    type Target = C;

    fn deref(&self) -> &C {
        &self.control.control
    }
}

/// Answer whatever still reaches a socket that will not come back, until nobody can reach it
///
/// The bus is told to remove the queue when a socket is retired, but a processor that has not
/// received the new service table yet still holds it and still sends to it. Closing the queue there
/// turns a request that should have come back [`ServiceError::UnableToReachService`] into a send
/// error for its sender, and a processor that treats that as fatal restarts on it. So the queue
/// outlives the socket, answering rather than closing, and only goes away once the last sender did.
///
/// `tx` is the sending end the socket held on its own queue, which would keep it open forever, so
/// it is taken by value and dropped here rather than left to the caller to remember.
pub fn retire_queue<M>(mut rx: mpsc::Receiver<InternalMsg<M>>, tx: mpsc::Sender<InternalMsg<M>>)
where
    M: 'static + Send + Sync + Sized + Clone + std::fmt::Debug + Tvf + Default,
{
    drop(tx);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let InternalMsg::Request(request) = msg {
                let service_name = request.get_service().clone();
                let _ = request
                    .return_error_to_sender(None, ServiceError::UnableToReachService(service_name));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_maximum() {
        let backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(30));

        // A socket that never failed reconnects right away
        assert_eq!(Duration::ZERO, backoff.delay(0));

        assert_eq!(Duration::from_millis(500), backoff.delay(1));
        assert_eq!(Duration::from_secs(1), backoff.delay(2));
        assert_eq!(Duration::from_secs(2), backoff.delay(3));
        assert_eq!(Duration::from_secs(30), backoff.delay(7));

        // A peer that stays down for a long time must not overflow the delay
        assert_eq!(Duration::from_secs(30), backoff.delay(u32::MAX));
    }

    #[test]
    fn backoff_base_never_exceeds_the_maximum() {
        // A maximum lower than the first delay caps it, instead of waiting more than the maximum
        // on the very first attempt
        let backoff = Backoff::new(Duration::from_secs(5), Duration::from_secs(1));

        assert_eq!(Duration::from_secs(1), backoff.base());
        assert_eq!(Duration::from_secs(1), backoff.delay(1));
        assert_eq!(Duration::from_secs(1), backoff.delay(9));
    }

    #[test]
    fn backoff_is_never_zero() {
        let backoff = Backoff::new(Duration::ZERO, Duration::ZERO);

        assert!(!backoff.base().is_zero());
        assert!(!backoff.delay(1).is_zero());
        assert!(!backoff.delay(u32::MAX).is_zero());

        // The first attempt of a socket that never failed still doesn't wait
        assert_eq!(Duration::ZERO, backoff.delay(0));
    }

    #[test]
    fn backoff_resets_on_a_working_connection() {
        let backoff = Backoff::default();

        assert_eq!(0, backoff.next_retry(7, true));
        assert_eq!(8, backoff.next_retry(7, false));

        // A peer that stays down for a very long time must not wrap the retry count back to zero,
        // which would make the socket dial it without waiting again
        assert_eq!(u32::MAX, backoff.next_retry(u32::MAX, false));
    }

    #[tokio::test]
    async fn control_reaches_the_socket_without_stopping_it() {
        let (tx, rx) = control_channel(12u32);

        assert_eq!(12, *rx.borrow());
        assert!(!rx.is_stopped());
        assert!(!tx.is_stopped());

        tx.update(|control| *control = 42);
        assert_eq!(42, *rx.borrow());
        assert!(!rx.is_stopped());
    }

    #[tokio::test]
    async fn stopped_resolves_only_once_the_socket_is_retired() {
        let (tx, mut rx) = control_channel(12u32);

        // An update is not a stop, so a socket waiting on `stopped` keeps serving through it
        tx.update(|control| *control = 42);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), rx.stopped())
                .await
                .is_err()
        );

        tx.stop();
        assert!(rx.is_stopped());
        assert!(tx.is_stopped());
        rx.stopped().await;
    }

    #[tokio::test]
    async fn is_stopped_does_not_consume_the_wake_up() {
        let (tx, mut rx) = control_channel(12u32);

        tx.stop();

        // Reading the control marks nothing as seen, so the socket still wakes up on the stop it
        // has just read. Marking it would leave `stopped` waiting forever on a socket that must go
        assert!(rx.is_stopped());
        assert_eq!(12, *rx.borrow());
        rx.stopped().await;
    }

    #[tokio::test]
    async fn a_dropped_sender_retires_the_socket() {
        let (tx, mut rx) = control_channel(12u32);

        // A processor only lets the sending end go after it stopped the socket. Spinning here
        // instead of stopping would burn a core for as long as the socket lives
        drop(tx);
        rx.stopped().await;
    }
}
