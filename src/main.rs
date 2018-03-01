#[macro_use]
extern crate lazy_static;

use std::env;
use std::io::{self, Read, Write};
use std::process::Command;
use std::sync::{Mutex, RwLock};

struct Singleton<T>(T);

lazy_static! {
	static ref H264_NAL_UNITS: Mutex<Vec<Vec<u8>>> = Mutex::new(vec![]);
	static ref H264_NAL_PIC_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref H264_NAL_SEQ_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref MP4_SERVE_BUFFER: RwLock<Vec<u8>> = RwLock::new(vec![]);
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
		1 => H264_NAL_UNITS.lock().unwrap().push(frame),
		5 => {
			let child = if let Ok(child) = Command::new("ffmpeg")
				.args(vec!["-loglevel", "quiet"]) // Don't output any crap that is not the actual output of the stream
				.args(vec!["-i", "-"]) // Bind to STDIN
				.args(vec!["-c:v", "copy"]) // Copy video only
				.args(vec!["-f", "mp4"]) // Output as mp4
				.arg("pipe:1") // Output to stdout
				.spawn() { child } else { return; };

			let mut ffmpeg = if let Some(out) = child.stdin { out } else { return; };

			{
				let mut units = H264_NAL_UNITS.lock().unwrap();

				let _ = ffmpeg.write(&H264_NAL_PIC_PARAM.read().unwrap().0[..]);
				let _ = ffmpeg.write(&H264_NAL_SEQ_PARAM.read().unwrap().0[..]);

				for i in 0..units.len() {
					let _ = ffmpeg.write(&units[i][..]);
				}
				units.clear();

				units.push(frame);
			}

			{
				if let Some(mut output) = child.stdout {
					let _ = output.read_to_end(&mut MP4_SERVE_BUFFER.write().unwrap());
				}
			}
		}
		7 => H264_NAL_PIC_PARAM.write().unwrap().0 = frame,
		8 => H264_NAL_SEQ_PARAM.write().unwrap().0 = frame,
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
