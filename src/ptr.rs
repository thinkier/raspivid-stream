use std::io::Read;
use std::ops::Deref;

pub trait ParentTerminator {
	fn terminate(&mut self);
}

pub struct ParentedRead<P: ParentTerminator, R: Read> {
	parent: P,
	stream: R,
}

impl<P: ParentTerminator, R: Read> ParentedRead<P, R> {
	pub fn new(parent: P, stream: R) -> ParentedRead<P, R> {
		ParentedRead { parent, stream }
	}
}

impl<P: ParentTerminator, R: Read> Drop for ParentedRead<P, R> {
	fn drop(&mut self) {
		self.parent.terminate();
	}
}

impl<P: ParentTerminator, R: Read> Deref for ParentedRead<P, R> {
	type Target = R;

	fn deref(&self) -> &<Self as Deref>::Target {
		&self.stream
	}
}
