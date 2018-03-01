#[macro_use]
extern crate lazy_static;
extern crate iron;

use iron::prelude::*;
use iron::{headers, status};
use std::env;
use std::io::{Read, Write};
use std::process::{self, Command};
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::Duration;

struct Singleton<T>(T);

lazy_static! {
	static ref H264_NAL_UNITS: Mutex<Vec<Vec<u8>>> = Mutex::new(vec![]);
	static ref H264_NAL_PIC_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref H264_NAL_SEQ_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref MP4_SERVE_BUFFER: RwLock<Vec<u8>> = RwLock::new(vec![]);
}

fn main() {
	thread::spawn(|| {
		Iron::new(|req: &mut Request| Ok(match req.url.path().pop().unwrap_or("index.html") {
			"stream.mp4" => {
				// Serve the MP4 in memory
				let mut response = Response::new();
				response.headers.set(headers::ContentType("video/mp4".parse().unwrap()));

				{
					let mp4_buffer = MP4_SERVE_BUFFER.read().unwrap();
					response.headers.set(headers::ContentLength(mp4_buffer.len() as u64));

					response.body = Some(Box::new(mp4_buffer.clone()));
				}

				response
			}
			_ => {
				// Serve the script with html
				Response::with((status::Ok, "<video id='stream' width='1280' height='720' src='/stream.mp4' autoplay/>\
	<script type='text/javascript'>var stream = document.getElementById('stream');stream.removeAttribute('controls');stream.addEventListener('ended',reloadStream,false);function reloadStream(e){window.location.reload(false);}</script>"))
			}
		})).http("0.0.0.0:3128").unwrap();
	});
	loop {
		let mut child = if let Ok(child) = Command::new("raspivid")
			.args(vec!["-o", "-"]) // Output to STDOUT
			.args(vec!["-w", "1280"]) // Width
			.args(vec!["-h", "720"]) // Height
			.args(vec!["-fps", "30"]) // Framerate
			.args(vec!["-a", "4"]) // Output time
			.args(vec!["-a", &format!("Device: {} | %F %X %z", env::var("HOSTNAME").unwrap_or("unknown".to_string()))]) // Supplementary argument
			.spawn() { child } else { process::exit(1); };

		thread::sleep(Duration::from_millis(100)); // The output stream takes time to initialize :thonk:

		let mut child_stdout = child.stdout.take().unwrap_or_else(|| panic!("Failed to attach to raspivid's STDOUT"));

		while let Ok(None) = child.try_wait() {
			split_stream(&mut child_stdout);
		}
	}
}

fn split_stream<R: Read>(input_stream: &mut R) {
	let mut nulls: usize = 0;
	let mut nal_unit: Vec<u8> = vec![];
	let mut buffer: [u8; 8192] = [0u8; 8192];

	while let Ok(count) = input_stream.read(&mut buffer) {
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
			let mut child = if let Ok(child) = Command::new("ffmpeg")
				.args(vec!["-loglevel", "quiet"]) // Don't output any crap that is not the actual output of the stream
				.args(vec!["-i", "-"]) // Bind to STDIN
				.args(vec!["-c:v", "copy"]) // Copy video only
				.args(vec!["-f", "mp4"]) // Output as mp4
				.arg("pipe:1") // Output to stdout
				.spawn() { child } else { return; };

			let mut ffmpeg = if let Some(out) = child.stdin.take() { out } else { return; };

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
				let mut serve_buffer = MP4_SERVE_BUFFER.write().unwrap();
				serve_buffer.clear();
				serve_buffer.extend(child.wait_with_output().unwrap().stdout)
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
