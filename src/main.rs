#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate env_logger;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate rocket;
#[macro_use]
extern crate serde_derive;

use config::StreamConfig;
use rocket::config::{Config, Environment};
use std::fs;
use std::sync::RwLock;

mod config;
mod http;
mod ptr;
mod h264_input;
mod mp4_containerizer;

const STREAM_TMP_DIR: &'static str = "/tmp/raspivid-stream";

lazy_static! {
	static ref CONFIG: RwLock<StreamConfig> = RwLock::new(StreamConfig::load());
}

fn reinit_tmp_dir() {
	let _ = fs::remove_dir_all(STREAM_TMP_DIR);
	let _ = fs::create_dir(STREAM_TMP_DIR);
}

fn start_rocket() {
	let config = Config::build(Environment::Production)
		.address("0.0.0.0")
		.port(80)
		.finalize()
		.unwrap();
	rocket::custom(config, true)
		.mount("/", routes![http::index, http::stream])
		.launch();
}

fn main() {
	env_logger::init();
	{ info!("Config: {:?}", CONFIG.read().unwrap()); }
	reinit_tmp_dir();

	start_rocket();
}
