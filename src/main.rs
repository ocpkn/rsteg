use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use rsteg::hsv::HSVColor;

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

    #[arg(short, long, value_name="KEY",
        requires("mode"))]
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

    let samples = samples - alpha;

    if args.stretch {
        let maxx: u8 = u8::MAX >> (8 - args.bits);

        let minmaxs = input.chunks_exact(samples).fold(vec![(maxx, 0u8); samples],
            |minmaxs, x| {
                let b = x.iter().zip(minmaxs);

                b.map(|(x, (min, max))| (min.min(*x), max.max(*x))).collect()
            }
        );

        for x in input.chunks_exact_mut(samples) {
            for (y, (min, max)) in x.iter_mut().zip(minmaxs.iter()) {
                let new = if *max == 0 {0} else {(*y - *min) as u16 * maxx as u16 / (*max - *min) as u16};
                *y = new as u8;
            }
        }
    } else if args.equalize {
        let hsvinput: Vec<HSVColor> = input.chunks_exact(samples).map(|px| {
            let (r, g, b) = (px[0], px[1], px[2]);
            HSVColor::from_rgb(r, g, b, args.bits)
        }).collect();

        let mut vals: Vec<f32> = hsvinput.iter().map(|hsv| hsv.val).collect();
        vals.sort_by(|a, b| a.partial_cmp(&b).unwrap());
        vals.dedup();

        let cdf = |v| vals.binary_search_by(|a| a.partial_cmp(&v).unwrap()).unwrap() as f32 / (vals.len() - 1) as f32;

        input = hsvinput.iter().flat_map(|hsv| {
            HSVColor {
                hue: hsv.hue,
                sat: hsv.sat,
                val: cdf(hsv.val)
            }.to_rgb(args.bits)
        }).collect();
    }

    // Encrypt/Decrypt
    if let Some(key) = args.key {
        let mut rng = ChaCha20Rng::seed_from_u64(key);

        let max: u8 = u8::MAX >> (8 - args.bits);

        for x in input.iter_mut() {
            *x ^= rng.gen_range(0..=max);
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

        assert!(i_samples - i_alpha == samples);

        let px_iter = input.chunks_exact(samples);
        let i_px_iter = i_buf.chunks_exact(i_samples);

        i_px_iter.zip(px_iter).flat_map(|(i_px, px)| {
            (0..samples).map(|i| {
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
    let w = &mut BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, info.width, info.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().unwrap();

    writer.write_image_data(&out_buf).unwrap();
}
