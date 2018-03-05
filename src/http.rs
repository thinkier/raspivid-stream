extern crate iron;

use self::iron::{headers, status};
use self::iron::prelude::*;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::thread;
use std::time::Duration;
use super::{CONFIG, STREAM_FILE_COUNTER, STREAM_TMP_DIR};

pub fn init_iron() {
	let _ = thread::Builder::new().name("iron serv".to_string()).spawn(|| {
		let addr: SocketAddr = CONFIG.read().unwrap().http.bind_addr.parse().expect("invalid bind address");
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
						document.body.style.backgroundColor = 0xFF0000;
						newStreamer.onplay = function() {
							document.body.style.backgroundColor = 0x00FF00;
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
					current_counter == 0 || current_counter < code && code - current_counter <= 2
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
		let _ = iron.http(addr).unwrap_or_else(|err| panic!("failed to start iron: {:?}", err));
	});
}

fn redir_to_newest_mp4() -> Response {
	let mut response = Response::with(status::TemporaryRedirect);
	response.headers.set(headers::Location(format!("/{}", STREAM_FILE_COUNTER.read().unwrap().0)));

	response
}
