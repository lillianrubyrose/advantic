#![warn(clippy::pedantic)]
#![allow(
	clippy::too_many_lines,
	clippy::similar_names,
	clippy::cast_possible_truncation,
	clippy::missing_errors_doc,
	clippy::missing_panics_doc,
	clippy::struct_excessive_bools,
	clippy::too_many_arguments
)]

pub mod audio;
mod cpu;
pub mod mem;
mod ppu;
mod timer;

use crate::audio::Audio;
use crate::mem::{MAX_BIOS_ADDRESS, Memory};
use crate::ppu::{LINE_CYCLES, TOTAL_LINES};
use crate::timer::Timers;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::collections::HashSet;
use std::ops::Add;
use std::time::{Duration, Instant};

trait Bits {
	fn bit(&self, bit: Self) -> bool;
	fn set_bit(&mut self, bit: Self, set: bool);
}

macro_rules! bits {
	($t: ty) => {
		impl Bits for $t {
			fn bit(&self, bit: Self) -> bool {
				(self & (1 << bit)) != 0
			}

			fn set_bit(&mut self, bit: Self, set: bool) {
				let bit = 1 << bit;
				*self = if set { *self | bit } else { *self & !bit }
			}
		}
	};
}
bits!(u8);
bits!(u16);
bits!(u32);

#[derive(Default)]
pub struct Interrupts {
	enabled: bool,
	control: u32,
}

impl Interrupts {
	fn interrupt(&mut self, kind: u32) {
		if self.enabled && self.control.bit(kind) {
			self.control.set_bit(kind + 16, true);
		}
	}

	fn write_register(&mut self, address: u32, value: u32) {
		match address {
			0x200 => self.control = value & 0xffff | (self.control & 0xffff_0000 & !value),
			0x208 => self.enabled = value.bit(0),
			_ => panic!("Invalid interrupt register {address:08x}"),
		}
	}
}

pub struct System {
	ppu: ppu::Ppu,
	pressed_keys: HashSet<Keycode>,
	interrupts: Interrupts,
	timer: Timers,
	audio: Audio,
}

const FRAME_CYCLES: u32 = LINE_CYCLES as u32 * TOTAL_LINES as u32;
const CYCLES_PER_SECOND: u32 = 2u32.pow(24);
const SYNC_CYCLE_DURATION: u32 = (Duration::from_secs(1).as_nanos() as u32) / (CYCLES_PER_SECOND / FRAME_CYCLES);

fn main() {
	let path = std::env::args().nth(1).expect("No path given");
	let bios = std::fs::read("gba_bios.bin").expect("BIOS file not found");
	let rom = std::fs::read(path).expect("ROM file not found");

	let mut cpu = cpu::Cpu::new();
	let sdl_context = sdl2::init().unwrap();
	let ppu = ppu::Ppu::new(&sdl_context.video().unwrap());
	let mut mem = Memory::new(&bios, &rom);
	let mut sys = System {
		pressed_keys: HashSet::new(),
		ppu,
		interrupts: Interrupts::default(),
		timer: Timers::default(),
		audio: Audio::default(),
	};

	let mut time = Instant::now();
	let mut instruction_last_print = Instant::now();
	let mut instruction_interval = 0u64;
	let mut event_pump = sdl_context.event_pump().unwrap();
	let mut booting = true;
	'running: loop {
		let mut cycle = cpu.cycle;
		if cpu.step(&mut mem, &mut sys) {
			instruction_interval += 1;
		}
		cpu.cycle = cpu.cycle.wrapping_add(mem.dma(&mut sys));
		loop {
			if cycle == cpu.cycle {
				break;
			}
			sys.ppu.step(&mut sys.interrupts);
			sys.timer.step(&mut sys.interrupts, cycle);
			if cycle.is_multiple_of(FRAME_CYCLES) {
				let now = Instant::now();
				let elapsed = now.duration_since(instruction_last_print);
				if elapsed >= Duration::from_secs(1) {
					let mips = instruction_interval as f64 / elapsed.as_secs_f64() / 1_000_000.0;
					println!("{mips:.2} MIPS");
					instruction_interval = 0;
					instruction_last_print = now;
				}

				for event in event_pump.poll_iter() {
					match event {
						Event::Quit { .. } => {
							break 'running;
						}
						Event::KeyDown { keycode: Some(keycode), .. } => {
							sys.pressed_keys.insert(keycode);
							sys.interrupts.interrupt(12);
						}
						Event::KeyUp { keycode: Some(keycode), .. } => {
							sys.pressed_keys.remove(&keycode);
						}
						_ => {}
					}
				}
				let sleep_until = time.add(Duration::new(0, SYNC_CYCLE_DURATION));
				let duration = sleep_until.duration_since(Instant::now());
				if cpu.pc() > MAX_BIOS_ADDRESS {
					booting = false;
				}
				if !booting {
					// std::thread::sleep(duration);
				}
				time = Instant::now();
			}
			cycle = cycle.wrapping_add(1);
		}
	}
}
