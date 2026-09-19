use std::collections::VecDeque;
use crate::Bits;
use crate::timer::Timers;

#[derive(Default)]
pub struct DmaChannel {
	buffer: VecDeque<u32>,
	pub dma_enabled: bool,
}

impl DmaChannel {
	fn step(&mut self, control: u32, timer: &Timers) {
		if control & 0b11 != 0 {
			if control.bit(3) {
				self.buffer.clear();
			} else {
				self.buffer.pop_front();
			}
			if !self.dma_enabled {
				self.dma_enabled = timer.timers[usize::from(control.bit(2))].overflow && self.buffer.len() < 5;
			}
		}
	}
}

#[derive(Default)]
pub struct Audio {
	pub dma: [DmaChannel; 2],
	pub control: [u32; 3]
}

impl Audio {
	pub fn step(&mut self, timers: &Timers) {
		if !self.control[2].bit(7) {
			return;
		}
		for (i, channel) in self.dma.iter_mut().enumerate() {
			channel.step(self.control[0] >> (24 + i * 4), timers);
		}
		self.control[0] &= !(0b10001 << 27);
	}
}