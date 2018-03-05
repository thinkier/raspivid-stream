use std::io::Read;
use streams::FFMpeg;

pub fn split_stream<F, R: Read>(input_stream: &mut R, ffmpeg: &mut FFMpeg, pic_param: &mut Vec<u8>, seq_param: &mut Vec<u8>, new_unit_handle: F) where F: Fn(Vec<u8>, &mut FFMpeg, &mut Vec<u8>, &mut Vec<u8>) {
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
						new_unit_handle(nal_unit, ffmpeg, pic_param, seq_param);
						nal_unit = vec![0; null_pads];
					}
				}
				nulls = 0;
			}
		}

		nal_unit.extend_from_slice(&buffer[begin..8192]);
	}
}

pub fn get_unit_type(frame: &Vec<u8>) -> u8 {
	let buffer = &frame[0..5];

	0b00011111 & if buffer[2] == 0x00 {
		buffer[4]
	} else {
		buffer[3]
	}
}
