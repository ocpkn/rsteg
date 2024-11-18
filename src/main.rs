use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use clap::Parser;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

// TODO support alpha channels
// TODO add histogram equalization and stretching
// TODO support text instead of images?

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
        requires("bits"),
        requires("image"))]
    conceal: bool,

    #[arg(short, long,
        requires("conceal"))]
    image: Option<PathBuf>,

    #[arg(short, long,
        group="mode",
        requires("key"))]
    encrypt: bool,

    #[arg(short, long,
        group="mode",
        requires("key"))]
    decrypt: bool,

    #[arg(short, long, value_name="KEY",
        requires("mode"))]
    key: Option<u64>,

    #[arg(short, long, value_name="1-8", value_parser=clap::value_parser!(u8).range(1..9),
        default_value("8"))]
    bits: u8,
}

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

    let (info, buf) = decode_image(args.input, true);

    let alpha: usize = (info.color_type as usize & 4) >> 2;
    let samples: usize = info.color_type.samples();

    assert!(samples - alpha == 3);

    // Remove alpha channel
    let mut input: Vec<u8> = buf.chunks_exact(samples).flat_map(
        |p| p.iter().take(samples - alpha).map(|x|
            if args.reveal {
                *x
            } else {
                x >> (8 - args.bits)
            }
        )
    ).collect();

    // Encrypt/Decrypt
    if let Some(key) = args.key {
        let mut rng = ChaCha20Rng::seed_from_u64(key);

        let max: u16 = 1 << (args.bits);

        for x in input.iter_mut() {
            let rand = rng.gen_range(0..max);
            let num = if args.decrypt || args.reveal {max - rand} else {rand};
            let y = *x as u16 + num;
            *x = (y % max) as u8;
        }
    }

    let out_buf: Vec<u8> = if let Some(image) = args.image {
        // Decode hidden image
        let (i_info, i_buf) = decode_image(image, true);

        // Exit if hidden image is too large
        if i_info.width != info.width || i_info.height != info.height {
            eprintln!("Image dimensions do not match");
            std::process::exit(-1);
        }

        let mask = u8::MAX << args.bits;

        let i_alpha: usize = (i_info.color_type as usize & 4) >> 2;
        let i_samples: usize = i_info.color_type.samples();

        assert!(i_samples - i_alpha == samples - alpha);

        let px_iter = input.chunks_exact(samples - alpha);
        let i_px_iter = i_buf.chunks_exact(i_samples);

        i_px_iter.zip(px_iter).flat_map(|(i_px, px)| {
            (0..i_samples - i_alpha).map(|i| {
                (i_px[i] & mask) | px[i]
            })
        }).collect()
    } else {
        let max_out: u8 = u8::MAX;
        let mask: u8 = max_out >> (8 - args.bits);

        input.chunks_exact(samples).flat_map(|p| {
            p.iter().map(|color| {
                ((color & mask) as u16 * max_out as u16 / mask as u16) as u8
            })
        }).collect()
    };

    let file = File::create(args.output).unwrap();
    let ref mut w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, info.width, info.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().unwrap();

    writer.write_image_data(&out_buf).unwrap();
}
