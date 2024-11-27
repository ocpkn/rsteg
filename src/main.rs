use std::path::PathBuf;

use clap::Parser;

mod hsv;
use crate::hsv::HSVColor;

mod img;

// TODO background color option

// CLI arg definition
#[derive(Parser, Debug)]
struct Args {
    input: PathBuf,

    #[arg(short, long, default_value("out.png"))]
    output: PathBuf,

    #[arg(short, long,
        group="mode",
        requires("bits"))]
    reveal: bool,

    #[arg(short, long,
        group="mode",
        requires("bits"))]
    conceal: Option<PathBuf>,

    #[arg(short, long, value_name="KEY")]
    key: Option<u64>,

    #[arg(short, long, value_name="1-8", value_parser=clap::value_parser!(u8).range(1..9),
        default_value("8"))]
    bits: u8,

    #[arg(short, long,
        conflicts_with_all(["equalize", "reveal"]))]
    stretch: bool,

    #[arg(short, long,
        conflicts_with_all(["reveal"]))]
    equalize: bool,
}

fn main() {
    let args = Args::parse();

    let (width, height, mut buf) = img::read_image_rgb8(args.input);

    // Contrast stretching algorithm for normalization
    if args.stretch {
        img::stretch(&mut buf);
    }

    // Histogram equalization algorithm, normalizes HSV value
    else if args.equalize {
        img::equalize(&mut buf);
    }

    if !args.reveal {
        for c in buf.iter_mut() {
            *c >>= 8 - args.bits;
        }
    }

    // Encryption/decryption using a stream cipher
    if let Some(key) = args.key {
        img::stream_cipher(&mut buf, key, args.bits);
    }

    // Concealing an image in another
    if let Some(image) = args.conceal {
        img::conceal(&mut buf, args.bits, width, height, image);
    } else {
        let max_out = u8::MAX;
        let mask = max_out >> (8 - args.bits);

        for c in buf.iter_mut() {
            *c = ((*c & mask) as u16 * max_out as u16 / mask as u16) as u8;
        }
    };

    img::write_image_rgb8(&buf, width, height, args.output);
}
