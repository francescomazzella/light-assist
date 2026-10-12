//! Choosing the picture's size, frame rate and bitrate during a session.
//!
//! The helper may fix any of the three; whatever they leave on automatic is chosen here, once a second,
//! from what the network can carry, what the sharer's encoder keeps up with, and how large the helper's
//! window is. For text (the default), sharpness comes first: the frame rate gives way before the size
//! does. For motion it is the other way round. For a fast response the picture comes as often as for
//! motion and is kept light, so that each one crosses the network quickly.

use serde::{Deserialize, Serialize};

/// Picture heights to step between, largest first. The screen's own size always comes first.
const HEIGHTS: [u32; 5] = [2160, 1440, 1080, 720, 540];
/// Frame rates to choose from, highest first; text never goes above the first of its own list.
const TEXT_RATES: [u32; 3] = [30, 20, 15];
const MOTION_RATES: [u32; 3] = [60, 30, 20];

/// Bits per pixel below which the picture visibly falls apart, and above which more bits buy nothing.
/// A desktop is mostly still, so it gets by on far less than its size suggests; the low figures keep
/// the picture large on connections that would otherwise seem too slow for it.
fn bits_per_pixel(content: Content) -> (f64, f64) {
    match content {
        Content::Text => (0.015, 0.12),
        Content::Motion => (0.02, 0.10),
        Content::Response => (0.012, 0.05),
    }
}

/// What to assume before the network has been measured.
const STARTING_ESTIMATE_BPS: f64 = 8_000_000.0;
/// Part of the measured capacity to use: the rest leaves room for the connection's own traffic and
/// for the measurement being optimistic.
const HEADROOM: f64 = 0.85;
/// Sending less than this share of the bitrate the encoder may use means the picture, not the network, sets the pace.
const APP_LIMITED: f64 = 0.5;
const MIN_BITRATE_BPS: u32 = 400_000;

/// Seconds a lower frame rate must be called for before it is used, and a higher one. Changing the
/// rate costs nothing.
const STEP_DOWN_AFTER: u32 = 2;
const STEP_UP_AFTER: u32 = 6;
/// The same for the picture's size. A new size starts with a whole picture sent at once, which a slow
/// connection takes a moment to swallow, so the size changes rarely.
const RESIZE_DOWN_AFTER: u32 = 5;
const RESIZE_UP_AFTER: u32 = 20;
/// The encoder is behind when a frame takes longer than the time there is for one, and has caught up
/// when it leaves some of that time over.
const ENCODER_BEHIND: f32 = 1.0;
const ENCODER_KEEPING_UP: f32 = 0.9;

/// When the sharer is told their connection is slow, and when that is lifted again.
const SLOW_BELOW_BPS: f64 = 1_500_000.0;
const SLOW_LOSS: f32 = 0.05;
const SLOW_AFTER: u32 = 3;
const FAST_ABOVE_BPS: f64 = 2_500_000.0;
const FAST_LOSS: f32 = 0.02;
const FAST_AFTER: u32 = 5;

/// What the picture mostly shows, which decides what gives way first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Content {
    #[default]
    Text,
    Motion,
    /// What matters is how soon the screen answers the helper's mouse and keyboard.
    Response,
}

impl Content {
    /// Whether the helper's browser should show each picture the moment it arrives. Holding pictures
    /// back for a moment evens out a jittery connection, which video wants and remote control does not.
    pub fn immediate(self) -> bool {
        self != Content::Motion
    }
}

/// The helper's choices. `None` is automatic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preference {
    pub content: Content,
    /// The picture's height in pixels; 0 is the screen's own size.
    pub height: Option<u32>,
    pub fps: Option<u32>,
    pub bitrate_kbps: Option<u32>,
}

/// What was measured over the last second.
#[derive(Clone, Copy, Debug, Default)]
pub struct Observation {
    /// The network's estimated capacity towards the helper.
    pub estimate_bps: Option<f64>,
    /// What was actually sent.
    pub sent_bps: Option<f64>,
    pub loss: Option<f32>,
    /// The helper's picture area, in the screen's own pixels.
    pub viewer: Option<(u32, u32)>,
    /// The helper's browser is dropping pictures it cannot decode in time.
    pub viewer_struggling: bool,
    /// How much of each frame's time capture and encoding took, 0 to 1 and beyond.
    pub encoder_busy: f32,
}

/// What the capture and encoder should produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_bps: u32,
    pub content: Content,
}

