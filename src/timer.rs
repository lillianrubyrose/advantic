use crate::{Bits, Interrupts};

#[derive(Default)]
pub struct Timer {
	register: u32,
	current: u16,
	pub overflow: bool,
}

#[derive(Default)]
pub struct Timers {
	pub timers: [Timer; 4],
}

fn timer_index(address: u32) -> usize {
	((address - 0x100) / 4) as usize
}

impl Timers {
	pub fn step(&mut self, interrupts: &mut Interrupts, cycle: u32) {
		let mut overflow = false;
		for (i, timer) in self.timers.iter_mut().enumerate() {
			let cond = if timer.register.bit(18) {
				overflow
			} else {
				let divider = match (timer.register >> 16) & 0b11 {
					0b00 => 1,
					0b01 => 64,
					0b10 => 256,
					0b11 => 1024,
					_ => unreachable!(),
				};
				cycle.is_multiple_of(divider)
			};
			overflow = false;

			if timer.register.bit(23) && cond {
				timer.current = timer.current.wrapping_add(1);
				overflow = timer.current == 0;
				if overflow {
					timer.current = timer.register as u16;
					if timer.register.bit(22) {
						interrupts.interrupt(3 + i as u32);
					}
				}
			}
			timer.overflow = overflow;
		}
	}

	pub fn read_register(&self, address: u32) -> u32 {
		let timer = &self.timers[timer_index(address)];
		timer.register | u32::from(timer.current)
	}

	pub fn write_register(&mut self, address: u32, value: u32) {
		let timer = &mut self.timers[timer_index(address)];
		if !timer.register.bit(23) && value.bit(23) {
			timer.current = value as u16;
		}
		timer.register = value & 0xffff_0000;
	}
}
