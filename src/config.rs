extern crate serde;
extern crate toml;

use std::fs::*;
use std::io::{Read, Write};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
	#[serde(default)]
	pub http: HttpConfig,
	#[serde(default)]
	pub raspivid: RaspividConfig,
}

impl Default for Config {
	fn default() -> Self {
		Config {
			http: HttpConfig::default(),
			raspivid: RaspividConfig::default(),
		}
	}
}

impl Config {
	pub fn load() -> Config {
		if let Ok(mut file) = File::open("config.toml") {
			let mut str = String::new();
			file.read_to_string(&mut str).unwrap_or_else(|err| panic!("failed to read from config.toml: {:?}", err));
			toml::from_str(&str).unwrap_or_else(|err| panic!("failed to deserialize config.toml: {:?}", err))
		} else {
			let config = Config::default();

			let config_str = toml::to_string(&config).unwrap();
			OpenOptions::new()
				.read(false)
				.write(true)
				.create(true)
				.open("config.toml")
				.unwrap_or_else(|err| panic!("failed to open config file for writing: {:?}", err))
				.write_all(config_str.as_bytes())
				.unwrap_or_else(|err| panic!("failed to write standard config to config.toml: {:?}", err));

			config
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpConfig {
	#[serde(default = "default_bind_addr")]
	pub bind_addr: String
}

impl Default for HttpConfig {
	fn default() -> Self {
		HttpConfig {
			bind_addr: "0.0.0.0:3128".to_string(),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RaspividConfig {
	#[serde(default = "default_width")]
	pub width: u16,
	#[serde(default = "default_height")]
	pub height: u16,
	#[serde(default = "default_framerate")]
	pub framerate: u8,
	#[serde(default = "default_rotation")]
	pub rotation: u16,
}

impl Default for RaspividConfig {
	fn default() -> Self {
		RaspividConfig {
			width: 1280,
			height: 720,
			framerate: 20,
			rotation: 0,
		}
	}
}

fn default_bind_addr() -> String {
	HttpConfig::default().bind_addr
}

fn default_width() -> u16 {
	RaspividConfig::default().width
}

fn default_height() -> u16 {
	RaspividConfig::default().height
}

fn default_framerate() -> u8 {
	RaspividConfig::default().framerate
}

fn default_rotation() -> u16 {
	RaspividConfig::default().rotation
}