pub struct Controller {
    preference: Preference,
    /// The screen's own size.
    native: (u32, u32),
    current: Target,
    /// A different size and rate that has been called for, and for how many seconds in a row.
    pending: Option<(((u32, u32), u32), u32)>,
    /// The most pixels a second that the sharer's computer and the helper's browser have kept up with.
    pixel_rate_cap: f64,
    /// The capacity believed, which only follows a falling estimate when it is evidence of a slower network.
    estimate: f64,
    /// The capacity last measured. Nothing is sent faster than this, whatever is believed, so the bitrate
    /// always follows it.
    measured: f64,
    /// The helper's picture area, as last told.
    viewer: Option<(u32, u32)>,
    slow: bool,
    slow_streak: u32,
    fast_streak: u32,
}

/// The width that goes with `height` at the screen's shape, made even as encoders need.
fn width_for(native: (u32, u32), height: u32) -> u32 {
    ((native.0 as u64 * height as u64 / native.1.max(1) as u64) as u32 & !1).max(2)
}

impl Controller {
    pub fn new(native: (u32, u32)) -> Self {
        let mut controller = Self {
            preference: Preference::default(),
            native,
            current: Target { width: 0, height: 0, fps: 30, bitrate_bps: 0, content: Content::Text },
            pending: None,
            pixel_rate_cap: f64::INFINITY,
            estimate: STARTING_ESTIMATE_BPS,
            measured: STARTING_ESTIMATE_BPS,
            viewer: None,
            slow: false,
            slow_streak: 0,
            fast_streak: 0,
        };
        // Start where a 1080p picture would at 30 frames a second, until something is measured.
        let (size, fps) = controller.best_shape(STARTING_ESTIMATE_BPS * HEADROOM, None);
        controller.current = controller.target_for(size, fps, STARTING_ESTIMATE_BPS * HEADROOM);
        controller
    }

    pub fn target(&self) -> Target {
        self.current
    }

    pub fn slow(&self) -> bool {
        self.slow
    }

    /// A new choice from the helper takes effect at once.
    pub fn set_preference(&mut self, preference: Preference) {
        self.preference = preference;
        self.pending = None;
        let (size, fps) = self.best_shape(self.estimate * HEADROOM, self.viewer);
        self.current = self.target_for(size, fps, self.measured * HEADROOM);
    }

    /// The screen being shown changed size, or another screen was chosen.
    pub fn set_native(&mut self, native: (u32, u32)) {
        if native != self.native {
            self.native = native;
            self.pending = None;
            let (size, fps) = self.best_shape(self.estimate * HEADROOM, self.viewer);
            self.current = self.target_for(size, fps, self.measured * HEADROOM);
        }
    }

    /// Heights the picture may take, largest first: the screen's own, then the steps below it.
    fn heights(&self, viewer: Option<(u32, u32)>) -> Vec<u32> {
        let native = self.native.1;
        if let Some(fixed) = self.preference.height {
            return vec![if fixed == 0 { native } else { fixed.min(native) }];
        }
        // No more pixels than the helper's window can show, but never below the smallest step.
        let viewer_cap = viewer.map_or(u32::MAX, |(width, height)| {
            let by_width = (width as u64 * native as u64 / self.native.0.max(1) as u64) as u32;
            height.min(by_width)
        });
        let mut heights: Vec<u32> = std::iter::once(native).chain(HEIGHTS.into_iter().filter(|&height| height < native)).collect();
        let fitting: Vec<u32> = heights.iter().copied().filter(|&height| height <= viewer_cap).collect();
        // A window smaller than every step still gets the smallest step.
        if fitting.is_empty() {
            heights = vec![*heights.last().unwrap_or(&native)];
        } else {
            // The smallest step that is at least the window's size, so the picture is never upscaled by much.
            let larger = heights.iter().copied().filter(|&height| height > viewer_cap).last();
            heights = larger.into_iter().chain(fitting).collect();
        }
        heights
    }

    fn rates(&self) -> Vec<u32> {
        if let Some(fixed) = self.preference.fps {
            return vec![fixed.clamp(1, 60)];
        }
        match self.preference.content {
            Content::Text => TEXT_RATES.to_vec(),
            Content::Motion | Content::Response => MOTION_RATES.to_vec(),
        }
    }

