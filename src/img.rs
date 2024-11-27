use std::path::PathBuf;
use std::fs::File;
use std::io::BufWriter;
use crate::HSVColor;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

pub fn read_image_rgb8(path: PathBuf) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(File::open(path).expect("Input file not found"));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("Image info failed to read");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("Image data failed to read");
    let samples = info.color_type.samples();

    let x = |b: u8, f: u8, a: u8| {
        let a = a as u16;
        let max = u8::MAX as u16;
        let f = f as u16 * a / max;
        let b = b as u16 * (max - a) / max;
        (f + b) as u8
    };

    let bkgd = [0, 0, 0];

    (info.width, info.height, buf.chunks_exact(samples).flat_map(|s|
        match s.len() {
            1 => [s[0], s[0], s[0]],
            2 => {
                let g = x(0, s[0], s[1]);
                [g, g, g]
            },
            3 => [s[0], s[1], s[2]],
            4 => {
                let r = x(bkgd[0], s[0], s[3]);
                let g = x(bkgd[1], s[1], s[3]);
                let b = x(bkgd[2], s[2], s[3]);
                [r, g, b]
            },
            _ => panic!("Unexpected sample size"),
        }
    ).collect())
}

pub fn stretch(buf: &mut [u8]) {
    let maxx: u8 = u8::MAX;

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

pub fn equalize(buf: &mut [u8]) {
    // Convert image to HSV color
    let mut hsvs: Vec<HSVColor> = 
        buf.chunks_exact(3).map(|p| {
            HSVColor::from_rgb(p[0], p[1], p[2])
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
        for (c, new) in p.iter_mut().zip(hsv.to_rgb()) {
            *c = new;
        }
    }
}

pub fn stream_cipher(buf: &mut [u8], key: u64, bits: u8) {
    // Seed PRNG with key
    let mut rng = ChaCha20Rng::seed_from_u64(key);

    let max: u8 = u8::MAX >> (8 - bits);

    // XOR each pixel with the stream
    for x in buf.iter_mut() {
        *x ^= rng.gen_range(0..=max);
    }
}

pub fn conceal(buf: &mut [u8], bits: u8, width: u32, height: u32, path: PathBuf) {
    // Decode hidden image
    let (i_width, i_height, i_buf) = read_image_rgb8(path);

    // Exit if hidden image is too large
    if i_width != width || i_height != height {
        panic!("Image dimensions do not match");
    }

    let mask = u8::MAX << bits;

    for (c, i_c) in buf.iter_mut().zip(i_buf.iter()) {
        *c |= *i_c & mask;
    }
}

pub fn write_image_rgb8(buf: &[u8], width: u32, height: u32, path: PathBuf) {
    let file = File::create(path).expect("Failed to create output file");
    let w = &mut BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().expect("Failed to write output header");

    writer.write_image_data(buf).expect("Failed to write output data");
}
