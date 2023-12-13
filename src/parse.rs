pub mod chunks;
pub mod error;

use std::io::Write;

use flate2::write::ZlibDecoder;
use iced::widget::canvas;

use chunks::{BitDepth, Chunk, ColorType, Colors, Interlace};
use error::Error;
use nom::{
    bits::complete::take as take_bits,
    bytes::complete::{tag, take},
    combinator::{iterator, map, map_res, recognize},
    sequence::tuple,
    IResult,
};

#[derive(Default, Copy, Clone, Debug)]
pub enum Zoom {
    #[default]
    X1,
    X1p5,
    X2,
    X2p5,
    X3,
    X3p5,
    X4,
}

impl From<Zoom> for iced::Size {
    fn from(value: Zoom) -> Self {
        [match value {
            Zoom::X1 => 1.0,
            Zoom::X1p5 => 1.5,
            Zoom::X2 => 2.0,
            Zoom::X2p5 => 2.5,
            Zoom::X3 => 3.0,
            Zoom::X3p5 => 3.5,
            Zoom::X4 => 4.0,
        }; 2]
            .into()
    }
}

impl std::ops::Mul<Zoom> for iced::Point {
    type Output = Self;

    fn mul(self, rhs: Zoom) -> Self::Output {
        match rhs {
            Zoom::X1 => self,
            Zoom::X1p5 => [self.x * 1.5, self.y * 1.5].into(),
            Zoom::X2 => [self.x * 2.0, self.y * 2.0].into(),
            Zoom::X2p5 => [self.x * 2.5, self.y * 2.5].into(),
            Zoom::X3 => [self.x * 3.0, self.y * 3.0].into(),
            Zoom::X3p5 => [self.x * 3.5, self.y * 3.5].into(),
            Zoom::X4 => [self.x * 4.0, self.y * 4.0].into(),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct State {
    zoom: Zoom,
}

impl State {
    pub fn zoom_in(&mut self) -> bool {
        let mut zoomed = true;
        self.zoom = match self.zoom {
            Zoom::X1 => Zoom::X1p5,
            Zoom::X1p5 => Zoom::X2,
            Zoom::X2 => Zoom::X2p5,
            Zoom::X2p5 => Zoom::X3,
            Zoom::X3 => Zoom::X3p5,
            Zoom::X3p5 => Zoom::X4,
            Zoom::X4 => {
                zoomed = false;
                Zoom::X4
            }
        };
        zoomed
    }

    pub fn zoom_out(&mut self) -> bool {
        let mut zoomed = true;
        self.zoom = match self.zoom {
            Zoom::X1 => {
                zoomed = false;
                Zoom::X1
            }
            Zoom::X1p5 => Zoom::X1,
            Zoom::X2 => Zoom::X1p5,
            Zoom::X2p5 => Zoom::X2,
            Zoom::X3 => Zoom::X2p5,
            Zoom::X3p5 => Zoom::X3,
            Zoom::X4 => Zoom::X3p5,
        };
        zoomed
    }

    pub fn zoom_toggle(&mut self) {
        self.zoom = match self.zoom {
            Zoom::X1 | Zoom::X1p5 | Zoom::X2 | Zoom::X2p5 | Zoom::X3 | Zoom::X3p5 => Zoom::X4,
            Zoom::X4 => Zoom::X1,
        };
    }
}

pub fn render(frame: &mut canvas::Frame, data: &[u8], state: &State) -> Result<(), Error> {
    let (data, _) = header(data)?;
    let (data, chunk) = chunks::chunk(data)?;

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

    let mut decoder = ZlibDecoder::new(Renderer::new(
        frame,
        state,
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
            Chunk::Gama(gamma) => {
                decoder.get_mut().set_gamma(gamma);
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

struct Renderer<'frame, 'data, 'state> {
    frame: Option<&'frame mut canvas::Frame>,
    state: &'state State,
    dimensions: iced::Size,
    color_type: ColorType,
    bits_per_pixel: usize,
    interlace: Interlace,
    palette: Option<Colors<'data>>,
    gamma: Option<f32>,
    scanline: usize,
    next_scanline: Vec<u8>,
    prev_scanline: Vec<u8>,
}

impl<'frame, 'data, 'state> Renderer<'frame, 'data, 'state> {
    fn new(
        frame: &'frame mut canvas::Frame,
        state: &'state State,
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

        tracing::debug!("width: {width} height: {height} bit_depth: {bit_depth:?}");
        tracing::debug!("color_type: {color_type:?} interlace: {interlace:?}");
        Ok(Self {
            frame: Some(frame),
            state,
            dimensions: iced::Size::new(width as f32, height as f32),
            color_type,
            bits_per_pixel,
            interlace,
            palette: None,
            gamma: None,
            scanline: 0,
            next_scanline: Vec::with_capacity(scanline_len),
            prev_scanline: Vec::with_capacity(scanline_len),
        })
    }

    fn set_palette(&mut self, colors: Colors<'data>) {
        self.palette = Some(colors);
    }

    fn set_gamma(&mut self, gamma: f32) {
        self.gamma = Some(gamma);
    }

    fn filter(&mut self) -> Result<(), Error> {
        let (_, filter_type) = one_byte_as::<FilterType>(&self.next_scanline)?;
        let bytes_per_pixel = (self.bits_per_pixel + 7) / 8;
        self.next_scanline[0] = 0;

        match filter_type {
            FilterType::None => {}

            FilterType::Sub => {
                for i in 1..self.next_scanline.len() {
                    let prior = i.saturating_sub(bytes_per_pixel);
                    self.next_scanline[i] =
                        self.next_scanline[i].wrapping_add(self.next_scanline[prior]);
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
                    let prior = i.saturating_sub(bytes_per_pixel);
                    let left = self.next_scanline[prior] as u16;
                    let up = *self.prev_scanline.get(i).unwrap_or(&0) as u16;
                    self.next_scanline[i] =
                        self.next_scanline[i].wrapping_add(((left + up) / 2) as u8);
                }
            }

            FilterType::Paeth => {
                for i in 1..self.next_scanline.len() {
                    let prior = i.saturating_sub(bytes_per_pixel);
                    let left = self.next_scanline[prior] as i16;
                    let up = *self.prev_scanline.get(i).unwrap_or(&0) as i16;
                    let up_left = *self.prev_scanline.get(prior).unwrap_or(&0) as i16;
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

    fn draw_pixel(&self, frame: &mut canvas::Frame, x: usize, y: usize, color: iced::Color) {
        // if let Some(gamma) = self.gamma {
        //     color.r = color.r.powf(gamma);
        //     color.g = color.g.powf(gamma);
        //     color.b = color.b.powf(gamma);
        // }

        frame.fill_rectangle(
            iced::Point::new(x as f32, y as f32) * self.state.zoom,
            self.state.zoom.into(),
            color,
        );

        self.draw_pixel_test(color, x == 0);
    }

    fn render(&mut self) -> Result<(), Error> {
        let frame = self.frame.take().ok_or(Error::default())?;

        let from_two_bytes =
            |bytes: &[u8]| u16::from_be_bytes(bytes.try_into().unwrap()) as f32 / u16::MAX as f32;

        if self.bits_per_pixel < 8 {
            let input = (&self.next_scanline[1..], 0);
            let mut iter = iterator(input, take_bits::<_, u8, _, _>(self.bits_per_pixel));

            match self.color_type {
                ColorType::GrayScale => {
                    let max_grayscale = 2f32.powi(self.bits_per_pixel as i32);
                    for (i, bits) in (&mut iter).enumerate() {
                        let grayscale = bits as f32 / max_grayscale;
                        let color = iced::Color::from_rgb(grayscale, grayscale, grayscale);
                        self.draw_pixel(frame, i, self.scanline, color);
                    }
                }

                ColorType::Palette => {
                    if let Some(palette) = self.palette.as_ref() {
                        for (i, bits) in (&mut iter).enumerate() {
                            let color = palette.get(bits as usize);
                            self.draw_pixel(frame, i, self.scanline, color);
                        }
                    }
                }

                _ => unreachable!("already checked in Renderer::new"),
            }

            iter.finish()?;
        } else {
            let input = &self.next_scanline[1..];
            let bytes_per_pixel = self.bits_per_pixel / 8;
            let mut iter = iterator(input, take(bytes_per_pixel));

            match self.color_type {
                ColorType::GrayScale => {
                    for (i, bytes) in (&mut iter).enumerate() {
                        let grayscale = if bytes_per_pixel == 1 {
                            bytes[0] as f32 / u8::MAX as f32
                        } else {
                            from_two_bytes(&bytes[..2])
                        };
                        let color = iced::Color::from_rgb(grayscale, grayscale, grayscale);
                        self.draw_pixel(frame, i, self.scanline, color);
                    }
                }

                ColorType::Rgb => match bytes_per_pixel {
                    3 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let &[red, green, blue] = bytes else {
                                unreachable!("must be 3 bytes per pixel")
                            };
                            let color = iced::Color::from_rgb8(red, green, blue);
                            self.draw_pixel(frame, i, self.scanline, color);
                        }
                    }

                    6 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let red = from_two_bytes(&bytes[..2]);
                            let green = from_two_bytes(&bytes[2..4]);
                            let blue = from_two_bytes(&bytes[4..6]);
                            let color = iced::Color::from_rgb(red, green, blue);
                            self.draw_pixel(frame, i, self.scanline, color);
                        }
                    }

                    _ => unreachable!("already checked in Renderer::new"),
                },

                ColorType::Palette => {
                    if let Some(palette) = self.palette.as_ref() {
                        for (i, byte) in (&mut iter).enumerate() {
                            let color = palette.get(byte[0] as usize);
                            self.draw_pixel(frame, i, self.scanline, color);
                        }
                    }
                }

                ColorType::GrayScaleAlpha => {
                    for (i, bytes) in (&mut iter).enumerate() {
                        let (grayscale, alpha) = if bytes_per_pixel == 2 {
                            (
                                bytes[0] as f32 / u8::MAX as f32,
                                bytes[1] as f32 / u8::MAX as f32,
                            )
                        } else {
                            (from_two_bytes(&bytes[..2]), from_two_bytes(&bytes[2..4]))
                        };
                        let color = iced::Color::from_rgba(grayscale, grayscale, grayscale, alpha);
                        self.draw_pixel(frame, i, self.scanline, color);
                    }
                }

                ColorType::RgbAlpha => match bytes_per_pixel {
                    4 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let &[red, green, blue, alpha] = bytes else {
                                unreachable!("must be 4 bytes per pixel")
                            };
                            let alpha = alpha as f32 / u8::MAX as f32;
                            let color = iced::Color::from_rgba8(red, green, blue, alpha);
                            self.draw_pixel(frame, i, self.scanline, color);
                        }
                    }

                    8 => {
                        for (i, bytes) in (&mut iter).enumerate() {
                            let red = from_two_bytes(&bytes[..2]);
                            let green = from_two_bytes(&bytes[2..4]);
                            let blue = from_two_bytes(&bytes[4..6]);
                            let alpha = from_two_bytes(&bytes[6..8]);
                            let color = iced::Color::from_rgba(red, green, blue, alpha);
                            self.draw_pixel(frame, i, self.scanline, color);
                        }
                    }

                    _ => unreachable!("already checked in Renderer::new"),
                },
            }

            iter.finish()?;
        }

        self.frame = Some(frame);
        Ok(())
    }
}

impl Write for Renderer<'_, '_, '_> {
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

#[cfg(feature = "termcolor")]
impl Renderer<'_, '_, '_> {
    fn draw_pixel_test(&self, color: iced::Color, newline: bool) {
        use termcolor::WriteColor;

        let mut out = termcolor::StandardStream::stdout(termcolor::ColorChoice::Always);
        let [r, g, b, _] = color.into_rgba8();
        out.set_color(termcolor::ColorSpec::new().set_bg(Some(termcolor::Color::Rgb(r, g, b))))
            .unwrap();

        if newline {
            out.write(b"\n").unwrap();
        }
        out.write(b" ").unwrap();
        out.flush().unwrap();
    }
}

#[cfg(not(feature = "termcolor"))]
impl Renderer<'_, '_, '_> {
    fn draw_pixel_test(&self, _color: iced::Color, _newline: bool) {}
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
            assert_eq!(Bytes::from($left as &[u8]), Bytes::from($right as &[u8]),)
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
