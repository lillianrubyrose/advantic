use crate::{Bits, System};
use sdl2::keyboard::Keycode;

#[derive(Default, Clone)]
struct DmaChannel {
	pub index: usize,
	pub source: u32,
	pub target: u32,
	pub count: u32,
	pub handled: bool,
}

impl DmaChannel {
	fn register_offset(&self) -> usize {
		0x2c + 3 * self.index
	}

	fn load_count(&mut self, control: u32) {
		let count = control & 0xffff;
		self.count = if self.index == 3 {
			if count == 0 { 0x10000 } else { count }
		} else {
			if count == 0 { 0x4000 } else { count & 0x3ff }
		};
	}
}

pub const MAX_BIOS_ADDRESS: u32 = 0x0000_4000;

#[derive(Clone, Copy, PartialEq)]
pub enum MemoryRegion {
	Bios,
	Rom,
	Ram,
	Vram,
	Palette,
	IO,
	Oam,
}

#[derive(Clone, Copy)]
pub struct ParsedAddress {
	pub region: MemoryRegion,
	pub address: u32,
	pub cycles: u32,
}

pub struct Memory {
	ram: Vec<u32>,
	rom: Vec<u32>,
	bios: Vec<u32>,
	io: [u32; 0x100],
	dma: [DmaChannel; 4],
}

fn update_u32(current: u32, new: u32, address: u32, size: u8) -> u32 {
	if size == 4 {
		new
	} else {
		let mask = (1 << (8 * u32::from(size))) - 1;
		let offset = (address & 3) * 8;
		current & !(mask << offset) | (new & mask) << offset
	}
}

fn write_u32(target: &mut [u32], address: u32, value: u32, size: u8) {
	let index = (address / 4) as usize;
	target[index] = update_u32(target[index], value, address, size);
}

const KEYCODES: &[&[Keycode]] = &[
	&[Keycode::SPACE, Keycode::Z],
	&[Keycode::LCTRL, Keycode::X],
	&[Keycode::TAB],
	&[Keycode::RETURN],
	&[Keycode::RIGHT, Keycode::D],
	&[Keycode::LEFT, Keycode::A],
	&[Keycode::UP, Keycode::W],
	&[Keycode::DOWN, Keycode::S],
	&[Keycode::E],
	&[Keycode::Q],
];

fn to_u32_vec(data: &[u8]) -> Vec<u32> {
	data.chunks(4).map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap())).collect()
}

impl Memory {
	#[must_use]
	pub fn new(bios: &[u8], rom: &[u8]) -> Self {
		Self {
			bios: to_u32_vec(bios),
			rom: to_u32_vec(rom),
			ram: vec![0; 0x14500],
			io: [0; _],
			dma: std::array::from_fn(|index| DmaChannel { index, ..Default::default() }),
		}
	}

