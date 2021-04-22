use clap::{App, Arg};
use fontdue::{Font, FontSettings};
use image::{
    imageops::{resize, FilterType},
    GenericImage, GenericImageView, GrayImage, ImageBuffer, Luma, Pixel,
};
use packer::Packer;
use std::{
    collections::HashMap,
    fmt::{self, Display},
    fs,
    iter::FromIterator,
    path::PathBuf,
};

type RasterCache = HashMap<char, ImageBuffer<Luma<u8>, Vec<u8>>>;

#[derive(Packer)]
#[packer(source = "assets/consolas.ttf")]
struct Assets;

struct AsciiImage(GrayImage, Vec<char>);

impl AsciiImage {
    fn rasterize(&self, font: Font, px: u32) -> ImageBuffer<Luma<u8>, Vec<u8>> {
        let cache: RasterCache = HashMap::from_iter(self.1.iter().map(|c| {
            let (metrics, bitmap) = font.rasterize(*c, (px - 1) as f32);

            assert!(
                metrics.height <= px as usize,
                "rastered image won't fit in bounding box '{}'",
                c
            );

            assert!(
                metrics.width <= px as usize,
                "rastered image won't fit in bounding box '{}'",
                c
            );

            let mut img: ImageBuffer<_, _> = ImageBuffer::from_pixel(px, px, Luma([255]));

            let dx = (px as usize - metrics.width) >> 1;
            let dy = (px as usize - metrics.height) >> 1;

            let mut bitmap = bitmap.into_iter();

            for y in dy..metrics.height + dy {
                for x in dx..metrics.width + dx {
                    img.put_pixel(
                        x as u32,
                        y as u32,
                        Luma([255 - bitmap.next().expect("rasterized image buffer too small")]),
                    )
                }
            }

            (*c, img)
        }));

        let mut img: ImageBuffer<Luma<u8>, _> =
            ImageBuffer::new(self.0.width() * px, self.0.height() * px);

        for iy in 0..self.0.height() {
            for ix in 0..self.0.width() {
                let mut sub_img = img.sub_image(ix * px, iy * px, px, px);
                let c = self.1[(self.0.get_pixel(ix, iy).0[0] as f64 / 255.
                    * (self.1.len() - 1) as f64)
                    .trunc() as usize];

                let raster = cache.get(&c).unwrap();

                for sy in 0..px {
                    for sx in 0..px {
                        sub_img.put_pixel(sx, sy, *raster.get_pixel(sx, sy));
                    }
                }
            }
        }

        img
    }
}

impl Display for AsciiImage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let text = self
            .0
            .rows()
            .map(|row| {
                row.map(|luma| {
                    self.1[(luma.0[0] as f64 / 255. * (self.1.len() - 1) as f64).trunc() as usize]
                        .to_string()
                        .repeat(2)
                })
                .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\r\n");

        write!(f, "{}", text)
    }
}

struct Scaler(Option<u32>, Option<u32>);

impl Scaler {
    fn parse(scale: &str) -> Scaler {
        let sizes: Vec<Option<u32>> = scale
            .split(":")
            .map(|s| match s {
                "_" => None,
                num @ _ => Some(
                    num.parse::<u32>()
                        .expect("couldn't parse scale value as u32"),
                ),
            })
            .collect();

        if sizes.len() == 1 {
            Self(sizes[0], sizes[0])
        } else {
            assert!(sizes.len() == 2, "invalid scale value");
            Self(sizes[0], sizes[1])
        }
    }

    fn scale<I: GenericImageView>(
        &self,
        image: &I,
        filter: FilterType,
    ) -> ImageBuffer<I::Pixel, Vec<<I::Pixel as Pixel>::Subpixel>>
    where
        I::Pixel: 'static,
        <I::Pixel as Pixel>::Subpixel: 'static,
    {
        match (self.0, self.1) {
            (Some(width), Some(height)) => resize(image, width, height, filter),

            (Some(width), None) => {
                let height = width as f64 / image.width() as f64 * image.height() as f64;

                resize(image, width, height as u32, filter)
            }

            (None, Some(height)) => {
                let width = height as f64 / image.height() as f64 * image.width() as f64;
                resize(image, width as u32, height, filter)
            }

            (None, None) => ImageBuffer::from_raw(
                image.width(),
                image.height(),
                image
                    .pixels()
                    .map(|(_, _, pixel)| {
                        pixel
                            .channels()
                            .iter()
                            .map(|p| *p)
                            .collect::<Vec<<I::Pixel as Pixel>::Subpixel>>()
                    })
                    .flatten()
                    .collect(),
            )
            .expect("failed to build image buffer"),
        }
    }
}

