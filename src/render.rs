pub mod chunks;
pub mod error;

use std::{cell::RefCell, io::Write};

use flate2::write::DeflateDecoder;
use iced::widget::canvas;

use chunks::Chunk;
use error::Error;
use nom::{
    bits::complete::take as take_bits,
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
                decoder.write_all(data.into())?;
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

trait Render {
    fn draw_rectangle(&mut self, top_left: iced::Point, size: iced::Size, color: iced::Color);
}

impl Render for &mut canvas::Frame {
    fn draw_rectangle(&mut self, top_left: iced::Point, size: iced::Size, color: iced::Color) {
        self.fill_rectangle(top_left, size, color);
    }
}

struct Renderer<'data, R> {
    renderable: RefCell<R>,
    dimensions: iced::Size,
    color_type: ColorType,
    bits_per_pixel: usize,
    interlace: Interlace,
    palette: Option<Colors<'data>>,
    scanline: usize,
    next_scanline: Vec<u8>,
    prev_scanline: Vec<u8>,
}

impl<'data, R: Render> Renderer<'data, R> {
    fn new(
        renderable: R,
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
            renderable: RefCell::new(renderable),
            dimensions: iced::Size::new(width as f32, height as f32),
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

    fn filter(&mut self) -> Result<(), Error> {
        let (_, filter_type) = one_byte_as::<FilterType>(&self.next_scanline)?;
        self.next_scanline[0] = 0;
        let bytes_per_pixel = self.bits_per_pixel.div_ceil(8);
        match filter_type {
            FilterType::None => {}
            FilterType::Sub => {
                for i in 1 + bytes_per_pixel..self.next_scanline.len() {
                    self.next_scanline[i] =
                        self.next_scanline[i].wrapping_add(self.next_scanline[i - bytes_per_pixel]);
                }
            }
            FilterType::Up => {
                if !self.prev_scanline.is_empty() {
                    for i in 1..self.next_scanline.len() {
                        self.next_scanline[i] =
                            self.next_scanline[i].wrapping_add(self.prev_scanline[i]);
                    }
                }
            }
            FilterType::Average => {
                for i in 1..self.next_scanline.len() {
                    let left = *self.next_scanline.get(i - bytes_per_pixel).unwrap_or(&0) as u16;
                    let up = *self.prev_scanline.get(i).unwrap_or(&0) as u16;
                    self.next_scanline[i] =
                        self.next_scanline[i].wrapping_add(((left + up) / 2) as u8);
                }
            }
            FilterType::Paeth => {
                for i in 1..self.next_scanline.len() {
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

        Ok(())
    }

    fn draw_pixel(&self, renderable: &mut R, row: usize, column: usize, color: iced::Color) {
        renderable.draw_rectangle(
            iced::Point::new(row as f32, column as f32),
            [2.0, 2.0].into(),
            color,
        );
    }

    fn render(&mut self) -> Result<(), Error> {
        let mut renderable = self.renderable.borrow_mut();

        let from_two_bytes = |first: u8, second: u8| {
            (((first as u16) << 8) + second as u16) as f32 / u16::MAX as f32
        };

        if self.bits_per_pixel < 8 {
            let input = (self.next_scanline.as_slice(), 0);
            let mut iter = iterator(input, take_bits::<_, u8, _, _>(self.bits_per_pixel));

            match self.color_type {
                ColorType::GrayScale => {
                    let max_grayscale = 2f32.powi(self.bits_per_pixel as i32);
                    for (i, bits) in (&mut iter).enumerate() {
                        let grayscale = bits as f32 / max_grayscale;
                        let color = iced::Color::from_rgb(grayscale, grayscale, grayscale);
                        self.draw_pixel(&mut renderable, self.scanline, i, color);
                    }
                }

                ColorType::Palette => {
                    if let Some(palette) = self.palette.as_ref() {
                        for (i, bits) in (&mut iter).enumerate() {
                            let color = palette.get(bits as usize);
                            self.draw_pixel(&mut renderable, self.scanline, i, color);
                        }
                    }
                }

                _ => unreachable!("already checked in Renderer::new"),
            }

            iter.finish()?;
        } else {
            let input = self.next_scanline.as_slice();
            let bytes_per_pixel = self.bits_per_pixel / 8;
            let mut iter = iterator(input, take(bytes_per_pixel));

            match self.color_type {
                ColorType::GrayScale => {
                    for (i, bytes) in (&mut iter).enumerate() {
                        let grayscale = if bytes_per_pixel == 1 {
                            bytes[0] as f32 / u8::MAX as f32
                        } else {
                            from_two_bytes(bytes[0], bytes[1])
                        };
                        let color = iced::Color::from_rgb(grayscale, grayscale, grayscale);
                        self.draw_pixel(&mut renderable, self.scanline, i, color);
                    }
                }

                ColorType::Rgb => match bytes_per_pixel {
                    3 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let &[red, green, blue] = bytes else {
                                unreachable!("must be 3 bytes per pixel");
                            };
                            let color = iced::Color::from_rgb8(red, green, blue);
                            self.draw_pixel(&mut renderable, self.scanline, i, color);
                        }
                    }

                    6 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let red = from_two_bytes(bytes[0], bytes[1]);
                            let green = from_two_bytes(bytes[3], bytes[4]);
                            let blue = from_two_bytes(bytes[4], bytes[5]);
                            let color = iced::Color::from_rgb(red, green, blue);
                            self.draw_pixel(&mut renderable, self.scanline, i, color);
                        }
                    }

                    _ => unreachable!("already checked in Renderer::new"),
                },
                ColorType::Palette => {
                    if let Some(palette) = self.palette.as_ref() {
                        for (i, byte) in (&mut iter).enumerate() {
                            let color = palette.get(byte[0] as usize);
                            self.draw_pixel(&mut renderable, self.scanline, i, color);
                        }
                    }
                }
                ColorType::GrayScaleAlpha => {
                    for (i, bytes) in (&mut iter).enumerate() {
                        let (grayscale, alpha) = if bytes_per_pixel == 1 {
                            (
                                bytes[0] as f32 / u8::MAX as f32,
                                bytes[1] as f32 / u8::MAX as f32,
                            )
                        } else {
                            (
                                from_two_bytes(bytes[0], bytes[1]),
                                from_two_bytes(bytes[2], bytes[3]),
                            )
                        };
                        let color = iced::Color::from_rgba(grayscale, grayscale, grayscale, alpha);
                        self.draw_pixel(&mut renderable, self.scanline, i, color);
                    }
                }
                ColorType::RgbAlpha => match bytes_per_pixel {
                    4 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let &[red, green, blue, alpha] = bytes else {
                                unreachable!("must be 4 bytes per pixel");
                            };
                            let alpha = alpha as f32 / i8::MAX as f32;
                            let color = iced::Color::from_rgba8(red, green, blue, alpha);
                            self.draw_pixel(&mut renderable, self.scanline, i, color);
                        }
                    }

                    8 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let red = from_two_bytes(bytes[0], bytes[1]);
                            let green = from_two_bytes(bytes[3], bytes[4]);
                            let blue = from_two_bytes(bytes[4], bytes[5]);
                            let alpha = from_two_bytes(bytes[6], bytes[7]);
                            let color = iced::Color::from_rgba(red, green, blue, alpha);
                            self.draw_pixel(&mut renderable, self.scanline, i, color);
                        }
                    }