	#[must_use]
	pub fn parse_address(&self, address: u32, size: u8, sequential: bool) -> Option<ParsedAddress> {
		fn get_cycles(waitcnt: u32, shift: u32, size: u8) -> u32 {
			let cycles = match (waitcnt >> shift) & 0b11 {
				0 => 4,
				1 => 3,
				2 => 2,
				3 => 8,
				_ => unreachable!(),
			};
			1 + cycles + if size == 4 { 3 } else { 0 }
		}
		let waitcnt = self.io[0x81];
		Some(match address >> 24 {
			0x0 if address < MAX_BIOS_ADDRESS => ParsedAddress { region: MemoryRegion::Bios, address, cycles: 1 },
			0x2 => ParsedAddress {
				region: MemoryRegion::Ram,
				address: address % 0x40000,
				cycles: if size == 4 { 6 } else { 3 },
			},
			0x3 => ParsedAddress { region: MemoryRegion::Ram, address: (address % 0x8000) + 0x40000, cycles: 1 },
			0x4 if address < 0x0400_03ff => {
				ParsedAddress { region: MemoryRegion::IO, address: address - 0x0400_0000, cycles: 1 }
			}
			0x5 => ParsedAddress {
				region: MemoryRegion::Palette,
				address: address % 0x400,
				cycles: if size == 4 { 2 } else { 1 },
			},
			0x6 => {
				let mut address = address % 0x20000;
				if address >= 0x18000 {
					address -= 0x8000;
				}
				ParsedAddress { region: MemoryRegion::Vram, address, cycles: if size == 4 { 2 } else { 1 } }
			}
			0x7 => ParsedAddress { region: MemoryRegion::Oam, address: address % 0x400, cycles: 1 },
			0x8..0xa => ParsedAddress {
				region: MemoryRegion::Rom,
				address: address - 0x0800_0000,
				cycles: if sequential { 1 + u32::from(!waitcnt.bit(4)) } else { get_cycles(waitcnt, 2, size) },
			},
			0xa..0xc => ParsedAddress {
				region: MemoryRegion::Rom,
				address: address - 0x0A00_0000,
				cycles: if sequential { 1 + u32::from(!waitcnt.bit(7)) * 3 } else { get_cycles(waitcnt, 5, size) },
			},
			0xc..0xe => ParsedAddress {
				region: MemoryRegion::Rom,
				address: address - 0x0C00_0000,
				cycles: if sequential { 1 + u32::from(!waitcnt.bit(10)) * 7 } else { get_cycles(waitcnt, 8, size) },
			},
			0xe => ParsedAddress {
				region: MemoryRegion::Ram,
				address: (address % 0x10000) + 0x48000,
				cycles: get_cycles(waitcnt, 0, size),
			},
			_ => return None,
		})
	}

	#[must_use]
	pub fn read(&self, sys: &System, parsed: ParsedAddress, size: u8) -> u32 {
		let aligned_address = parsed.address & !3;
		let index = (parsed.address / 4) as usize;
		let value = match parsed.region {
			MemoryRegion::Bios => self.bios[index],
			MemoryRegion::Ram => self.ram[index],
			MemoryRegion::Palette => sys.ppu.palettes[index],
			MemoryRegion::Vram => {
				let index = aligned_address as usize;
				u32::from_le_bytes(sys.ppu.vram[index..=index + 3].try_into().unwrap())
			}
			MemoryRegion::Rom => {
				if index >= self.rom.len() {
					return 0;
				}
				self.rom[index]
			}
			MemoryRegion::Oam => sys.ppu.oam[index],
			MemoryRegion::IO => match aligned_address {
				0..0x60 => sys.ppu.read_register(parsed.address),
				0x100..0x110 => sys.timer.read_register(parsed.address),
				0x130 => {
					let mut value = 0;
					for (bit, keys) in KEYCODES.iter().enumerate() {
						value.set_bit(bit as u32, !keys.iter().any(|key| sys.pressed_keys.contains(key)));
					}
					value
				}
				0x200 => sys.interrupts.control,
				0x208 => u32::from(sys.interrupts.enabled),
				_ => self.io[index],
			},
		};
		value.rotate_right(8 * (parsed.address % 4)) & 1u32.unbounded_shl(u32::from(size) * 8).wrapping_sub(1)
	}