    /// The size and frame rate that best fit `budget` bits a second, in the order the content asks for.
    fn best_shape(&self, budget: f64, viewer: Option<(u32, u32)>) -> ((u32, u32), u32) {
        let (enough, _) = bits_per_pixel(self.preference.content);
        let fixed_bitrate = self.preference.bitrate_kbps.is_some();
        let fits = |height: u32, fps: u32| {
            let pixels = width_for(self.native, height) as f64 * height as f64 * fps as f64;
            // With a bitrate the helper fixed, the picture is not shrunk for it: they chose that trade-off.
            pixels <= self.pixel_rate_cap && (fixed_bitrate || budget / pixels >= enough)
        };
        let (heights, rates) = (self.heights(viewer), self.rates());

        let choice = match self.preference.content {
            // Sharpness first: the largest picture, at whatever rate it can be carried.
            Content::Text => heights.iter().find_map(|&height| rates.iter().find(|&&fps| fits(height, fps)).map(|&fps| (height, fps))),
            // Smoothness first: the highest rate, at whatever size it can be carried.
            Content::Motion | Content::Response => rates.iter().find_map(|&fps| heights.iter().find(|&&height| fits(height, fps)).map(|&height| (height, fps))),
        };
        let (height, fps) = choice.unwrap_or((*heights.last().unwrap_or(&self.native.1), *rates.last().unwrap_or(&30)));
        ((width_for(self.native, height), height), fps)
    }

    fn target_for(&self, (width, height): (u32, u32), fps: u32, budget: f64) -> Target {
        let bitrate = match self.preference.bitrate_kbps {
            Some(kbps) => kbps.saturating_mul(1000),
            None => {
                // No more than the picture can use.
                let (_, plenty) = bits_per_pixel(self.preference.content);
                let useful = width as f64 * height as f64 * fps as f64 * plenty;
                budget.min(useful) as u32
            }
        };
        Target { width, height, fps, bitrate_bps: bitrate.max(MIN_BITRATE_BPS), content: self.preference.content }
    }

    /// Called once a second with what was measured; returns the target from now on.
    pub fn tick(&mut self, seen: &Observation) -> Target {
        // The sharer's computer or the helper's browser falling behind caps how many pixels a second
        // are asked of them; the cap lifts slowly once they keep up again.
        let rate = self.current.width as f64 * self.current.height as f64 * self.current.fps as f64;
        if seen.encoder_busy > ENCODER_BEHIND || seen.viewer_struggling {
            self.pixel_rate_cap = self.pixel_rate_cap.min(rate * 0.75);
        } else if seen.encoder_busy < ENCODER_KEEPING_UP && self.pixel_rate_cap.is_finite() {
            self.pixel_rate_cap *= 1.25;
            if self.pixel_rate_cap > rate * 4.0 {
                self.pixel_rate_cap = f64::INFINITY;
            }
        }

        // A still screen sends next to nothing, and the estimate cannot grow, or even hold, without traffic
        // to measure. So while the picture uses little of the link, a falling estimate says more about the
        // picture than about the network, and only loss is believed.
        let measured = seen.estimate_bps.unwrap_or(STARTING_ESTIMATE_BPS);
        let app_limited = seen.sent_bps.is_some_and(|sent| sent < self.current.bitrate_bps as f64 * APP_LIMITED);
        let losing = seen.loss.unwrap_or(0.0) >= FAST_LOSS;
        if measured >= self.estimate || !app_limited || losing {
            self.estimate = measured;
        }
        self.measured = measured;
        self.viewer = seen.viewer.or(self.viewer);
        // The size and rate come from what is believed; the bitrate from what was measured.
        let (budget, spend) = (self.estimate * HEADROOM, measured * HEADROOM);
        let (size, fps) = self.best_shape(budget, seen.viewer);
        let shape = (size, fps);
        let now = ((self.current.width, self.current.height), self.current.fps);

        if shape != now {
            let pixels = |((width, height), fps): ((u32, u32), u32)| width as u64 * height as u64 * fps as u64;
            let wait = match (shape.0 != now.0, pixels(shape) < pixels(now)) {
                (true, true) => RESIZE_DOWN_AFTER,
                (true, false) => RESIZE_UP_AFTER,
                (false, true) => STEP_DOWN_AFTER,
                (false, false) => STEP_UP_AFTER,
            };
            let streak = match self.pending {
                Some((pending, streak)) if pending == shape => streak + 1,
                _ => 1,
            };
            self.pending = Some((shape, streak));
            if streak >= wait {
                self.pending = None;
                self.current = self.target_for(size, fps, spend);
                self.update_slow(seen);
                return self.current;
            }
        } else {
            self.pending = None;
        }
        // The size stays; the bitrate follows the network at once.
        self.current = self.target_for(now.0, now.1, spend);
        self.update_slow(seen);
        self.current
    }

