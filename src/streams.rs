use std::io::Write;
use std::mem::swap;
use std::ops::Drop;
use std::process::{self, Child, Command};
use super::{CONFIG, STREAM_TMP_DIR};

pub trait StreamProcessor {
	fn spawn() -> Self;
	fn write(&mut self, buf: &mut Vec<u8>);
	fn process(&mut self);
	fn is_saturated(&self) -> bool;
}

/// Literally does nothing but be a router class in case footage can be dropped because noone is watching
pub struct Null;

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
pub struct FFMpeg {
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
		self.nal_units > CONFIG.read().unwrap().raspivid.framerate as usize * 4
	}
}

impl Drop for FFMpeg {
	fn drop(&mut self) {
		self.process();
	}
}
