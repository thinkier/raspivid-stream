#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate iron;

use iron::{headers, status};
use iron::prelude::*;
// use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::mem::swap;
use std::ops::Drop;
use std::process::{self, Child, Command};
use std::sync::RwLock;
use std::thread;
use std::time::Duration;

const STREAM_TMP_DIR: &'static str = "/tmp/raspivid-stream";
const FRAMERATE: usize = 20;

struct Singleton<T>(T);

lazy_static! {
	static ref STREAM_FILE_COUNTER: RwLock<Singleton<usize>> = RwLock::new(Singleton(0));
}

fn main() {
	env_logger::init();
	clean_tmp_dir();

	let _ = thread::Builder::new().name("iron serv".to_string()).spawn(|| {
		thread::sleep(Duration::from_secs(5));
		info!("Starting iron and serving video over HTTP.");

		let mut iron = Iron::new(|req: &mut Request| Ok(match req.url.path().pop().unwrap_or("") {
			"" => {
				// Serve the script with html
				let num = STREAM_FILE_COUNTER.read().unwrap().0;
				let mut response = Response::with((status::Ok, format!("<!doctype html><html><body><center><video id='streamer{}' autoplay src='/{}'/ style='width:100%;height:auto;'></video></center><script type='text/javascript'>
				register(document.getElementById('streamer{}'), {});
				{}</script></body></html>", num, num, num, num + 1, "
				function register(streamer, num){
					streamer.onended = function() {
						var newStreamer = document.createElement('video');
						streamer.parentNode.appendChild(newStreamer);
						newStreamer.id = 'streamer' + num;
						newStreamer.autoplay = true;
						newStreamer.src = '/' + num;
						newStreamer.style = 'width:100%;height:auto;display:none;';
						newStreamer.onplay = function() {
							streamer.parentNode.removeChild(streamer);
							newStreamer.style.display = 'inline';
							register(newStreamer, num + 1);
						};
					}
				}
				")));
				response.headers.set(headers::ContentType::html());

				response
			}
			"current_code" => {
				Response::with((status::Ok, format!("{}", STREAM_FILE_COUNTER.read().unwrap().0)))
			}
			code => {
				let code: usize = if let Ok(code) = code.parse() { code } else {
					return Ok(redir_to_newest_mp4());
				};

				while {
					let current_counter = STREAM_FILE_COUNTER.read().unwrap().0;
					current_counter < code && code - current_counter <= 2
				} {
					thread::sleep(Duration::from_millis(150));
				}

				let path = format!("{}/{}", STREAM_TMP_DIR, code);
				if let Ok(mut file) = File::open(&path) {
					let mut buffer = vec![];
					let _ = file.read_to_end(&mut buffer);
					let mut response = Response::with((status::Ok, buffer));
					response.headers.set(headers::CacheControl(vec![headers::CacheDirective::Public, headers::CacheDirective::MaxAge(60)]));
					response.headers.set(headers::ContentType("video/mp4".parse().unwrap()));

					response
				} else {
					redir_to_newest_mp4()
				}
			}
		}));
		iron.threads = 8usize;
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
//			.args(vec!["-a", "4"]) // Output time
//			.args(vec!["-a", &format!("Device: {} | %F %X %Z", env::var("HOSTNAME").unwrap_or("unknown".to_string()))]) // Supplementary argument hmm rn it requires an additional `export` command
			.stdin(process::Stdio::null())
			.stdout(process::Stdio::piped())
			.spawn() { child } else { panic!("Failed to spawn raspivid process."); };
		info!("Loaded raspivid instance.");

		let mut child_stdout = child.stdout.take().unwrap_or_else(|| {
			let _ = child.kill();
			panic!("Failed to attach to raspivid's STDOUT")
		});

		let mut pic_param = vec![];
		let mut seq_param = vec![];

		while let Ok(None) = child.try_wait() {
			split_stream(&mut child_stdout, &mut ffmpeg, &mut pic_param, &mut seq_param);
		}
	}
}

fn split_stream<R: Read>(input_stream: &mut R, ffmpeg: &mut FFMpeg, pic_param: &mut Vec<u8>, seq_param: &mut Vec<u8>) {
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
						new_unit_event(nal_unit, ffmpeg, pic_param, seq_param);
						nal_unit = vec![0; null_pads];
					}
				}
				nulls = 0;
			}
		}

		nal_unit.extend_from_slice(&buffer[begin..8192]);
	}
}

