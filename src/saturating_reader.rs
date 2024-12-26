use std::io::{ErrorKind, Read, Seek, SeekFrom};

use crate::buffer::Buffer;

/// A reader which maintains internal buffers of everything it reads.
#[derive(Debug)]
pub struct SaturatingReader<R: Read + Seek> {
    inner: R,
    buffers: Vec<Buffer>,
    cursor_pos: u64,
    bufread_size: usize,
}

impl<R: Read + Seek> SaturatingReader<R> {
    pub fn new(inner: R) -> Self {
        Self::with_capacity(8 * 1024, inner)
    }

    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        Self {
            inner,
            buffers: Vec::new(),
            cursor_pos: 0,
            bufread_size: capacity,
        }
    }

    /// Adds a new buffer to the internally maintained set. Overlapping buffers are merged together
    /// for optimisation.
    fn add_buffer(&mut self, offset: u64, buf: &[u8]) {
        let new_buffer = Buffer::from_slice(offset, buf);

        // Pull out all overlapping buffers
        // todo: replace with https://github.com/rust-lang/rfcs/issues/2140 once it has stabilised
        let buffers = std::mem::take(&mut self.buffers);
        let (overlapping, non_overlapping): (Vec<_>, Vec<_>) =
            buffers.into_iter().partition(|x| x.overlaps(&new_buffer));
        self.buffers = non_overlapping;

        // Merge the overlapping buffers
        let new_buffer = overlapping
            .into_iter()
            .fold(new_buffer, |acc, x| acc.merge(x));

        // Add the new buffer into the collection
        self.buffers.push(new_buffer);
    }

    /// Consumes the reader, returning the inner reader. Note that the cursor position may not be
    /// the same as the outer reader, as it is updated lazily during reads.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Reads from the inner reader, storing it in the buffer. If the requested anount is small,
    /// buffer it up to a minimum.
    fn read_inner(&mut self, at_least: usize) -> std::io::Result<usize> {
        let inner_pos = self.inner.stream_position()?;
        self.inner
            .seek_relative(self.cursor_pos as i64 - inner_pos as i64)?;

        // If not, we fetch the range from the underlying reader
        let mut buf = vec![0; at_least.max(self.bufread_size)];
        let num_bytes_read = self.inner.read(&mut buf)?;

        // Then we store the fetched data in a new buffer internally
        self.add_buffer(self.cursor_pos, &buf[..num_bytes_read]);

        Ok(num_bytes_read)
    }
}

impl<R: Seek + Read> Read for SaturatingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // todo: If requested range partially exists in the internal buffers, try to re-use as much
        // as possible before fetching the rest from inner.

        // First check if the range exists in the maintained buffers
        let existing_buffer = self
            .buffers
            .iter()
            .find_map(|b| b.get_range(self.cursor_pos, buf.len() as u64));

        // Copy out from internal buffer if it exists
        if let Some(existing_buffer) = existing_buffer {
            buf.copy_from_slice(existing_buffer);
            self.cursor_pos += buf.len() as u64;
            return Ok(buf.len());
        }

        // If not, we'll read from the inner reader
        self.read_inner(buf.len())?;

        // Then we re-call the read function with the loaded data.
        self.read(buf)
    }
}

impl<R: Read + Seek> Seek for SaturatingReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            // For start/current, don't seek the underlying reader. It will be handled in read() if
            // needed.
            SeekFrom::Start(p) => self.cursor_pos = p,
            SeekFrom::Current(p) => {
                self.cursor_pos = self.cursor_pos.checked_add_signed(p).ok_or_else(|| {
                    std::io::Error::new(ErrorKind::Other, "Seek position underflowed.")
                })?;
            }
            // Our inner might not support seeking from end, so defer to its implementation
            // instead.
            SeekFrom::End(_) => {
                self.inner.seek(pos)?;
                self.cursor_pos = self.inner.stream_position()?;
            }
        };

        Ok(self.cursor_pos)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Seek, SeekFrom};

    use super::SaturatingReader;

    #[test]
    fn test1() {
        let reader = Cursor::new((0..=255).collect::<Vec<_>>());
        let mut bufreader = SaturatingReader::new(reader);

        let mut buf = [0; 64];
        bufreader.read_exact(&mut buf).unwrap();
        println!("{:?}", bufreader);
        bufreader.read_exact(&mut buf).unwrap();
        println!("{:?}", bufreader);

        let mut buf = vec![];
        let n = bufreader.read_to_end(&mut buf).unwrap();
        println!("{:?} {:?}", n, bufreader);
    }

    #[test]
    fn test2() {
        let reader = Cursor::new((0..=255).collect::<Vec<_>>());
        let mut bufreader = SaturatingReader::new(reader);

        let mut buf = [0; 64];
        bufreader.read_exact(&mut buf).unwrap();

        // Repeated
        bufreader.seek(SeekFrom::Start(0)).unwrap();
        bufreader.read_exact(&mut buf).unwrap();

        assert_eq!(buf.as_slice(), (0..64).collect::<Vec<_>>().as_slice());
        assert_eq!(bufreader.buffers.len(), 1);
        assert_eq!(bufreader.buffers[0].range(), (0, 64));
        println!("{:?}", bufreader.buffers);

        // Partial overlap
        bufreader.seek(SeekFrom::Start(32)).unwrap();
        bufreader.read_exact(&mut buf).unwrap();

        assert_eq!(buf.as_slice(), (32..96).collect::<Vec<_>>().as_slice());
        assert_eq!(bufreader.buffers.len(), 1);
        assert_eq!(bufreader.buffers[0].range(), (0, 96));
        println!("{:?}", bufreader.buffers);

        // Disjoint
        bufreader.seek(SeekFrom::Start(128)).unwrap();
        bufreader.read_exact(&mut buf).unwrap();

        assert_eq!(
            buf.as_slice(),
            (128..128 + 64).collect::<Vec<_>>().as_slice()
        );
        assert_eq!(bufreader.buffers.len(), 2);
        assert_eq!(bufreader.buffers[0].range(), (0, 96));
        assert_eq!(bufreader.buffers[1].range(), (128, 128 + 64));
        println!("{:?}", bufreader.buffers);
    }
}
