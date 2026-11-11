//! The picture a pixel effect shows a text or an image with (`PixelBitmap` of Rocket Show): its pixels in frames (an
//! animation has more than one), each pixel a brightness (a text, a byte each) or red, green and blue (an image, three
//! bytes each), row by row from the top left.
//!
//! # The bytes of a bitmap
//!
//! The project holds a bitmap as JSON, its frames in base 64. The compiler decodes it once (`encode_bitmap` of the
//! `std` feature) into the bytes below, which [`Bitmap::parse`] reads without copying. Every number is little-endian
//! and starts at a multiple of 4 bytes.
//!
//! | offset            | type      | value                                                                         |
//! | ----------------- | --------- | ----------------------------------------------------------------------------- |
//! | 0                 | `i32`     | `width` in pixels (a bitmap of no width or height shows nothing)              |
//! | 4                 | `i32`     | `height` in pixels                                                            |
//! | 8                 | `u32`     | `channels`, the bytes of each pixel: 1 (a brightness) or 3 (red, green, blue) |
//! | 12                | `u32`     | `F`, how many frames there are                                                |
//! | 16                | `u32`     | `L`, how many letters start somewhere                                         |
//! | 20                | `u32`     | `B`, how many bytes are kept of each frame                                    |
//! | 24                | `F × f32` | when each frame ends, in ms from the start of the first one                   |
//! | 24 + 4F           | `L × i32` | the column each letter of a text starts at, ascending                         |
//! | 24 + 4F + 4L      | `F × S`   | the frames, each `B` bytes and zeros up to `S`, `B` rounded up to 4           |
//!
//! The length is `24 + 4F + 4L + F × S` exactly.
//!
//! - **The ends of the frames** replace the `durations` of the project: each frame lasts as long as the project says,
//!   100 ms where it says nothing or nothing above 0, and its end is the sum up to it (worked out in double
//!   precision, then rounded). They never fall and are finite. The last one is how long the animation takes.
//! - **The starts of the letters** are sorted, the ones the project has as `null` left out.
//! - **A frame** holds the decoded bytes of its pixels: byte `(y × width + x) × channels + channel`. A frame which
//!   is shorter in the project (or can not be decoded: no bytes at all) is filled up with zeros, which is what a
//!   pixel beyond the bytes reads as anyway. `B` is the length of the longest frame, but not more than
//!   `width × height × channels`.

/// The bytes before the ends of the frames.
pub const HEADER_LEN: usize = 24;

/// A view over the bytes of a bitmap (see the layout of the module). Reading it can not go beyond the bytes: the
/// lengths are checked once by [`Bitmap::parse`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bitmap<'a> {
    width: i32,
    height: i32,
    channels: u8,
    frame_count: usize,
    letter_count: usize,
    frame_len: usize,
    stride: usize,
    ends: &'a [u8],
    letters: &'a [u8],
    frames: &'a [u8],
}

fn bytes_at(data: &[u8], offset: usize) -> Option<[u8; 4]> {
    data.get(offset..offset.checked_add(4)?)?.try_into().ok()
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    bytes_at(data, offset).map(u32::from_le_bytes)
}

fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
    bytes_at(data, offset).map(i32::from_le_bytes)
}

fn f32_at(data: &[u8], offset: usize) -> Option<f32> {
    bytes_at(data, offset).map(f32::from_le_bytes)
}

