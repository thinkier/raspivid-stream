use CONFIG;
use ptr::*;
use std::process::{self, Child, ChildStdout, Command};
use std::thread;
use std::time::Duration;

pub fn get_stream() -> ParentedRead<ChildProcessWrapper, ChildStdout> {
	let mut child = {
		let raspivid_cfg = &CONFIG.read().unwrap().raspivid;
		if let Ok(child) = Command::new("raspivid")
			.args(vec!["-o", "-"]) // Output to STDOUT
			.args(vec!["-t", "7200000"]) // Stay on for a 2 hours instead of quickly exiting
			.args(vec!["-rot", &format!("{}", raspivid_cfg.rotation)]) // Rotation for orientation problems
			.args(vec!["-w", &format!("{}", raspivid_cfg.width)]) // Width
			.args(vec!["-h", &format!("{}", raspivid_cfg.height)]) // Height
			.args(vec!["-fps", &format!("{}", raspivid_cfg.framerate)]) // Framerate
//			.args(vec!["-a", "4"]) // Output time
//			.args(vec!["-a", &format!("Device: {} | %F %X %Z", env::var("HOSTNAME").unwrap_or("unknown".to_string()))]) // Supplementary argument hmm rn it requires an additional `export` command
			.stdin(process::Stdio::null())
			.stdout(process::Stdio::piped())
			.spawn() { child } else { panic!("Failed to spawn raspivid process."); }
	};
	info!("Loading raspivid instance...");

	thread::sleep(Duration::from_secs(1));
	if let Ok(Some(code)) = child.try_wait() {
		if let Some(code) = code.code() {
			error!("Raspivid exited with code: {}", code);
		} else {
			error!("Raspivid has been killed externally.");
		}
		process::exit(1);
	}

	info!("Loaded raspivid instance.");
	let child_stdout = child.stdout.take().unwrap_or_else(|| {
		let _ = child.kill();
		panic!("Failed to attach to raspivid's STDOUT")
	});

	ParentedRead::new(ChildProcessWrapper(child), child_stdout)
}

pub struct ChildProcessWrapper(Child);

impl ParentTerminator for ChildProcessWrapper {
	fn terminate(&mut self) {
		let _ = self.0.kill();
		let _ = self.0.wait(); // reap zombie
	}
}