fn new_unit_event(mut frame: Vec<u8>, ffmpeg: &mut FFMpeg, pic_param: &mut Vec<u8>, seq_param: &mut Vec<u8>) {
	let unit_type = get_unit_type(&frame);
	loop {
		match unit_type {
			5 => {
				// Minimum 4 seconds buffer
				if ffmpeg.is_saturated() {
					let mut handle = FFMpeg::spawn();

					handle.write(pic_param);
					handle.write(seq_param);

					swap(ffmpeg, &mut handle);
					let _ = thread::Builder::new().name("ffmpeg handle".to_string()).spawn(move || {
						let counter = {
							let mut ptr = STREAM_FILE_COUNTER.write().unwrap();
							ptr.0 += 1;
							ptr.0
						};
						handle.process();

						let path = format!("{}/{}", STREAM_TMP_DIR, counter);
						let _ = fs::rename(&format!("{}/stream_replace.mp4", STREAM_TMP_DIR), &path);
						info!("Outputted new mp4 file at {}", path);

						if counter >= 4 {
							let _ = fs::remove_file(&format!("{}/{}", STREAM_TMP_DIR, counter - 4)); // Delete old
						}
					});
				}
			}
			7 => pic_param.extend(&frame[..]),
			8 => seq_param.extend(&frame[..]),
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

fn redir_to_newest_mp4() -> Response {
	let mut response = Response::with(status::TemporaryRedirect);
	response.headers.set(headers::Location(format!("/{}", STREAM_FILE_COUNTER.read().unwrap().0)));

	response
}

trait StreamProcessor {
	fn spawn() -> Self;
	fn write(&mut self, buf: &mut Vec<u8>);
	fn process(&mut self);
	fn is_saturated(&self) -> bool;
}

/// Literally does nothing but be a phantom class
struct Null;

impl StreamProcessor for Null {
	fn spawn() -> Self {
		Null {}
	}

	fn write(&mut self, _buf: &mut Vec<u8>) {}

	fn process(&mut self) {}

	fn is_saturated(&self) -> bool {
		true
	}
}

/// Handle the stream and convert to mp4 for FFMpeg
struct FFMpeg {
	process: Child,
	nal_units: usize,
}

impl StreamProcessor for FFMpeg {
	fn spawn() -> Self {
		let process = Command::new("ffmpeg")
			.args(vec!["-loglevel", "quiet"]) // Don't output any crap that is not the actual output of the stream
			.args(vec!["-i", "-"]) // Bind to STDIN
			.args(vec!["-c:v", "copy"]) // Copy video only
			.args(vec!["-f", "mp4"]) // Output as mp4
			.args(vec!["-movflags", "faststart"]) // Mov data at the start for faster loading
			.arg(&format!("{}/stream_replace.mp4", STREAM_TMP_DIR))
			.stdin(process::Stdio::piped())
			.stdout(process::Stdio::null())
			.spawn()
			.expect("Failed to spawn ffmpeg process.");

		info!("Loaded ffmpeg instance.");
		FFMpeg { process, nal_units: 0 }
	}

	fn write(&mut self, buf: &mut Vec<u8>) {
		let mut stdin = self.process.stdin.take().expect("Failed to open STDIN of FFMpeg");

		let _ = stdin.write_all(&mut buf[..]);

		{
			let mut opt_stdin = Some(stdin);
			swap(&mut self.process.stdin, &mut opt_stdin); // Inject it back into self.process
		}

		self.nal_units += 1;
	}

	fn process(&mut self) {
		{ let _ = self.process.stdin.take(); }
		let _ = self.process.wait();
	}

	fn is_saturated(&self) -> bool {
		self.nal_units > FRAMERATE * 4
	}
}

impl Drop for FFMpeg {
	fn drop(&mut self) {
		self.process();
	}
}

fn clean_tmp_dir() {
	let _ = fs::remove_dir_all(STREAM_TMP_DIR);
	let _ = fs::create_dir(STREAM_TMP_DIR);
}
