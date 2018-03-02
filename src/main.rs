#![feature(fs_read_write)]
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate env_logger;
extern crate iron;
extern crate time;

use iron::{headers, status};
use iron::prelude::*;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::mem::swap;
use std::ops::Drop;
use std::process::{self, Child, Command};
use std::sync::RwLock;
use std::thread;
use time::Duration;

const STREAM_TMP_DIR: &'static str = "/tmp/raspivid-stream";
const FRAMERATE: usize = 20;

struct Singleton<T>(T);

lazy_static! {
	// All H264 stuff can be moved into a reference passed around with new frame events
	static ref H264_NAL_PIC_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref H264_NAL_SEQ_PARAM: RwLock<Singleton<Vec<u8>>> = RwLock::new(Singleton(vec![]));
	static ref STREAM_FILE_LOCK: RwLock<bool> = RwLock::new(false);
}

fn main() {
	env_logger::init();

	if let Err(_) = fs::read_dir(STREAM_TMP_DIR) {
		fs::create_dir(STREAM_TMP_DIR).expect("Failed to create temporary directory.");
	}

	thread::spawn(|| {
		let mut iron = Iron::new(|req: &mut Request| Ok(match req.url.path().pop().unwrap_or("index.html") {
			"stream.mp4" => {
				let _ = STREAM_FILE_LOCK.read();
				if let Ok(mut file) = File::open(&format!("{}/stream.mp4", STREAM_TMP_DIR)) {
					let mut buffer = vec![];
					let _ = file.read_to_end(&mut buffer);
					let mut response = Response::with((status::Ok, buffer));
					response.headers.set(headers::Expires(headers::HttpDate(time::now() + Duration::seconds(1))));

					response
				} else { Response::new() }
			}
			_ => {
				// Serve the script with html
				let mut response = Response::with((status::Ok, "<!doctype html><html><body><video id='stream' width='1280' height='720' src='/stream.mp4' autoplay/>\
	<script type='text/javascript'>var stream = document.getElementById('stream');stream.removeAttribute('controls');stream.addEventListener('ended',reloadStream,false);function reloadStream(e){window.location.reload(false);}</script></body></html>"));
				response.headers.set(headers::ContentType::html());

				info!("[{}]: normal looper", req.remote_addr);
				response
			}
		}));
		iron.threads = 2usize;
		iron.http("0.0.0.0:3128").unwrap();
	});

	let mut ffmpeg = FFMpeg::spawn();
	loop {
		let mut child = if let Ok(child) = Command::new("raspivid")
			.args(vec!["-o", "-"]) // Output to STDOUT
			.args(vec!["-w", "1280"]) // Width
			.args(vec!["-h", "720"]) // Height
			.args(vec!["-fps", &format!("{}", FRAMERATE)]) // Framerate
			.args(vec!["-t", "7200000"]) // Stay on for a 2 hours instead of quickly exiting
			.args(vec!["-rot", "90"]) // Rotate 90 degrees as the device is sitting sideways.
			.args(vec!["-a", "4"]) // Output time
			.args(vec!["-a", &format!("Device: {} | %F %X %z", env::var("HOSTNAME").unwrap_or("unknown".to_string()))]) // Supplementary argument hmm rn it requires an additional `export` command
			.stdin(process::Stdio::null())
			.stdout(process::Stdio::piped())
			.spawn() { child } else { panic!("Failed to spawn raspivid process."); };

		let mut child_stdout = child.stdout.take().unwrap_or_else(|| {
			let _ = child.kill();
			panic!("Failed to attach to raspivid's STDOUT")
		});

		while let Ok(None) = child.try_wait() {
			split_stream(&mut child_stdout, &mut ffmpeg);
		}
	}
}

fn split_stream<R: Read>(input_stream: &mut R, mut ffmpeg: &mut FFMpeg) {
	let mut nulls: usize = 0;
	let mut nal_unit: Vec<u8> = vec![];
	let mut buffer = [0u8; 8192];

	while let Ok(_) = input_stream.read_exact(&mut buffer) {
		let mut begin = 0;
		for i in 0..8192 {
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
						new_unit_event(nal_unit, &mut ffmpeg);
						nal_unit = vec![0; null_pads];
					}
				}
				nulls = 0;
			}
		}

		nal_unit.extend_from_slice(&buffer[begin..8192]);
	}
}

fn new_unit_event(mut frame: Vec<u8>, ffmpeg: &mut FFMpeg) {
	let unit_type = get_unit_type(&frame);
	loop {
		match unit_type {
			5 => {
				// Minimum 256kb h264 buffer
				if ffmpeg.nal_units > FRAMERATE * 3 {
					let mut newinst = FFMpeg::spawn();
					swap(ffmpeg, &mut newinst);
				}

				let _ = STREAM_FILE_LOCK.write();
				let path = format!("{}/stream.mp4", STREAM_TMP_DIR);
				let _ = fs::remove_file(&path);
				let _ = fs::rename(&format!("{}/stream_replace.mp4", STREAM_TMP_DIR), &path);
			}
			7 => H264_NAL_PIC_PARAM.write().unwrap().0 = frame.clone(),
			8 => H264_NAL_SEQ_PARAM.write().unwrap().0 = frame.clone(),
			_ => {}
		}
		break;
	}
	ffmpeg.write(&mut frame);
}

fn get_unit_type(frame: &Vec<u8>) -> u8 {
	let buffer = &frame[0..5];

	0b00011111 & if buffer[2] == 0x00 {
		buffer[4]
	} else {
		buffer[3]
	}
}

struct FFMpeg {
	pub process: Child,
	pub nal_units: usize,
}

impl FFMpeg {
	pub fn spawn() -> Self {
		let process = Command::new("ffmpeg")
			.args(vec!["-loglevel", "quiet"]) // Don't output any crap that is not the actual output of the stream
			.args(vec!["-i", "-"]) // Bind to STDIN
			.args(vec!["-c:v", "copy"]) // Copy video only
			.args(vec!["-f", "mp4"]) // Output as mp4
			.arg(&format!("{}/stream_replace.mp4", STREAM_TMP_DIR))
			.stdin(process::Stdio::piped())
			.stdout(process::Stdio::null())
			.spawn()
			.expect("Failed to spawn ffmpeg process.");

		let mut ffmpeg = FFMpeg { process, nal_units: 0 };

		for param in vec![H264_NAL_PIC_PARAM.read().unwrap(), H264_NAL_SEQ_PARAM.read().unwrap()] {
			if param.0.len() > 0 {
				ffmpeg.write(&mut param.0.clone());
			}
		}

		return ffmpeg;
	}

	pub fn write(&mut self, buf: &mut Vec<u8>) {
		let mut stdin = self.process.stdin.take().expect("Failed to open STDIN of FFMpeg");

		let _ = stdin.write_all(&mut buf[..]);
		swap(&mut self.process.stdin, &mut Some(stdin)); // Inject it back into self.process

		self.nal_units += 1;
	}
}

impl Drop for FFMpeg {
	fn drop(&mut self) {
		{ let _ = self.process.stdin.take(); }
		let _ = self.process.wait();
	}
}