    fn update_slow(&mut self, seen: &Observation) {
        let estimate = self.estimate;
        let loss = seen.loss.unwrap_or(0.0);
        if estimate < SLOW_BELOW_BPS || loss > SLOW_LOSS {
            self.slow_streak += 1;
            self.fast_streak = 0;
        } else if estimate > FAST_ABOVE_BPS && loss < FAST_LOSS {
            self.fast_streak += 1;
            self.slow_streak = 0;
        }
        if !self.slow && self.slow_streak >= SLOW_AFTER {
            self.slow = true;
        } else if self.slow && self.fast_streak >= FAST_AFTER {
            self.slow = false;
        }
    }
}

/// How much the connection to the helper carries, judged from what comes back about what was sent:
/// packets lost and the time a round trip takes. Nothing is held back to measure it, so the picture
/// is never delayed for the sake of the estimate.
pub struct Link {
    bps: f64,
    /// The shortest round trip seen, which is the connection when nothing queues on it.
    best_rtt: Option<f64>,
    /// Seconds in a row with signs of trouble.
    troubled: u32,
}

const LINK_FLOOR_BPS: f64 = 300_000.0;
const LINK_CEILING_BPS: f64 = 50_000_000.0;
/// A round trip this much longer than the shortest means packets are queueing somewhere. Mobile
/// connections wander by a hundred milliseconds or more without it meaning anything.
const QUEUEING_RTT_SECS: f64 = 0.3;
const LOCAL_RTT_SECS: f64 = 0.005;
const LOCAL_BPS: f64 = 30_000_000.0;

impl Link {
    pub fn new() -> Self {
        Self { bps: STARTING_ESTIMATE_BPS, best_rtt: None, troubled: 0 }
    }