                    _ => unreachable!("already checked in Renderer::new"),
                },
            }

            iter.finish()?;
        }

        Ok(())
    }
}

impl<R: Render> Write for Renderer<'_, R> {
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
            self.filter().map_err(std::io::Error::other)?;
            self.render().map_err(std::io::Error::other)?;
            std::mem::swap(&mut self.next_scanline, &mut self.prev_scanline);
            self.next_scanline.clear();
            self.scanline += 1;
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
    use nom::{combinator::iterator, sequence::preceded};
    use std::error::Error;

    const PNG: &[u8] = include_bytes!("../assets/xkcd.png");

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

    mod mock_test {
        use crate::render::Render;
        use std::io::Write;
        use termcolor::*;

        struct MockRender(StandardStream);

        impl Default for MockRender {
            fn default() -> Self {
                Self(StandardStream::stdout(ColorChoice::Always))
            }
        }

        impl Render for MockRender {
            fn draw_rectangle(
                &mut self,
                _top_left: iced::Point,
                _size: iced::Size,
                color: iced::Color,
            ) {
                let [red, green, blue, ..] = color.into_rgba8();
                self.0
                    .set_color(ColorSpec::new().set_bg(Some(Color::Rgb(red, green, blue))))
                    .unwrap();
                write!(self.0, "_").unwrap();
            }
        }
    }
}
