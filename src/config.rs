extern crate serde;
extern crate toml;

use std::fs::File;
use std::io::Read;

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
			Config::default()
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpConfig {
	#[serde(default)]
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
	#[serde(default)]
	pub width: u16,
	#[serde(default)]
	pub height: u16,
	#[serde(default)]
	pub framerate: u8,
	#[serde(default)]
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