    /// Takes a second's measurements and gives the new estimate.
    pub fn update(&mut self, sent_bps: Option<f64>, loss: Option<f32>, rtt_secs: Option<f64>) -> f64 {
        let loss = loss.unwrap_or(0.0);
        if let Some(rtt) = rtt_secs {
            self.best_rtt = Some(self.best_rtt.map_or(rtt, |best| best.min(rtt)));
        }
        let queueing = matches!((rtt_secs, self.best_rtt), (Some(rtt), Some(best)) if rtt > best + QUEUEING_RTT_SECS);

        if loss > SLOW_LOSS || queueing {
            // One bad second is weather; two in a row is the connection.
            self.troubled += 1;
            if self.troubled >= 2 {
                // No lower than what is getting through now, which is known to fit.
                let floor = sent_bps.map_or(0.0, |sent| sent * (1.0 - loss as f64) * 0.9);
                self.bps = (self.bps * 0.85).max(floor.min(self.bps));
            }
        } else {
            self.troubled = 0;
            // Only what was actually tried is known to fit, so the estimate grows while it is being used.
            let used = sent_bps.is_some_and(|sent| sent > self.bps * APP_LIMITED);
            // A round trip of a few milliseconds is a local network, which carries any picture there is.
            let local = self.best_rtt.is_some_and(|best| best < LOCAL_RTT_SECS) && self.bps < LOCAL_BPS;
            if loss < FAST_LOSS && (used || local) {
                self.bps *= 1.15;
            }
        }
        self.bps = self.bps.clamp(LINK_FLOOR_BPS, LINK_CEILING_BPS);
        self.bps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_estimate_falls_on_loss_and_grows_only_while_used() {
        let mut link = Link::new();
        let start = link.update(Some(100_000.0), Some(0.0), Some(0.02));
        assert_eq!(start, STARTING_ESTIMATE_BPS, "a still screen proves nothing either way");
        assert!(Link::new().update(Some(100_000.0), Some(0.0), Some(0.001)) > start, "except on a local network");
        let grown = link.update(Some(7_000_000.0), Some(0.0), Some(0.02));
        assert!(grown > start);
        let blip = link.update(Some(7_000_000.0), Some(0.1), Some(0.02));
        assert_eq!(blip, grown, "one bad second changes nothing");
        let lossy = link.update(Some(7_000_000.0), Some(0.1), Some(0.02));
        assert!(lossy < grown);
        let queued = link.update(Some(7_000_000.0), Some(0.0), Some(0.4));
        assert!(queued < lossy);
    }

    const SCREEN: (u32, u32) = (2560, 1440);

    fn seen(mbps: f64) -> Observation {
        Observation { estimate_bps: Some(mbps * 1e6), loss: Some(0.0), ..Default::default() }
    }

    fn settle(controller: &mut Controller, observation: &Observation) -> Target {
        for _ in 0..60 {
            controller.tick(observation);
        }
        controller.target()
    }

    #[test]
    fn a_fast_network_gets_the_whole_screen_at_30() {
        let mut controller = Controller::new(SCREEN);
        let target = settle(&mut controller, &seen(100.0));
        assert_eq!((target.width, target.height, target.fps), (2560, 1440, 30));
    }

    #[test]
    fn text_gives_up_frame_rate_before_size() {
        let mut controller = Controller::new(SCREEN);
        // 1.5 Mbps: too little for 1440p at 30, enough at 20.
        let target = settle(&mut controller, &seen(1.5));
        assert_eq!(target.height, 1440);
        assert!(target.fps < 30, "{target:?}");
    }

    #[test]
    fn motion_gives_up_size_before_frame_rate() {
        let mut controller = Controller::new(SCREEN);
        controller.set_preference(Preference { content: Content::Motion, ..Default::default() });
        let target = settle(&mut controller, &seen(3.0));
        assert_eq!(target.fps, 60);
        assert!(target.height < 1440, "{target:?}");
    }

    #[test]
    fn the_helpers_choices_win() {
        let mut controller = Controller::new(SCREEN);
        controller.set_preference(Preference { height: Some(720), fps: Some(60), bitrate_kbps: Some(3000), ..Default::default() });
        let target = settle(&mut controller, &seen(1.0));
        assert_eq!((target.width, target.height, target.fps, target.bitrate_bps), (1280, 720, 60, 3_000_000));
    }

    #[test]
    fn no_more_pixels_than_the_helpers_window() {
        let mut controller = Controller::new(SCREEN);
        let target = settle(&mut controller, &Observation { viewer: Some((1300, 730)), ..seen(100.0) });
        // The smallest step that covers the window: 1080p is more than 730 pixels, 720p is less.
        assert_eq!(target.height, 1080);
    }

    #[test]
    fn a_struggling_encoder_is_asked_for_less() {
        let mut controller = Controller::new(SCREEN);
        settle(&mut controller, &seen(100.0));
        let busy = Observation { encoder_busy: 1.2, ..seen(100.0) };
        let before = controller.target();
        let after = settle(&mut controller, &busy);
        assert!(after.width * after.height * after.fps < before.width * before.height * before.fps, "{before:?} {after:?}");
    }

    #[test]
    fn size_does_not_flap() {
        let mut controller = Controller::new(SCREEN);
        settle(&mut controller, &seen(100.0));
        // One bad second is not enough to change size.
        let target = controller.tick(&seen(1.0));
        assert_eq!(target.height, 1440);
    }

    #[test]
    fn slow_is_reported_after_a_few_seconds_and_lifted_after_more() {
        let mut controller = Controller::new(SCREEN);
        for _ in 0..2 {
            controller.tick(&seen(1.0));
        }
        assert!(!controller.slow());
        controller.tick(&seen(1.0));
        assert!(controller.slow());
        for _ in 0..4 {
            controller.tick(&seen(10.0));
        }
        assert!(controller.slow());
        controller.tick(&seen(10.0));
        assert!(!controller.slow());
    }

    #[test]
    fn a_still_screen_does_not_shrink_the_picture() {
        let mut controller = Controller::new(SCREEN);
        settle(&mut controller, &seen(20.0));
        let before = controller.target();
        // Almost nothing is sent, and the estimate sinks with it, but nothing is lost.
        let still = Observation { sent_bps: Some(100_000.0), ..seen(0.8) };
        let target = settle(&mut controller, &still);
        assert_eq!((target.width, target.height), (before.width, before.height));
        assert!(!controller.slow());
        // Losing packets is believed whatever is being sent.
        let lossy = Observation { loss: Some(0.1), ..still };
        let target = settle(&mut controller, &lossy);
        assert!(target.height < before.height, "{target:?}");
    }
}
