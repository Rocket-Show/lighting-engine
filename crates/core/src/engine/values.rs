//! The values of the channels of a part of a fixture while a frame is worked out.

/// the channel has a value
pub const SET: u32 = 1;
/// a preset or an effect put the value there (not the default of the channel)
pub const TOUCHED: u32 = 2;

/// The values of a part, to read and to write.
pub struct Vals<'m> {
    pub values: &'m mut [u32],
    pub flags: &'m mut [u32],
}

impl<'m> Vals<'m> {
    pub fn get(&self, channel: usize) -> Option<f32> {
        if self.flags.get(channel).copied().unwrap_or(0) & SET != 0 {
            self.values.get(channel).map(|bits| f32::from_bits(*bits))
        } else {
            None
        }
    }

    pub fn touched(&self, channel: usize) -> bool {
        self.flags.get(channel).copied().unwrap_or(0) & TOUCHED != 0
    }

    fn put(&mut self, channel: usize, value: f32, flags: u32) {
        if let (Some(slot), Some(flag)) = (self.values.get_mut(channel), self.flags.get_mut(channel)) {
            *slot = value.to_bits();
            *flag = flags;
        }
    }

    /// the value a channel starts out with
    pub fn set_default(&mut self, channel: usize, value: f32) {
        self.put(channel, value, SET);
    }

    /// put a value on a channel, over what is on it
    pub fn set(&mut self, channel: usize, value: f32) {
        self.put(channel, value, SET | TOUCHED);
    }

    /// Mix a value over what is on a channel (or 0) by an intensity: 1 covers it, less fades between both.
    pub fn mix(&mut self, channel: usize, value: f32, intensity: f32) {
        let mut value = value;

        if intensity < 1.0 {
            let existing = self.get(channel).unwrap_or(0.0);
            value = existing * (1.0 - intensity) + value * intensity;
        }

        self.set(channel, value);
    }

    pub fn clear(&mut self) {
        for flag in self.flags.iter_mut() {
            *flag = 0;
        }
    }
}

/// The values of a part as they were at a moment, to read.
pub struct Snapshot<'m> {
    pub values: &'m [u32],
    pub flags: &'m [u32],
}

impl<'m> Snapshot<'m> {
    pub fn get(&self, channel: usize) -> Option<f32> {
        if self.flags.get(channel).copied().unwrap_or(0) & SET != 0 {
            self.values.get(channel).map(|bits| f32::from_bits(*bits))
        } else {
            None
        }
    }
}
