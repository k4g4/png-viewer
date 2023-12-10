use std::fmt::Write;

use super::{one_byte_as, Error};

use nom::{
    bytes::complete::{tag, take, take_while_m_n},
    character::is_alphabetic,
    combinator::all_consuming,
    number::complete::be_u32,
    Err, HexDisplay, IResult,
};

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum BitDepth {
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
    Sixteen = 16,
}

impl TryFrom<u8> for BitDepth {
    type Error = super::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            4 => Ok(Self::Four),
            8 => Ok(Self::Eight),
            16 => Ok(Self::Sixteen),
            _ => Err(super::Error::InvalidBitDepth(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ColorType {
    GrayScale = 0,
    Rgb = 2,
    Palette = 3,
    GrayScaleAlpha = 4,
    RgbAlpha = 6,
}

impl TryFrom<u8> for ColorType {
    type Error = super::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::GrayScale),
            2 => Ok(Self::Rgb),
            3 => Ok(Self::Palette),
            4 => Ok(Self::GrayScaleAlpha),
            6 => Ok(Self::RgbAlpha),
            _ => Err(super::Error::InvalidColorType(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Interlace {
    None = 0,
    Adam7 = 1,
}

impl TryFrom<u8> for Interlace {
    type Error = super::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Adam7),
            _ => Err(super::Error::InvalidInterlace(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Colors<'data>(&'data [u8]);

impl<'data> Colors<'data> {
    pub fn new(input: &'data [u8]) -> Result<Self, super::Error> {
        if input.len() % 3 > 0 || input.len() > 256 * 3 {
            Err(super::Error::InvalidPaletteSize(input.len()))
        } else {
            Ok(Self(input))
        }
    }

    pub fn get(&self, index: usize) -> iced::Color {
        if let [r, g, b] = self.0[index * 3..][..3] {
            iced::Color::from_rgb8(r, g, b)
        } else {
            panic!(
                "index out of bounds: the len is {} but the index is {}",
                self.len(),
                index
            )
        }
    }

    pub fn len(&self) -> usize {
        self.0.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[repr(transparent)]
pub struct BytesPrinter([u8]);

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

// SAFETY:  BytesPrinter is just a transparent newtype around [u8],
//          so the transmute is trivial.

impl From<&[u8]> for &BytesPrinter {
    fn from(value: &[u8]) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

impl From<&BytesPrinter> for &[u8] {
    fn from(value: &BytesPrinter) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Chunk<'data> {
    Ihdr {
        width: u32,
        height: u32,
        bit_depth: BitDepth,
        color_type: ColorType,
        interlace: Interlace,
    },
    Plte(Colors<'data>),
    Idat(&'data BytesPrinter),
    Iend,
    Unknown,
}

pub fn chunk(input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    let (input, length) = be_u32(input)?;
    let (input, ty) = take_while_m_n(4, 4, is_alphabetic)(input)?;
    let critical = ty[0].is_ascii_uppercase();
    let (input, chunk_data) = take(length)(input)?;
    let (input, _crc) = take(4usize)(input)?;

    let ty_upper = {
        let mut ty: [u8; 4] = ty.try_into().expect("just took exactly 4");
        ty.make_ascii_uppercase();
        ty
    };

    let (_, chunk) = all_consuming(match &ty_upper {
        b"IHDR" => ihdr,
        b"PLTE" => plte,
        b"IDAT" => idat,
        b"IEND" => iend,
        _ => {
            if critical {
                return Err(Err::Failure(Error::UnknownCriticalChunk(
                    String::from_utf8(ty_upper.to_vec()).unwrap_or_else(|_| "{invalid}".into()),
                )));
            } else {
                tracing::debug!("found unknown chunk: {:?}", std::str::from_utf8(&ty_upper));
                unknown
            }
        }
    })(chunk_data)?;

    Ok((input, chunk))
}

fn unknown(_input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    Ok((b"", Chunk::Unknown))
}

fn ihdr(input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    let (input, width) = be_u32(input)?;
    let (input, height) = be_u32(input)?;
    let (input, bit_depth) = one_byte_as::<BitDepth>(input)?;
    let (input, color_type) = one_byte_as::<ColorType>(input)?;
    let (input, _compression) = tag(b"\x00")(input)?;
    let (input, _filter) = tag(b"\x00")(input)?;
    let (input, interlace) = one_byte_as::<Interlace>(input)?;

    Ok((
        input,
        Chunk::Ihdr {
            width,
            height,
            bit_depth,
            color_type,
            interlace,
        },
    ))
}

fn plte(input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    Ok((
        input,
        Chunk::Plte(Colors::new(input).map_err(Err::Failure)?),
    ))
}

fn idat(input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    Ok((b"", Chunk::Idat(input.into())))
}

fn iend(input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    if input.is_empty() {
        Ok((input, Chunk::Iend))
    } else {
        Err(Err::Failure(Error::InvalidIEnd))
    }
}
