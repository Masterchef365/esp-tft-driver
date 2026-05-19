#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::spi::{
    Mode,
    master::{Config, Spi},
};
use ili9341::Ili9341;
use ili9341::Orientation;
use esp_hal::time::Rate;
use display_interface_spi::SPIInterface;
use esp_hal::gpio::Pin;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::RgbColor;
use esp_hal::{
    gpio::{Level, Output, Input, InputConfig, OutputConfig},
};

use vek::num_traits::Float;

use euc::{Buffer2d, Empty, Pipeline, TriangleList};
use vek::Rgba;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32 -o esp32-wroom-32 -o alloc -o neovim -o esp

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO2
    // - GPIO5
    // - GPIO12
    // - GPIO15
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO6;
    let _ = peripherals.GPIO7;
    let _ = peripherals.GPIO8;
    let _ = peripherals.GPIO9;
    let _ = peripherals.GPIO10;
    let _ = peripherals.GPIO11;
    let _ = peripherals.GPIO16;
    let _ = peripherals.GPIO20;

    esp_alloc::heap_allocator!(size: 150 * 1024);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let config = OutputConfig::default();

    let mosi = Output::new(peripherals.GPIO23, Level::Low, config);
    let miso = Input::new(peripherals.GPIO19, InputConfig::default());
    let sck = Output::new(peripherals.GPIO18, Level::Low, config);

    let mut spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_khz(40000))
            .with_mode(Mode::_0),
    ).unwrap()
    .with_sck(sck)
    .with_mosi(mosi)
    .with_miso(miso);

    let mut delay = esp_hal::delay::Delay::new();

    let dc = Output::new(peripherals.GPIO21, Level::Low, config);
    let cs = Output::new(peripherals.GPIO16, Level::Low, config);

    let reset_gpio = Output::new(peripherals.GPIO5, Level::Low, config); // Unused

    let device = ExclusiveDevice::new(spi, cs, delay).unwrap();

    let iface = SPIInterface::new(device, dc);

    let mut display = Ili9341::new(
        iface,
        reset_gpio,
        &mut delay,
        Orientation::Landscape,
        ili9341::DisplaySize240x320,
    )
    .unwrap();

    display.clear(Rgb565::RED).unwrap();

    let [w, h] = [320, 240];
    //let [w, h] = [320/2, 240/2];
    let mut color = Buffer2d::fill([w, h], 0);

    display.clear(Rgb565::BLUE).unwrap();

    let mut i = 0;
    let colors = [Algebra565::RED, Algebra565::GREEN, Algebra565::BLUE, Algebra565::CYAN, Algebra565::YELLOW, Algebra565::MAGENTA];
    loop {
        Triangle.render(
            &[
                ([-1.0, -1.0], colors[i]),
                ([1.0, -1.0], colors[(i + 1) % colors.len()]),
                ([0.0, 1.0], colors[(i + 2) % colors.len()]),
            ],
            &mut color,
            &mut Empty::default(),
        );
        i = (i + 1) % colors.len();

        display.draw_raw_iter(0, 0, w as _, h as _, color.raw().iter().copied());

        //let delay_start = Instant::now();
        //while delay_start.elapsed() < Duration::from_millis(500) {}
    }
}

struct Triangle;

impl<'r> Pipeline<'r> for Triangle {
    type Vertex = ([f32; 2], Algebra565);
    type VertexData = Algebra565;
    type Primitives = TriangleList;
    type Fragment = Algebra565;
    type Pixel = u16;

    fn vertex(&self, (pos, col): &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        ([pos[0], pos[1], 0.0, 1.0], *col)
    }

    fn fragment(&self, col: Self::VertexData) -> Self::Fragment {
        col
    }

    fn blend(&self, _: Self::Pixel, col: Self::Fragment) -> Self::Pixel {
        col.bits
    }
}

#[derive(Copy, Clone, Default)]
struct Algebra565 {
    bits: u16,
}

fn float_to_bits(value: f32, nbits: u8) -> u16 {
    let maxval = ((1u16 << nbits) - 1) as f32;
    (value.clamp(0.0, 1.0) * maxval).floor() as u16
}

fn bits_to_float(bits: u16, nbits: u8) -> f32 {
    let maxval = ((1u16 << nbits) - 1) as f32;
    bits as f32 / maxval
}

fn extract_bits_range(bits: u16, nbits: u8, position: u8) -> u16 {
    (bits >> position) & ((1 << nbits) - 1)
}

impl Algebra565 {
    pub const RED: Self = Self { bits: 0b1111100000000000 };
    pub const GREEN: Self = Self { bits: 0b0000011111100000 };
    pub const BLUE: Self = Self { bits: 0b0000000000011111 };
    pub const CYAN: Self = Self { bits: 0b0000011111111111 };
    pub const YELLOW: Self = Self { bits: 0b1111111111000000 };
    pub const MAGENTA: Self = Self { bits: 0b1111100000011111 };

    pub fn new(bits: u16) -> Self {
        Self { bits }
    }

    pub fn to_bgrf(&self) -> [f32; 3] {
        [
            bits_to_float(extract_bits_range(self.bits, 5, 0), 5),
            bits_to_float(extract_bits_range(self.bits, 6, 5), 6),
            bits_to_float(extract_bits_range(self.bits, 5, 6+5), 5),
        ]
    }

    pub fn from_bgrf([b, g, r]: [f32; 3]) -> Self {
        let mut bits = 0;
        bits |= float_to_bits(b, 5);
        bits |= float_to_bits(g, 6) << 5;
        bits |= float_to_bits(r, 5) << (5+6);
        Self { bits }
    }
}

#[cfg(test)]
#[test]
fn test_algebra565_roundtrip() {
    assert_eq!(Algebra565::from_bgrf([1., 0., 0.]).to_bgrf(), [1.0, 0.0, 0.0]);
    assert_eq!(Algebra565::from_bgrf([0., 1., 0.]).to_bgrf(), [0.0, 1.0, 0.0]);
    assert_eq!(Algebra565::from_bgrf([0., 0., 1.]).to_bgrf(), [0.0, 0.0, 1.0]);

    assert_eq!(Algebra565::from_bgrf([15.0/31.0, 19.0/63.0, 19.0/31.0]).to_bgrf(), [15.0/31.0, 19.0/63.0, 19.0/31.0]);
}

impl euc::math::WeightedSum for Algebra565 {
    fn weighted_sum<const N: usize>(
        values: [Self; N],
        weights: [f32; N],
    ) -> Self {
        let mut sum = [0_f32; 3];

        for i in 0..N {
            let bgr = values[i].to_bgrf();

            for j in 0..3 {
                sum[j] += bgr[j] * weights[i];
            }
        }

        Self::from_bgrf(sum)
    }
}
