use rocket::response::Stream;
use std::io;

#[get("/")]
fn index() -> &'static str {
	include_str!("index.html")
}

#[get("/stream.mp4")]
fn stream() -> Stream<Mock> {
	unimplemented!()
}

struct Mock;

impl io::Read for Mock {
	fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
		unimplemented!()
	}
}