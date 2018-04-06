use std::io::Read;
use std::ops::{Deref, DerefMut};

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

impl<P: ParentTerminator, R: Read> DerefMut for ParentedRead<P, R> {
	fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
		&mut self.stream
	}
}