fn main() {
    let matches = App::new("ascii")
        .version("1.0")
        .author("QuantumCoded github")
        .about("A simple program to convert images into ascii art")
        .arg(
            Arg::with_name("INPUT")
                .required(true)
                .index(1)
                .help("The image to convert"),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .required(true)
                .index(2)
                .help("The output ascii file"),
        )
        .arg(
            Arg::with_name("scale")
                .short("s")
                .long("scale")
                .help("The resolution to scale the image to")
                .value_name("scale"),
        )
        .arg(
            Arg::with_name("filter")
                .long("filter")
                .help("The scaling filter to use when resizing the image")
                .possible_value("nearest")
                .possible_value("triangle")
                .possible_value("catmull-rom")
                .possible_value("gaussian")
                .possible_value("lanczos3")
                .default_value("lanczos3"),
        )
        .arg(
            Arg::with_name("ascii table")
                .short("t")
                .long("table")
                .help("The ascii character to use ordered from darkest to lightest")
                .default_value("@%#*+=-:. "),
        )
        .arg(
            Arg::with_name("raster")
                .short("r")
                .long("raster")
                .help("Changes the output type from a text file to a rastered image"),
        )
        .arg(
            Arg::with_name("font")
                .long("font")
                .help("The font to use when rastering the image")
                .value_name("font"),
        )
        .arg(
            Arg::with_name("font size")
                .long("font-size")
                .help("The height of the font in pixels")
                .value_name("font size"),
        )
        .arg(
            Arg::with_name("rgb")
                .long("rgb")
                .help("Colors the rasterized characters")
        )
        .get_matches();

    let input: PathBuf = matches.value_of("INPUT").unwrap().into();
    let output: PathBuf = matches.value_of("OUTPUT").unwrap().into();
    let scale = matches.value_of("scale");
    let filter = match matches.value_of("filter").unwrap() {
        "nearest" => FilterType::Nearest,
        "triangle" => FilterType::Triangle,
        "catmull-rom" => FilterType::CatmullRom,
        "gaussian" => FilterType::Gaussian,
        "lanczos3" => FilterType::Lanczos3,
        _ => panic!("unsupported filter type"),
    };
    let ascii = matches.value_of("ascii table").unwrap().chars().collect();
    let rastered = matches.is_present("raster");
    let font = matches.value_of("font");
    let font_size = matches
        .value_of("font size")
        .unwrap_or("16")
        .parse::<u32>()
        .expect("invalid font size");

    if !input.exists() {
        println!("Can not find input image!");
        std::process::exit(0);
    }
    let rgb = matches.is_present("rgb");

    let img = if let Some(size) = scale {
        let scaler = Scaler::parse(size);
        let img = image::open(input).expect("failed to open image").to_luma8();

        scaler.scale(&img, filter)
    } else {
        image::open(input).expect("failed to open image").to_luma8()
    };

    if rastered {
        let font = if let Some(font_path) = font {
            if !(PathBuf::from(font_path)).exists() {
                println!("Can not find font file!");
                std::process::exit(0);
            }

            Font::from_bytes(
                fs::read(font_path).expect("can't read font file"),
                FontSettings::default(),
            )
            .expect("can't parse font file")
        } else {
            Font::from_bytes(
                Assets::get("assets/consolas.ttf").unwrap(),
                FontSettings::default(),
            )
            .unwrap()
        };

        AsciiImage(img, ascii)
            .rasterize(font, font_size)
            .save(output)
            .expect("failed to write output file");
    } else {
        let ascii_image = AsciiImage(img, ascii);
        fs::write(output, ascii_image.to_string()).expect("failed to write output file");
    }
}
