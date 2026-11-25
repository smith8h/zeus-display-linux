//! Background loop: reads sensors once a second and feeds the display four frames a second,
//! reconnecting whenever the cooler is unplugged, replugged or the machine wakes from sleep.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::device::{Device, Link};
use crate::protocol;
use crate::sensors::{Reading, Sensors};

const FRAME_INTERVAL: Duration = Duration::from_millis(250);
const RETRY: Duration = Duration::from_secs(2);
const WAKE_SETTLE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy)]
pub struct Update {
    pub reading: Reading,
    pub link: Link,
}

pub struct Worker {
    pub fahrenheit: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(fahrenheit: bool, tx: async_channel::Sender<Update>) -> Self {
        let unit = Arc::new(AtomicBool::new(fahrenheit));
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let (unit, stop) = (unit.clone(), stop.clone());
            thread::Builder::new()
                .name("zeus-display".into())
                .spawn(move || run(&unit, &stop, &tx))
                .expect("spawn display thread")
        };
        Self { fahrenheit: unit, stop, handle: Some(handle) }
    }

    /// Stops the loop and blanks the display with the vendor's idle packet.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(fahrenheit: &AtomicBool, stop: &AtomicBool, tx: &async_channel::Sender<Update>) {
    let mut sensors = Sensors::new();
    let mut device = Device::default();
    let mut reading = Reading::default();
    let mut link = Link::Missing;
    let mut next_open = Instant::now();
    let mut last_tick = Instant::now();
    let mut tick: u64 = 0;

    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();

        // A long gap between ticks means the machine slept: the handle is stale and USB needs a moment.
        if now.duration_since(last_tick) > Duration::from_secs(3) {
            device.close();
            next_open = now + WAKE_SETTLE;
        }
        last_tick = now;

        let second = tick % 4 == 0;
        tick = tick.wrapping_add(1);
        if second {
            reading = sensors.read();
        }

        if !device.is_open() && now >= next_open {
            match device.open() {
                Ok(()) => {}
                Err(why) => {
                    link = why;
                    next_open = now + RETRY;
                }
            }
        }

        if device.is_open() {
            let frame = protocol::build_frame(
                reading.temp_c.unwrap_or(0.0),
                reading.load,
                fahrenheit.load(Ordering::Relaxed),
            );
            link = if device.send(&frame) {
                Link::Live
            } else {
                next_open = now + RETRY;
                Link::WriteFailed
            };
        }

        if second {
            let _ = tx.try_send(Update { reading, link });
        }

        let elapsed = now.elapsed();
        if elapsed < FRAME_INTERVAL {
            thread::sleep(FRAME_INTERVAL - elapsed);
        }
    }

    if device.is_open() || device.open().is_ok() {
        device.send(&protocol::shutdown_frame());
    }
}
