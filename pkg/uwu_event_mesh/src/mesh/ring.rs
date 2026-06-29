//! A bounded ring channel used to back `BackpressurePolicy::DropOldest`.
//!
//! Unlike `tokio::mpsc`, this channel pops the oldest queued item when the
//! buffer is full instead of blocking the sender. Receiver is async via
//! `Notify`. Only single-consumer is supported (matching `Subscription`).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tokio::sync::Notify;

struct Inner<T> {
    cap: usize,
    queue: Mutex<VecDeque<T>>,
    notify: Notify,
    closed: AtomicBool,
    sender_count: Mutex<usize>,
}

pub(super) struct RingSender<T> {
    inner: Arc<Inner<T>>,
}

pub(super) struct RingReceiver<T> {
    inner: Arc<Inner<T>>,
}

pub(super) fn channel<T>(cap: usize) -> (RingSender<T>, RingReceiver<T>) {
    let inner = Arc::new(Inner {
        cap: cap.max(1),
        queue: Mutex::new(VecDeque::with_capacity(cap.max(1))),
        notify: Notify::new(),
        closed: AtomicBool::new(false),
        sender_count: Mutex::new(1),
    });
    (
        RingSender { inner: inner.clone() },
        RingReceiver { inner },
    )
}

impl<T> Clone for RingSender<T> {
    fn clone(&self) -> Self {
        *self.inner.sender_count.lock() += 1;
        Self { inner: self.inner.clone() }
    }
}

impl<T> Drop for RingSender<T> {
    fn drop(&mut self) {
        let mut c = self.inner.sender_count.lock();
        *c -= 1;
        if *c == 0 {
            self.inner.closed.store(true, Ordering::Release);
            self.inner.notify.notify_waiters();
        }
    }
}

pub(super) enum SendOutcome {
    Enqueued,
    EvictedOldest,
    Closed,
}

impl<T> RingSender<T> {
    pub fn send(&self, item: T) -> SendOutcome {
        if self.inner.closed.load(Ordering::Acquire) {
            return SendOutcome::Closed;
        }
        let mut q = self.inner.queue.lock();
        let evicted = if q.len() == self.inner.cap {
            q.pop_front();
            true
        } else {
            false
        };
        q.push_back(item);
        drop(q);
        self.inner.notify.notify_one();
        if evicted {
            SendOutcome::EvictedOldest
        } else {
            SendOutcome::Enqueued
        }
    }

    #[allow(dead_code)]
    pub fn is_closed(&self) -> bool {
        // Receiver closed: any future recv on the receiver itself is N/A,
        // but for sender-side `is_closed` we report whether all senders are
        // gone (always false from a live sender) OR receiver dropped (we
        // don't track that explicitly; treat as open while receiver alive).
        false
    }

    #[allow(dead_code)]
    pub fn queued(&self) -> usize {
        self.inner.queue.lock().len()
    }
}

impl<T> RingReceiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        loop {
            // Fast path.
            if let Some(v) = self.inner.queue.lock().pop_front() {
                return Some(v);
            }
            if self.inner.closed.load(Ordering::Acquire) {
                // Drain anything that arrived between checks.
                if let Some(v) = self.inner.queue.lock().pop_front() {
                    return Some(v);
                }
                return None;
            }
            // Register for notification, then re-check (avoid lost wakeups).
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(v) = self.inner.queue.lock().pop_front() {
                return Some(v);
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return None;
            }
            notified.await;
        }
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        if let Some(v) = self.inner.queue.lock().pop_front() {
            return Ok(v);
        }
        if self.inner.closed.load(Ordering::Acquire) {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drop_oldest_pops_front() {
        let (tx, mut rx) = channel::<u32>(3);
        for i in 0..5u32 {
            tx.send(i);
        }
        // After 5 sends with cap=3, queue should hold [2,3,4].
        let mut got = Vec::new();
        while let Ok(v) = rx.try_recv() {
            got.push(v);
        }
        assert_eq!(got, vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn recv_async_wakes() {
        let (tx, mut rx) = channel::<u32>(2);
        let h = tokio::spawn(async move { rx.recv().await });
        tokio::task::yield_now().await;
        tx.send(42);
        assert_eq!(h.await.unwrap(), Some(42));
    }
}
