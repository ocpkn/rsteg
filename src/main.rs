use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use rsteg::hsv::HSVColor;

use clap::Parser;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/*
spec, or: how are we gonna deal with alpha channels
cases for hide:
- input has alpha/input doesn't have alpha
- conceal has alpha/conceal doesn't have alpha
- encrypted/not encrypted
*/

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

// TODO return dimensions and samples, write function to remove alpha channel
fn read_image(path: PathBuf) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(File::open(path).expect("Input file not found"));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("Image info failed to read");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("Image data failed to read");
    let samples = info.color_type.samples();

    (info.width, info.height, buf.chunks_exact(samples).flat_map(|s|
        match s.len() {
            1 => [s[0], s[0], s[0]],
            2 => {
                let a = s[1] as u16;
                let g = (a * s[0] as u16 / 0xFF) as u8;
                [g, g, g]
            },
            3 => [s[0], s[1], s[2]],
            4 => {
                let a = s[3] as u16;
                let r = (a * s[0] as u16 / 0xFF) as u8;
                let g = (a * s[1] as u16 / 0xFF) as u8;
                let b = (a * s[2] as u16 / 0xFF) as u8;
                [r, g, b]
            },
            _ => panic!(),
        }
    ).collect())
}

#[allow(dead_code)]
fn decode_image(path: PathBuf, normalize: bool) -> (png::OutputInfo, Vec<u8>) {
    let mut decoder = png::Decoder::new(File::open(path).expect("Input file not found"));

    if normalize {
        decoder.set_transformations(png::Transformations::normalize_to_color8());
    }

    let mut reader = decoder.read_info().expect("Image info failed to read");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("Image data failed to read");

    (info, buf)
}

fn main() {
    let args = Args::parse();

    let (width, height, mut buf) = read_image(args.input);

    if !args.reveal {
        for c in buf.iter_mut() {
            *c >>= 8 - args.bits;
        }
    }

    // Contrast stretching algorithm for normalization
    if args.stretch {
        let maxx: u8 = u8::MAX >> (8 - args.bits);

        let minmaxs = buf.chunks_exact(3).fold(
            vec![(maxx, 0u8); 3],
            |minmaxs, p| {
                let b = p.iter().zip(minmaxs);
                b.map(|(p, (min, max))| (min.min(*p), max.max(*p))).collect()
            }
        );

        for p in buf.chunks_exact_mut(3) {
            for (c, (min, max)) in p.iter_mut()
                                    .zip(minmaxs.iter()) {
                let new = if *max == 0 {0}
                else {(*c - *min) as u16 * maxx as u16 / (*max - *min) as u16};
                *c = new as u8;
            }
        }
    }

    // Histogram equalization algorithm, normalizes HSV value
    else if args.equalize {
        // Convert image to HSV color
        let mut hsvs: Vec<HSVColor> = 
            buf.chunks_exact(3).map(|p| {
                HSVColor::from_rgb(p[0], p[1], p[2], args.bits)
            }).collect();

        // Create a sorted vector of unique values for the CDF
        let mut vals: Vec<f32> = hsvs.iter().map(|hsv| hsv.val).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vals.dedup();

        // Define the CDF with range [0, 1]
        let cdf = |v| vals.binary_search_by(|a|
            a.partial_cmp(&v).unwrap()
        ).unwrap() as f32 / (vals.len() - 1) as f32;

        // Equalize and convert back to RGB
        for (p, hsv) in buf.chunks_exact_mut(3).zip(hsvs.iter_mut()) {
            hsv.val = cdf(hsv.val);
            for (c, new) in p.iter_mut().zip(hsv.to_rgb(args.bits)) {
                *c = new;
            }
        }
    }

    // Encryption/decryption using a stream cipher
    if let Some(key) = args.key {
        // Seed PRNG with key
        let mut rng = ChaCha20Rng::seed_from_u64(key);

        let max: u8 = u8::MAX >> (8 - args.bits);

        // XOR each pixel with the stream
        for x in buf.iter_mut() {
            *x ^= rng.gen_range(0..=max);
        }
    }

    // Concealing an image in another
    if let Some(image) = args.conceal {
        // Decode hidden image
        let (i_width, i_height, i_buf) = read_image(image);

        // Exit if hidden image is too large
        if i_width != width || i_height != height {
            eprintln!("Image dimensions do not match");
            std::process::exit(-1);
        }

        let mask = u8::MAX << args.bits;

        for (c, i_c) in buf.iter_mut().zip(i_buf.iter()) {
            *c = (*i_c & mask) | *c;
        }

    } else {
        let max_out = u8::MAX;
        let mask = max_out >> (8 - args.bits);

        for c in buf.iter_mut() {
            *c = ((*c & mask) as u16 * max_out as u16 / mask as u16) as u8;
        }
    };

    let file = File::create(args.output).unwrap();
    let w = &mut BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().unwrap();

    writer.write_image_data(&buf).unwrap();
}
