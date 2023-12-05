use super::chunks::*;
use super::Error;

use nom::combinator::all_consuming;
use nom::combinator::map_res;
use nom::Parser;
use nom::{
    bytes::complete::{tag, take, take_while_m_n},
    character::is_alphabetic,
    combinator::recognize,
    number::complete::be_u32,
    sequence::tuple,
    IResult,
};

fn one_byte_as<Into: TryFrom<u8, Error = Error>>(input: &[u8]) -> IResult<&[u8], Into, Error> {
    map_res(take(1usize).map(|input: &[u8]| input[0]), |b| {
        Into::try_from(b)
    })
    .parse(input)
}

pub fn header(input: &[u8]) -> IResult<&[u8], &[u8], Error> {
    recognize(tuple((
        tag(&[0x89]),
        tag(b"PNG"),
        tag(&[0x0D, 0x0A, 0x1A, 0x0A]),
    )))(input)
}

pub fn chunk(input: &[u8]) -> IResult<&[u8], Chunk, Error> {
    let (input, length) = be_u32(input)?;
    let (input, ty) = take_while_m_n(4, 4, is_alphabetic)(input)?;
    let critical = ty[0].is_ascii_uppercase();
    let (input, chunk_data) = take(length)(input)?;
    let (_input, _crc) = take(4usize)(input)?;

    let ty_upper = {
        let mut ty: [u8; 4] = ty.try_into().expect("just took exactly 4");
        ty.make_ascii_uppercase();
        ty
    };

    all_consuming(match &ty_upper[..] {
        b"IHDR" => ihdr,
        _ => {
            if critical {
                return Err(nom::Err::Failure(Error::UnknownCriticalChunk(
                    String::from_utf8(ty_upper.to_vec()).unwrap_or_else(|_| "{invalid}".into()),
                )));
            } else {
                unknown
            }
        }
    })(chunk_data)
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
        Chunk::Ihdr(Ihdr {
            width,
            height,
            bit_depth,
            color_type,
            interlace,
        }),
    ))
}