impl<'a> Bitmap<'a> {
    /// The bitmap of its bytes. None if they are not a bitmap: too short or too long for what the header says, other
    /// channels than 1 or 3, ends of frames which fall or are not finite, starts of letters which are not sorted.
    pub fn parse(data: &'a [u8]) -> Option<Bitmap<'a>> {
        let width = i32_at(data, 0)?;
        let height = i32_at(data, 4)?;
        let channels = u32_at(data, 8)?;
        let frame_count = u32_at(data, 12)? as u64;
        let letter_count = u32_at(data, 16)? as u64;
        let frame_len = u32_at(data, 20)? as u64;

        if channels != 1 && channels != 3 {
            return None;
        }

        // (none of these can leave 64 bits: every factor is below 2^32)
        let stride = (frame_len + 3) & !3;
        let ends_start = HEADER_LEN as u64;
        let letters_start = ends_start + frame_count * 4;
        let frames_start = letters_start + letter_count * 4;
        let end = frames_start.checked_add(frame_count.checked_mul(stride)?)?;

        if end != data.len() as u64 {
            return None;
        }

        // (all within the length of the bytes from here, so they fit a usize)
        let ends = data.get(ends_start as usize..letters_start as usize)?;
        let letters = data.get(letters_start as usize..frames_start as usize)?;
        let frames = data.get(frames_start as usize..)?;

        let bitmap = Bitmap {
            width,
            height,
            channels: channels as u8,
            frame_count: frame_count as usize,
            letter_count: letter_count as usize,
            frame_len: frame_len as usize,
            stride: stride as usize,
            ends,
            letters,
            frames,
        };

        let mut last_end = 0.0f32;

        for index in 0..bitmap.frame_count {
            let end = f32_at(ends, index.checked_mul(4)?)?;

            if !end.is_finite() || end < last_end {
                return None;
            }

            last_end = end;
        }

        let mut last_start = i32::MIN;

        for index in 0..bitmap.letter_count {
            let start = i32_at(letters, index.checked_mul(4)?)?;

            if start < last_start {
                return None;
            }

            last_start = start;
        }

        Some(bitmap)
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// The bytes of each pixel: 1 (a brightness) or 3 (red, green and blue).
    pub fn channels(&self) -> i32 {
        self.channels as i32
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn letter_count(&self) -> usize {
        self.letter_count
    }

    /// The bytes of a frame, none for a frame there is not.
    pub fn frame(&self, index: usize) -> &'a [u8] {
        if index >= self.frame_count {
            return &[];
        }

        index
            .checked_mul(self.stride)
            .and_then(|start| self.frames.get(start..start.checked_add(self.frame_len)?))
            .unwrap_or(&[])
    }

    /// A byte of a pixel of a frame: of its brightness (channel 0) or of red, green and blue (0 to 2). 0 outside of
    /// the picture and beyond the bytes there are.
    pub fn value(&self, frame: &[u8], x: i32, y: i32, channel: i32) -> i32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height || channel < 0 {
            return 0;
        }

        // (y * width + x fits 64 bits, with the channels it is checked)
        let pixel = y as i64 * self.width as i64 + x as i64;
        let index = pixel
            .checked_mul(self.channels as i64)
            .and_then(|index| index.checked_add(channel as i64));

        index
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| frame.get(index))
            .map(|byte| *byte as i32)
            .unwrap_or(0)
    }

    /// When a frame ends, in ms from the start of the first one.
    pub fn frame_end(&self, index: usize) -> f32 {
        index
            .checked_mul(4)
            .and_then(|offset| f32_at(self.ends, offset))
            .unwrap_or(0.0)
    }

    /// The frame shown at a moment of the period of the effect (0 to 1): the frames one after the other, each as long
    /// as it lasts.
    pub fn frame_at(&self, moment: f32) -> usize {
        let count = self.frame_count;

        if count <= 1 {
            return 0;
        }

        let at = moment * self.frame_end(count - 1);

        // the first frame which ends after the moment: the ends never fall, so the halves of what is left are tried
        let mut low = 0;
        let mut high = count;

        for _ in 0..usize::BITS {
            if low >= high {
                break;
            }

            let middle = low + (high - low) / 2;

            if at < self.frame_end(middle) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }

        if low >= count {
            count - 1
        } else {
            low
        }
    }

    /// The column a letter starts at.
    pub fn letter_start(&self, index: usize) -> Option<i32> {
        if index >= self.letter_count {
            return None;
        }

        i32_at(self.letters, index.checked_mul(4)?)
    }

