#[derive(Debug)]
pub struct Buffer {
    start: u64,
    end: u64, // exclusive
    data: Vec<u8>,
}

impl Buffer {
    pub fn new(start: u64, end: u64) -> Self {
        assert!(start < end, "Buffer must represent a valid range.");

        Self {
            start,
            end,
            data: vec![0; (end - start) as usize],
        }
    }

    pub fn from_slice(start: u64, buf: &[u8]) -> Self {
        Self {
            start,
            end: start + buf.len() as u64,
            data: buf.to_vec(),
        }
    }

    // Consumes both buffers, merging them
    pub fn merge(self, other: Self) -> Self {
        assert!(self.overlaps(&other), "buffers do not overlap");

        // Create new buffer object
        let start = self.start.min(other.start);
        let end = self.end.max(other.end);
        let mut new = Self::new(start, end);

        // Copy data from self over
        new.data[(self.start - start) as usize..(self.end - start) as usize]
            .copy_from_slice(&self.data);
        // Copy data from other over
        new.data[(other.start - start) as usize..(other.end - start) as usize]
            .copy_from_slice(&other.data);

        new
    }

    // Check if there is any intersection between the ranges [self.start, self.end) and [other.start, other.end)
    // Also if they are touching end to end
    pub fn overlaps(&self, other: &Buffer) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    // Checks if the requested read exists fully within the buffer
    fn contains_range(&self, offset: u64, length: u64) -> bool {
        self.start <= offset && offset + length <= self.end
    }

    // Returns a reference to the requested range if it exists in the buffer.
    pub fn get_range(&self, offset: u64, length: u64) -> Option<&[u8]> {
        if !self.contains_range(offset, length) {
            return None;
        }

        let start = offset - self.start;
        let end = start + length;
        Some(&self.data[start as usize..end as usize])
    }

    // Returns the range of data this buffer represents
    pub fn range(&self) -> (u64, u64) {
        (self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::Buffer;

    #[test]
    fn test_overlaps() {
        let buf1 = Buffer::new(0, 10);
        let buf2 = Buffer::new(5, 15);
        let buf3 = Buffer::new(11, 20);
        let buf4 = Buffer::new(10, 20);
        assert!(buf1.overlaps(&buf2));
        assert!(!buf1.overlaps(&buf3));
        assert!(buf3.overlaps(&buf3));
        assert!(buf1.overlaps(&buf4));
    }

    #[test]
    fn test_merge() {
        let mut buf1 = Buffer::new(0, 10);
        buf1.data.copy_from_slice(&(0..10).collect::<Vec<_>>());

        let mut buf2 = Buffer::new(5, 15);
        buf2.data.copy_from_slice(&(5..15).collect::<Vec<_>>());

        let new_buf1 = buf1.merge(buf2);
        assert_eq!(new_buf1.start, 0);
        assert_eq!(new_buf1.end, 15);
        assert_eq!(new_buf1.data, (0..15).collect::<Vec<_>>())
    }

    #[test]
    fn test_get_range() {
        let mut buf1 = Buffer::new(10, 20);
        buf1.data.copy_from_slice(&(10..20).collect::<Vec<_>>());

        let range1 = buf1.get_range(11, 4);
        assert_eq!(range1, Some(vec![11, 12, 13, 14].as_slice()));
        let range2 = buf1.get_range(8, 4);
        assert_eq!(range2, None);
        let range3 = buf1.get_range(8, 40);
        assert_eq!(range3, None);
        let range4 = buf1.get_range(10, 10);
        assert_eq!(range4, Some((10..20).collect::<Vec<_>>().as_slice()));
    }
}
