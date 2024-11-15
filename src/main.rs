use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::cmp::min;

use clap::{ Parser, Subcommand, ValueEnum };

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short, long, value_name="1..8", value_parser=clap::value_parser!(u8).range(1..8), default_value="2")]
    bits: u8,

    #[arg(value_enum, short, long)]
    normalize: Option<NormalTypes>,

    #[arg(short, long, value_name="KEY", conflicts_with="unscramble")]
    scramble: Option<u64>,

    #[arg(short, long, value_name="KEY")]
    unscramble: Option<u64>,

    input: PathBuf,

    output: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Hide {
        image: PathBuf,
    },
    Reveal,
}

#[derive(ValueEnum, Debug, Clone)]
enum NormalTypes {
    Stretch,
    Equalize,
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

    let (info, mut buf) = decode_image(args.input, true);

    let alpha = (info.color_type as usize & 4) >> 2;
    let samples = info.color_type.samples();

    assert!(samples - alpha == 3);

    if let Some(key) = args.scramble {
        let mut rng = ChaCha8Rng::seed_from_u64(key);

        let color_samples = (samples - alpha) as usize;
        let len: usize = (info.width * info.height * (color_samples as u32)) as usize;
        let nums: Vec<usize> = (1..len).rev().map(|i| rng.gen_range(0..i + 1)).collect();

        let f = |i| i + ((alpha * i) / color_samples);

        for i in (1..len).rev() {
            buf.swap(f(i), f(nums[i-1]));
        }
    }

    let mut out_buf: Vec<u8> = match args.command {
        None => {
            buf.chunks_exact(samples).flat_map(|pixel|
                pixel.iter().take(samples - alpha).map(|v| *v)
            ).collect()
        },
        // TODO fix for input grayscale
        Some(Commands::Hide { image: hidden }) => {
            // Decode hidden image
            let (h_info, h_buf) = decode_image(hidden, true);

            // Exit if hidden image is too large
            if h_info.width != info.width || h_info.height != info.height {
                eprintln!("Hidden image dimensions do not match input");
                std::process::exit(-1);
            }

            let mask = 0xFF << args.bits;

            let h_alpha = (h_info.color_type as usize & 4) >> 2;
            let h_samples = h_info.color_type.samples();

            assert!(h_samples - h_alpha == 3);

            let row_iter = buf.chunks_exact(info.line_size);
            let h_row_iter = h_buf.chunks_exact(h_info.line_size);

            h_row_iter.zip(row_iter).flat_map(|(h_row, row)| {
                let h_pixel_iter = h_row.chunks_exact(h_samples);
                let pixel_iter = row.chunks_exact(samples);

                h_pixel_iter.zip(pixel_iter).flat_map(|(h_pixel, pixel)| {
                    (0..h_samples - h_alpha).map(|i| {
                        let h_color = h_pixel[min(h_samples-1-h_alpha, i)];
                        let color = pixel[min(samples-1-alpha, i)];
                        (h_color & mask) | (color >> (8 - args.bits))
                    })
                })
            }).collect()
        },
        Some(Commands::Reveal) => {
            let mask = 0xFF >> (8 - args.bits);

            let max_in = (1 << args.bits) - 1;
            let max_out = 0xFF;

            buf.chunks_exact(samples).flat_map(|pixel| {
                pixel.iter().map(|color| {
                    let input = (color & mask) as u16;
                    (input * max_out / max_in) as u8
                })
            }).collect()
        },
    };

    if let Some(key) = args.unscramble {
        let mut rng = ChaCha8Rng::seed_from_u64(key);

        let nums: Vec<usize> = (1..out_buf.len()).rev().map(|i| rng.gen_range(0..i + 1)).collect();

        for i in 1..out_buf.len() {
            out_buf.swap(i, nums[i - 1]);
        }
    }

    let file = File::create(args.output).unwrap();
    let ref mut w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, info.width, info.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().unwrap();

    writer.write_image_data(&out_buf).unwrap();
}
