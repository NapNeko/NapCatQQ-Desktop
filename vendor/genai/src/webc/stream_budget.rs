use crate::{Error, Result};

/// Counts a frame before UTF-8 decoding or concatenation, including across raw body chunks.
pub(crate) struct FrameBudget {
	limit: usize,
	pending: usize,
	matched: usize,
	delimiter: &'static [u8],
	sse: bool,
	previous_cr: bool,
}

impl FrameBudget {
	pub(crate) fn new(limit: usize, delimiter: &'static str, sse: bool) -> Self {
		Self {
			limit,
			pending: 0,
			matched: 0,
			delimiter: delimiter.as_bytes(),
			sse,
			previous_cr: false,
		}
	}

	pub(crate) fn observe(&mut self, bytes: &[u8]) -> Result<()> {
		if self.delimiter.is_empty() {
			return Err(Error::StreamGuardRejected {
				reason: "empty stream delimiter",
			});
		}
		for &byte in bytes {
			let normalized = if self.sse {
				let was_cr = self.previous_cr;
				self.previous_cr = byte == b'\r';
				if was_cr && byte == b'\n' {
					continue;
				}
				if byte == b'\r' { b'\n' } else { byte }
			} else {
				byte
			};
			self.pending = self.pending.checked_add(1).ok_or(Error::StreamGuardRejected {
				reason: "frame byte limit exceeded",
			})?;
			self.matched = if normalized == self.delimiter[self.matched] {
				self.matched + 1
			} else {
				usize::from(normalized == self.delimiter[0])
			};
			if self.matched == self.delimiter.len() {
				self.pending = 0;
				self.matched = 0;
			} else if self.pending.saturating_sub(self.matched) > self.limit {
				return Err(Error::StreamGuardRejected {
					reason: "frame byte limit exceeded",
				});
			}
		}
		Ok(())
	}

	pub(crate) fn finish(&self) -> Result<()> {
		if self.pending > self.limit {
			return Err(Error::StreamGuardRejected {
				reason: "frame byte limit exceeded",
			});
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rejects_frame_growth_across_body_chunks() {
		let mut budget = FrameBudget::new(5, "\n\n", true);
		assert!(budget.observe(b"123").is_ok());
		assert!(budget.observe(b"456").is_err());
	}

	#[test]
	fn resets_on_sse_crlf_and_cr_boundaries_across_chunks() {
		let mut budget = FrameBudget::new(5, "\n\n", true);
		assert!(budget.observe(b"12345\r").is_ok());
		assert!(budget.observe(b"\n\r").is_ok());
		assert!(budget.observe(b"\n12345\r\r12345\n\n").is_ok());
		assert!(budget.finish().is_ok());
	}

	#[test]
	fn rejects_oversized_unterminated_tail() {
		let mut budget = FrameBudget::new(5, "\n\n", true);
		assert!(budget.observe(b"12345\n").is_ok());
		assert!(budget.finish().is_err());
	}

	#[test]
	fn accepts_many_small_frames_in_one_large_chunk() {
		let mut budget = FrameBudget::new(3, "\n\n", true);
		assert!(budget.observe(b"123\n\n123\n\n123\n\n").is_ok());
	}
}
