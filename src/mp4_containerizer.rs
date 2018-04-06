use CONFIG;
use ptr::*;
use std::io::{self, Read, Write};
use std::process::{self, ChildStdout, Command};

pub fn translate<P: ParentTerminator, R: Read>(mut reader: ParentedRead<P, R>) -> Vec<u8> {
	let raspivid_cfg = &CONFIG.read().unwrap().raspivid;
	let mut child = if let Ok(child) = Command::new("ffmpeg")
		.arg("-re")
		.args(vec!["-i", "-"]) // Stay on for a 2 hours instead of quickly exiting
		.args(vec!["-c:v", "copy"]) // don't reencode
		.args(vec!["-f", "mp4"]) // mp4 format
		.args(vec!["-movflags", "faststart+empty_moov"]) // fragment the mp4
		.arg("pipe:1")
		.stdin(process::Stdio::piped())
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::null())
		.spawn() { child } else { panic!("Failed to spawn ffmpeg process."); };

	if let Some(mut child_stdin) = child.stdin.take() {
		let mut stdin_buf = vec![];
		let _ = reader.read_to_end(&mut stdin_buf);

		let _ = child_stdin.write_all(stdin_buf.as_mut_slice());
	} else {
		panic!("failed to attach stdin of ffmpeg")
	};

	let mut child_stdout = if let Some(inner) = child.stdout.take() {
		inner
	} else {
		panic!("failed to attach stdout of ffmpeg")
	};

	let mut stdout_buf = vec![];

	let _ = child_stdout.read_to_end(&mut stdout_buf);

	stdout_buf
}

#[cfg(test)]
#[test]
pub fn ffmpeg_test() {
	use std::fs::{File, OpenOptions, remove_file};
	struct Void;

	impl ParentTerminator for Void {
		fn terminate(&mut self) {}
	}

	let buf = translate(ParentedRead::new(Void, File::open("test.h264").unwrap()));

	let _ = remove_file("test.mp4");

	let mut file = OpenOptions::new()
		.read(false)
		.write(true)
		.create_new(true)
		.open("test.mp4")
		.unwrap();

	let _ = file.write_all(&buf[..]);
}