	pub fn write(&mut self, sys: &mut System, parsed: ParsedAddress, value: u32, size: u8) {
		let aligned_address = parsed.address & !3;
		match parsed.region {
			MemoryRegion::Ram => write_u32(&mut self.ram, parsed.address, value, size),
			MemoryRegion::Palette => write_u32(&mut sys.ppu.palettes, parsed.address, value, size),
			MemoryRegion::Oam => write_u32(&mut sys.ppu.oam, parsed.address, value, size),
			MemoryRegion::IO => {
				match aligned_address {
					0..0x60 => sys.ppu.write_register(
						aligned_address,
						update_u32(sys.ppu.read_register(aligned_address), value, parsed.address, size),
					),
					0x80..=0x88 => {
						write_u32(&mut sys.audio.control, parsed.address - 0x80, value, size);
					}
					0xb8 | 0xc4 | 0xd0 | 0xdc => {
						let index = ((aligned_address - 0xb8) / 0xc) as usize;
						let channel = &mut self.dma[index];
						let registers = &self.io[channel.register_offset()..];
						let value = update_u32(registers[2], value, parsed.address, size);

						if value.bit(31) && !registers[2].bit(31) {
							channel.source = registers[0] & 0xfff_ffff;
							channel.target = registers[1] & 0xfff_ffff;
							channel.load_count(value);
							channel.handled = false;
						}
					}
					0x100..0x110 => sys.timer.write_register(
						aligned_address,
						update_u32(sys.timer.read_register(aligned_address), value, parsed.address, size),
					),
					0x200 => sys.interrupts.write_register(
						aligned_address,
						update_u32(sys.interrupts.control & 0xffff, value, parsed.address, size),
					),
					0x208 => sys.interrupts.write_register(
						aligned_address,
						update_u32(u32::from(sys.interrupts.enabled), value, parsed.address, size),
					),
					_ => {}
				}
				write_u32(&mut self.io, parsed.address, value, size);
			}
			MemoryRegion::Vram => {
				for i in 0..size {
					sys.ppu.vram[parsed.address as usize + usize::from(i)] = (value >> (i * 8)) as u8;
				}
			}
			MemoryRegion::Bios => eprintln!("Attempt to write to BIOS {aligned_address:08x}"),
			MemoryRegion::Rom => eprintln!("Attempt to write to ROM {aligned_address:08x}"),
		}
	}

	pub fn dma(&mut self, sys: &mut System) -> u32 {
		fn increment_sign(control: u32) -> i32 {
			match (control) & 0b11 {
				0 | 3 => 1,
				1 => -1,
				2 => 0,
				_ => unreachable!(),
			}
		}

		let mut cycles = 0;
		for i in 0..self.dma.len() {
			let channel = &mut self.dma[i];
			let offset = channel.register_offset();
			let mut control = self.io[offset + 2];

			if !control.bit(31) {
				continue;
			}
			let cond = match (control >> 28) & 0b11 {
				1 => sys.ppu.line == 160,
				2 => sys.ppu.hblank,
				3 if channel.index == 3 => sys.ppu.hblank,
				3 => sys.audio.dma[channel.index - 1].dma_enabled,
				_ => true,
			};
			if !cond {
				channel.handled = false;
				continue;
			}
			if channel.handled {
				continue;
			}

			let mut channel = channel.clone();
			channel.handled = true;
			assert!(!control.bit(27));
			let size = if control.bit(26) { 4 } else { 2 };
			let old_target = channel.target;
			let source_increment = i32::from(size) * increment_sign(control >> 23);
			let target_increment = i32::from(size) * increment_sign(control >> 21);
			channel.source &= !u32::from(size - 1);
			channel.target &= !u32::from(size - 1);
			let mut sequential = false;
			for _ in 0..channel.count {
				let source = self.parse_address(channel.source, size, sequential);
				let target = self.parse_address(channel.target, size, sequential);
				if let Some(source) = source
					&& let Some(target) = target
				{
					cycles += source.cycles + target.cycles;
					let value = self.read(sys, source, size);
					self.write(sys, target, value, size);
				}
				channel.source = channel.source.wrapping_add_signed(source_increment);
				channel.target = channel.target.wrapping_add_signed(target_increment);
				sequential = true;
			}
			cycles += 2;
			if control.bit(25) {
				channel.load_count(control);
				if (control >> 21) & 0b11 == 0b11 {
					channel.target = old_target;
				}
			} else {
				control.set_bit(31, false);
				self.io[offset + 2] = control;
			}
			if control.bit(14) {
				sys.interrupts.interrupt((channel.index + 8) as u32);
			}
			self.dma[i] = channel;
		}
		cycles
	}
}
