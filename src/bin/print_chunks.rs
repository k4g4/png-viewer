use nom::combinator::iterator;
use png_viewer::render::*;
use std::{env, error::Error, fs::read};

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let mut args = env::args();
    args.next();
    let file_path = args.next().ok_or("Missing file path arg.")?;
    let file_data = read(file_path)?;
    let (input, _) = parse::header(&file_data)?;
    let mut iter = iterator(input, parse::chunk);
    for chunk in &mut iter {
        println!("{chunk:?}");
    }
    iter.finish()?;
    Ok(())
}