    /// How many letters start at a column or before it: the number of the letter the column is in (from 1), for the
    /// colors of the letters.
    pub fn letters_up_to(&self, column: i32) -> i32 {
        // the first letter which starts after the column: the starts are sorted
        let mut low = 0;
        let mut high = self.letter_count;

        for _ in 0..usize::BITS {
            if low >= high {
                break;
            }

            let middle = low + (high - low) / 2;

            match self.letter_start(middle) {
                Some(start) if start <= column => low = middle + 1,
                _ => high = middle,
            }
        }

        // (a bitmap holds fewer than 2^32 letters, more than 2^31 of them count as that many)
        i32::try_from(low).unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the bytes of a bitmap, written by hand
    fn bytes(
        width: i32,
        height: i32,
        channels: u32,
        ends: &[f32],
        letters: &[i32],
        frame_len: u32,
        frames: &[&[u8]],
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(width.to_le_bytes());
        data.extend(height.to_le_bytes());
        data.extend(channels.to_le_bytes());
        data.extend((frames.len() as u32).to_le_bytes());
        data.extend((letters.len() as u32).to_le_bytes());
        data.extend(frame_len.to_le_bytes());

        for end in ends {
            data.extend(end.to_le_bytes());
        }

        for letter in letters {
            data.extend(letter.to_le_bytes());
        }

        for frame in frames {
            let mut frame = frame.to_vec();
            frame.resize(((frame_len as usize) + 3) & !3, 0);
            data.extend(frame);
        }

        data
    }

    #[test]
    fn reads_the_pixels_up_to_its_edges() {
        // the bytes 10, 20, 30, 40 and 50 for 2 x 2 pixels (PixelBitmapTest)
        let data = bytes(2, 2, 1, &[100.0], &[], 5, &[&[10, 20, 30, 40, 50]]);
        let bitmap = Bitmap::parse(&data).unwrap();
        let frame = bitmap.frame(0);

        assert_eq!(data.len(), 24 + 4 + 8);
        assert_eq!(frame, &[10, 20, 30, 40, 50]);
        assert_eq!(bitmap.value(frame, 0, 0, 0), 10);
        assert_eq!(bitmap.value(frame, 1, 0, 0), 20);
        assert_eq!(bitmap.value(frame, 0, 1, 0), 30);
        assert_eq!(bitmap.value(frame, 1, 1, 0), 40);

        // outside of the picture, though the bytes go on
        assert_eq!(bitmap.value(frame, 2, 0, 0), 0);
        assert_eq!(bitmap.value(frame, 0, 2, 0), 0);
        assert_eq!(bitmap.value(frame, -1, 0, 0), 0);
        assert_eq!(bitmap.value(frame, 0, -1, 0), 0);
        assert_eq!(bitmap.value(frame, 0, 0, -1), 0);

        // a frame there is not
        assert_eq!(bitmap.frame(1).len(), 0);
        assert_eq!(bitmap.frame(usize::MAX).len(), 0);
        assert_eq!(bitmap.value(bitmap.frame(1), 0, 0, 0), 0);
    }

    #[test]
    fn reads_the_colors_of_a_pixel_and_nothing_beyond_the_bytes_there_are() {
        // a red pixel, and a green one cut off after its red
        let data = bytes(2, 1, 3, &[100.0], &[], 4, &[&[255, 0, 0, 0]]);
        let bitmap = Bitmap::parse(&data).unwrap();
        let frame = bitmap.frame(0);

        assert_eq!(bitmap.channels(), 3);
        assert_eq!(bitmap.value(frame, 0, 0, 0), 255);
        assert_eq!(bitmap.value(frame, 0, 0, 1), 0);
        assert_eq!(bitmap.value(frame, 1, 0, 0), 0);
        assert_eq!(bitmap.value(frame, 1, 0, 1), 0);
        assert_eq!(bitmap.value(frame, 1, 0, 2), 0);
    }

    #[test]
    fn shows_each_frame_as_long_as_it_lasts() {
        // 100 ms, 300 ms and 100 ms
        let data = bytes(1, 1, 1, &[100.0, 400.0, 500.0], &[], 1, &[&[1], &[2], &[3]]);
        let bitmap = Bitmap::parse(&data).unwrap();

        assert_eq!(bitmap.frame_at(0.0), 0);
        assert_eq!(bitmap.frame_at(0.19), 0);
        assert_eq!(bitmap.frame_at(0.21), 1);
        assert_eq!(bitmap.frame_at(0.79), 1);
        assert_eq!(bitmap.frame_at(0.81), 2);
        // the end of the period shows the last frame
        assert_eq!(bitmap.frame_at(1.0), 2);
        assert_eq!(bitmap.frame_at(f32::NAN), 2);
        assert_eq!(bitmap.frame_at(-1.0), 0);
        assert_eq!(bitmap.frame(2), &[3]);

        // a picture which does not move
        let data = bytes(1, 1, 1, &[100.0], &[], 1, &[&[1]]);
        assert_eq!(Bitmap::parse(&data).unwrap().frame_at(0.7), 0);
        let data = bytes(1, 1, 1, &[], &[], 0, &[]);
        assert_eq!(Bitmap::parse(&data).unwrap().frame_at(0.7), 0);

        // many frames, each 10 ms
        let ends: Vec<f32> = (1..=1000).map(|frame| frame as f32 * 10.0).collect();
        let frames: Vec<&[u8]> = (0..1000).map(|_| &[7u8][..]).collect();
        let data = bytes(1, 1, 1, &ends, &[], 1, &frames);
        let bitmap = Bitmap::parse(&data).unwrap();

        for frame in 0..1000 {
            assert_eq!(bitmap.frame_at((frame as f32 + 0.5) / 1000.0), frame);
        }
    }

    #[test]
    fn counts_the_letters_up_to_a_column() {
        let data = bytes(20, 1, 1, &[], &[0, 6, 12, 18], 0, &[]);
        let bitmap = Bitmap::parse(&data).unwrap();

        assert_eq!(bitmap.letter_count(), 4);
        assert_eq!(bitmap.letter_start(1), Some(6));
        assert_eq!(bitmap.letter_start(4), None);
        assert_eq!(bitmap.letters_up_to(-1), 0);
        assert_eq!(bitmap.letters_up_to(0), 1);
        assert_eq!(bitmap.letters_up_to(5), 1);
        assert_eq!(bitmap.letters_up_to(6), 2);
        assert_eq!(bitmap.letters_up_to(17), 3);
        assert_eq!(bitmap.letters_up_to(18), 4);
        assert_eq!(bitmap.letters_up_to(i32::MAX), 4);
    }

    #[test]
    fn takes_no_bytes_which_are_not_a_bitmap() {
        let data = bytes(
            2,
            2,
            1,
            &[100.0, 200.0],
            &[0, 3],
            5,
            &[&[10, 20, 30, 40, 50], &[1, 2, 3]],
        );

        assert!(Bitmap::parse(&data).is_some());

        // too short, too long
        for length in 0..data.len() {
            assert!(Bitmap::parse(&data[..length]).is_none(), "{length} bytes");
        }

        let mut longer = data.clone();
        longer.push(0);
        assert!(Bitmap::parse(&longer).is_none());

        // other channels
        let mut other = data.clone();
        other[8] = 2;
        assert!(Bitmap::parse(&other).is_none());

        // counts beyond the bytes
        for offset in [12, 16, 20] {
            let mut other = data.clone();
            other[offset + 3] = 0xff;
            assert!(Bitmap::parse(&other).is_none());
        }

        // ends which fall, or are not finite
        assert!(Bitmap::parse(&bytes(1, 1, 1, &[200.0, 100.0], &[], 1, &[&[1], &[2]])).is_none());
        assert!(Bitmap::parse(&bytes(1, 1, 1, &[f32::NAN], &[], 1, &[&[1]])).is_none());
        assert!(Bitmap::parse(&bytes(1, 1, 1, &[f32::INFINITY], &[], 1, &[&[1]])).is_none());
        assert!(Bitmap::parse(&bytes(1, 1, 1, &[-1.0], &[], 1, &[&[1]])).is_none());

        // letters which are not sorted
        assert!(Bitmap::parse(&bytes(1, 1, 1, &[], &[6, 0], 0, &[])).is_none());

        // a huge picture with few bytes is fine: what is beyond them is dark
        let huge = bytes(i32::MAX, i32::MAX, 3, &[100.0], &[], 2, &[&[9, 8]]);
        let bitmap = Bitmap::parse(&huge).unwrap();
        assert_eq!(bitmap.value(bitmap.frame(0), 1, 0, 0), 0);
        assert_eq!(bitmap.value(bitmap.frame(0), i32::MAX - 1, i32::MAX - 1, 2), 0);
        assert_eq!(bitmap.value(bitmap.frame(0), 0, 0, 1), 8);
    }
}
