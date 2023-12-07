pub mod chunks;
pub mod error;

use std::io::Write;

use flate2::write::DeflateDecoder;
use iced::widget::canvas;

use chunks::Chunk;
use error::Error;
use nom::{
    bytes::complete::{tag, take},
    combinator::{iterator, map, map_res, recognize},
    sequence::tuple,
    IResult,
};

use self::chunks::{BitDepth, ColorType, Colors, Interlace};

pub fn render(frame: &mut canvas::Frame, data: &[u8]) -> Result<(), Error> {
    let (data, _) = header(data)?;
    let (_data, chunk) = chunks::chunk(data)?;

    let Chunk::Ihdr {
        width,
        height,
        bit_depth,
        color_type,
        interlace,
    } = chunk
    else {
        return Err(Error::MissingCritical("IHDR"));
    };

    let mut decoder = DeflateDecoder::new(Renderer::new(
        frame,
        width as usize,
        height as usize,
        bit_depth,
        color_type,
        interlace,
    )?);
    for chunk in &mut iterator(data, chunks::chunk) {
        match chunk {
            Chunk::Ihdr { .. } => {
                return Err(Error::DuplicateIhdr);
            }
            Chunk::Plte(colors) => {
                decoder.get_mut().set_palette(colors);
            }
            Chunk::Idat(data) => {
                decoder.write_all(data)?;
            }
            Chunk::Iend => {
                return Ok(());
            }
            Chunk::Unknown => {}
        }
    }

    Err(Error::MissingCritical("IEND"))
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
enum FilterType {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
}

impl TryFrom<u8> for FilterType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Sub),
            2 => Ok(Self::Up),
            3 => Ok(Self::Average),
            4 => Ok(Self::Paeth),
            _ => Err(Error::InvalidFilterType(value)),
        }
    }
}

fn one_byte_as<Into: TryFrom<u8, Error = Error>>(input: &[u8]) -> IResult<&[u8], Into, Error> {
    map_res(map(take(1usize), |input: &[u8]| input[0]), |b| {
        Into::try_from(b)
    })(input)
}

pub fn header(input: &[u8]) -> IResult<&[u8], &[u8], Error> {
    recognize(tuple((
        tag(&[0x89]),
        tag(b"PNG"),
        tag(&[0x0D, 0x0A, 0x1A, 0x0A]),
    )))(input)
}

struct Renderer<'frame, 'data> {
    frame: &'frame mut canvas::Frame,
    dimensions: iced::Size,
    bit_depth: BitDepth,
    color_type: ColorType,
    bits_per_pixel: usize,
    interlace: Interlace,
    palette: Option<Colors<'data>>,
    scanline: usize,
    next_scanline: Vec<u8>,
    prev_scanline: Vec<u8>,
}

impl<'frame, 'data> Renderer<'frame, 'data> {
    fn new(
        frame: &'frame mut canvas::Frame,
        width: usize,
        height: usize,
        bit_depth: BitDepth,
        color_type: ColorType,
        interlace: Interlace,
    ) -> Result<Self, Error> {
        let bits_per_pixel = {
            use BitDepth as BD;
            use ColorType as CT;

            match (bit_depth, color_type) {
                (BD::One, CT::GrayScale | CT::Palette) => 1,
                (BD::Two, CT::GrayScale | CT::Palette) => 2,
                (BD::Four, CT::GrayScale | CT::Palette) => 4,
                (BD::Eight, CT::GrayScale | CT::Palette) => 8,
                (BD::Eight, CT::GrayScaleAlpha) | (BD::Sixteen, CT::GrayScale) => 16,
                (BD::Eight, CT::Rgb) => 24,
                (BD::Eight, CT::RgbAlpha) | (BD::Sixteen, CT::GrayScaleAlpha) => 32,
                (BD::Sixteen, CT::Rgb) => 48,
                (BD::Sixteen, CT::RgbAlpha) => 64,
                _ => {
                    return Err(Error::InvalidBitColorCombo(
                        bit_depth as u8,
                        color_type as u8,
                    ))
                }
            }
        };
        let scanline_len = (width * bits_per_pixel).div_ceil(8) + 1;

        Ok(Self {
            frame,
            dimensions: iced::Size::new(width as f32, height as f32),
            bit_depth,
            color_type,
            bits_per_pixel,
            interlace,
            palette: None,
            scanline: 0,
            next_scanline: Vec::with_capacity(scanline_len),
            prev_scanline: Vec::with_capacity(scanline_len),
        })
    }

    fn set_palette(&mut self, colors: Colors<'data>) {
        self.palette = Some(colors);
    }

