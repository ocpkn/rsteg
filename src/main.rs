use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::cmp::min;
use clap::{ Parser, Subcommand, ValueEnum };

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, value_name="1..8", value_parser=clap::value_parser!(u8).range(1..8), default_value="2")]
    bits: u8,

    #[arg(value_enum, short, long)]
    normalize: Option<NormalTypes>,

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

    let (info, buf) = decode_image(args.input, true);
    let out_buf: Vec<u8> = match args.command {
        // TODO fix for input grayscale
        Commands::Hide { image: hidden } => {
            // Decode hidden image
            let (h_info, h_buf) = decode_image(hidden, true);

            // Exit if hidden image is too large
            if h_info.width != info.width || h_info.height != info.height {
                eprintln!("Hidden image dimensions do not match input");
                std::process::exit(-1);
            }

            let mask = 0xFF << args.bits;

            let alpha = (info.color_type as u8 & 4) > 0;
            let samples = info.color_type.samples();

            let h_alpha = (h_info.color_type as u8 & 4) > 0;
            let h_samples = h_info.color_type.samples();

            let row_iter = buf.chunks_exact(info.line_size);
            let h_row_iter = h_buf.chunks_exact(h_info.line_size);

            row_iter.zip(h_row_iter).flat_map(|(row, h_row)| {
                let pixel_iter = row.chunks_exact(samples);
                let h_pixel_iter = h_row.chunks_exact(h_samples);

                pixel_iter.zip(h_pixel_iter).flat_map(|(pixel, h_pixel)| {
                    (0..3).map(|i| {
                        let color = pixel[min(samples-1-alpha as usize, i)];
                        let h_color = h_pixel[min(h_samples-1-h_alpha as usize, i)];
                        (color & mask) | (h_color >> (8 - args.bits))
                    })
                })
            }).collect()
        },
        Commands::Reveal => {
            let mask = 0xFF >> (8 - args.bits);

            let alpha = (info.color_type as usize & 4) >> 2;
            let samples = info.color_type.samples() - alpha;

            assert!(samples == 3);

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

    let file = File::create(args.output).unwrap();
    let ref mut w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, info.width, info.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().unwrap();

    writer.write_image_data(&out_buf).unwrap();
}
