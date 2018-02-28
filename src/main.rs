#[macro_use]
extern crate lazy_static;

use std::io::{self, Read};
use std::sync::{Mutex, RwLock};

struct Singleton<T>(T);

lazy_static! {
	static ref UNITS: Mutex<Vec<Vec<u8>>> = Mutex::new(vec![]);
	static ref PIC_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref SEQ_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
}

fn main() {
	let mut nulls: usize = 0;
	let mut nal_unit: Vec<u8> = vec![];
	let mut buffer: [u8; 8192] = [0u8; 8192];

	while let Ok(count) = io::stdin().read(&mut buffer) {
		if count <= 0 { break; }
		let mut begin = 0;
		for i in 0..count {
			if buffer[i] == 0x00 {
				nulls += 1;
			} else {
				if (nulls == 2 || nulls == 3) && buffer[i] == 0x01 {
					let mut null_pads = if i >= nulls {
						let unwritten_count = i - nulls;
						nal_unit.extend_from_slice(&buffer[begin..unwritten_count]);
						begin = unwritten_count;
						0
					} else {
						for _ in 0..nulls - i {
							let _ = nal_unit.pop();
						}
						nulls - i
					};
					if nal_unit.len() > 0 {
						new_unit_event(nal_unit);
						nal_unit = vec![0; null_pads];
					}
				}
				nulls = 0;
			}
		}

		nal_unit.extend_from_slice(&buffer[begin..count]);
	}
}

fn new_unit_event(frame: Vec<u8>) {
	match get_unit_type(&frame) {
		1 => UNITS.lock().unwrap().push(frame),
		5 => {
			// todo Flush
			UNITS.lock().unwrap().push(frame);
		}
		7 => PIC_PARAM.write().unwrap().0 = frame,
		8 => SEQ_PARAM.write().unwrap().0 = frame,
		_ => return // Ignore lol
	}
}

fn get_unit_type(frame: &Vec<u8>) -> u8 {
	let buffer = &frame[0..5];

	0b00011111 & if buffer[2] == 0x00 {
		buffer[4]
	} else {
		buffer[3]
	}
}