    fn render_scanline(&mut self) -> Result<(), Error> {
        let (_, filter_type) = one_byte_as::<FilterType>(&self.next_scanline)?;
        let bytes_per_pixel = self.bits_per_pixel.div_ceil(8);
        match filter_type {
            FilterType::None => {}
            FilterType::Sub => {
                for i in bytes_per_pixel..self.next_scanline.len() {
                    self.next_scanline[i] =
                        self.next_scanline[i].wrapping_add(self.next_scanline[i - bytes_per_pixel]);
                }
            }
            FilterType::Up => {
                if !self.prev_scanline.is_empty() {
                    for i in 0..self.next_scanline.len() {
                        self.next_scanline[i] =
                            self.next_scanline[i].wrapping_add(self.prev_scanline[i]);
                    }
                }
            }
            FilterType::Average => {
                for i in 0..self.next_scanline.len() {
                    let left = *self.next_scanline.get(i - bytes_per_pixel).unwrap_or(&0) as u16;
                    let up = *self.prev_scanline.get(i).unwrap_or(&0) as u16;
                    self.next_scanline[i] =
                        self.next_scanline[i].wrapping_add(((left + up) / 2) as u8);
                }
            }
            FilterType::Paeth => {
                for i in 0..self.next_scanline.len() {
                    let left = *self.next_scanline.get(i - bytes_per_pixel).unwrap_or(&0) as u16;
                    let up = *self.prev_scanline.get(i).unwrap_or(&0) as u16;
                    let up_left = *self.prev_scanline.get(i - bytes_per_pixel).unwrap_or(&0) as u16;
                    let p = left + up - up_left;
                    let (p_left, p_up, p_up_left) =
                        (p.abs_diff(left), p.abs_diff(up), p.abs_diff(up_left));
                    let paeth = if p_left <= p_up && p_left <= p_up_left {
                        left
                    } else if p_up <= p_up_left {
                        up
                    } else {
                        up_left
                    } as u8;
                    self.next_scanline[i] = self.next_scanline[i].wrapping_add(paeth);
                }
            }
        }

        /*
        let mut iter = iterator(scanline, take(1usize));

        for pixel in &mut iter {
            //
        }
        iter.finish()?;
        */

        std::mem::swap(&mut self.next_scanline, &mut self.prev_scanline);
        self.next_scanline.clear();
        Ok(())
    }
}

impl Write for Renderer<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut remainder = buf;
        loop {
            let scanline_spare_len = self.next_scanline.capacity() - self.next_scanline.len();
            if remainder.len() < scanline_spare_len {
                self.next_scanline.extend_from_slice(remainder);
                break Ok(buf.len());
            }
            self.next_scanline
                .extend_from_slice(&remainder[..scanline_spare_len]);
            remainder = &remainder[scanline_spare_len..];
            self.render_scanline().map_err(std::io::Error::other)?;
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::chunks::*;
    use super::*;
    use nom::{combinator::iterator, sequence::preceded, HexDisplay};
    use std::{error::Error, fmt::Write};

    const PNG: &[u8] = include_bytes!("../assets/xkcd.png");

    #[repr(transparent)]
    struct BytesPrinter([u8]);

    impl std::fmt::Debug for BytesPrinter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_char('\n')?;
            let len = self.0.len().min(64);
            f.write_str(&self.0[..len].to_hex(8))
        }
    }

    impl std::cmp::PartialEq for BytesPrinter {
        fn eq(&self, other: &Self) -> bool {
            &self.0 == &other.0
        }
    }

    impl From<&[u8]> for &BytesPrinter {
        fn from(value: &[u8]) -> Self {
            unsafe { std::mem::transmute(value) }
        }
    }

    macro_rules! assert_bytes {
        ($left:expr, $right:expr $(,)?) => {
            assert_eq!(
                <&BytesPrinter>::from($left as &[u8]),
                <&BytesPrinter>::from($right as &[u8]),
            )
        };
    }

    #[test]
    fn parse_header() -> Result<(), Box<dyn Error>> {
        let (_, result) = header(PNG)?;
        assert_bytes!(result, b"\x89PNG\r\n\x1A\x0A");
        Ok(())
    }

    #[test]
    fn parse_ihdr() -> Result<(), Box<dyn Error>> {
        let (_, chunk) = preceded(header, chunk)(PNG)?;
        assert_eq!(
            chunk,
            Chunk::Ihdr {
                width: 293,
                height: 165,
                bit_depth: BitDepth::Eight,
                color_type: ColorType::Rgb,
                interlace: Interlace::None,
            }
        );
        Ok(())
    }

    #[test]
    fn iend_is_last() -> Result<(), Box<dyn Error>> {
        let (input, _) = header(PNG)?;
        let mut iter = iterator(input, chunk);
        let last_chunk = iter.last();
        let (input, _) = iter.finish()?;
        assert!(input.is_empty());
        assert_eq!(last_chunk, Some(Chunk::Iend));
        Ok(())
    }
}
