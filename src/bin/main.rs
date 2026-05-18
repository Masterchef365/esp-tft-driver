#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use display_interface::WriteOnlyDataCommand;
use display_interface_spi::SPIInterface;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::RgbColor;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Pin;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::main;
use esp_hal::spi::{
    master::{Config, Spi},
    Mode,
};
use esp_hal::time::Rate;
use esp_hal::time::{Duration, Instant};
use ili9341::Ili9341;
use ili9341::Orientation;

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

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let config = OutputConfig::default();

    let mosi = Output::new(peripherals.GPIO23, Level::Low, config);
    let miso = Input::new(peripherals.GPIO19, InputConfig::default());
    let sck = Output::new(peripherals.GPIO18, Level::Low, config);

    let mut spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_khz(10000))
            .with_mode(Mode::_0),
    )
    .unwrap()
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

    let mut rng = esp_hal::rng::Rng::new();

    let rule = if rng.random() & 1 == 0 {
        Default::default()
    } else {
        let mut b = (rng.random() & 0b111111111) as u16;
        let mut s = (rng.random() & 0b111111111) as u16;

        b >>= rng.random() % 9;
        s >>= rng.random() % 9;

        Rule {
            b, s
        }
    };

    //let mut sim = Sim::new(Default::default());

    let mut sim = Sim::new(rule);

    loop {
        sim.draw(&mut display);
        sim.step();

        //let delay_start = Instant::now();
        //while delay_start.elapsed() < Duration::from_millis(1500) {}
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

const BUF_WIDTH: usize = 320;
const BUF_HEIGHT: usize = 240;
const BUF_WIDTH_BYTES: usize = BUF_WIDTH / 8;
const BUF_SIZE_BYTES: usize = BUF_WIDTH_BYTES * BUF_HEIGHT;

struct Sim {
    front: Buffer,
    back: Buffer,
    rule: Rule,
}

struct Buffer {
    bytes: [u8; BUF_SIZE_BYTES],
}

impl Sim {
    pub fn new(rule: Rule) -> Self {
        Self {
            back: Buffer::zeros(),
            front: Buffer::random(),
            rule,
        }
    }

    pub fn step(&mut self) {
        for y in 0..BUF_HEIGHT as i16 {
            for x in 0..BUF_WIDTH as i16 {
                let mut neighbors: u8 = 0;

                for yi in y - 1 ..= y + 1 {
                    for xi in x - 1 ..= x + 1 {
                        if (x, y) == (xi, yi) {
                            continue;
                        }

                        if let Some(true) = self.front.read(xi, yi) {
                            neighbors += 1;
                        }
                    }
                }

                let center = self.front.read(x, y).unwrap();
                let next = self.rule.exec(neighbors, center);

                self.back.write(x, y, next);
            }
        }

        core::mem::swap(&mut self.front, &mut self.back);
    }

    pub fn draw<I, R>(&self, mut display: &mut Ili9341<I, R>)
    where
        I: WriteOnlyDataCommand,
    {
        let iter = self
            .front
            .bytes
            .iter()
            .zip(&self.back.bytes)
            .map(|(front, back)| {
                (0..8).map(|i| {
                    let f = extract_bit(*front, i);
                    let b = extract_bit(*back, i);

                    if f {
                        if b {
                            0b1111100000000000_u16
                        } else {
                            0xFFFF_u16
                        }
                    } else {
                        0x0000_u16
                    }
                })
            })
        .flatten();

        display
            .draw_raw_iter(0, 0, BUF_WIDTH as _, BUF_HEIGHT as _, iter)
            .unwrap();
    }
}

impl Buffer {
    fn zeros() -> Self {
        Self {
            bytes: [0; BUF_SIZE_BYTES],
        }
    }

    fn random() -> Self {
        let mut ret = Self::zeros();

        let mut rng = esp_hal::rng::Rng::new();
        rng.read(&mut ret.bytes);

        let div = (rng.random() & 0xff) as u8;
        for b in &mut ret.bytes {
            *b /= div;
        }

        ret
    }

    fn draw<I, R>(&self, mut display: &mut Ili9341<I, R>)
    where
        I: WriteOnlyDataCommand,
    {
        let iter = self
            .bytes
            .iter()
            .map(|byte| {
                (0..8).map(|i| {
                    if extract_bit(*byte, i) {
                        0xFFFF_u16
                    } else {
                        0x0000_u16
                    }
                })
            })
        .flatten();

        display
            .draw_raw_iter(0, 0, BUF_WIDTH as _, BUF_HEIGHT as _, iter)
            .unwrap();
    }

    fn bounds(x: i16, y: i16) -> bool {
        x >= 0 && y >= 0 && (x as usize) < BUF_WIDTH && (y as usize) < BUF_HEIGHT
    }

    /// Returns (byte, bit)
    fn index(x: i16, y: i16) -> Option<(usize, u8)> {
        Self::bounds(x, y).then(|| {
            let idx = y as usize * BUF_WIDTH_BYTES + x as usize / 8;
            let bit = (x % 8) as u8;
            (idx, bit)
        })
    }

    fn read(&self, x: i16, y: i16) -> Option<bool> {
        let (idx, bit) = Self::index(x, y)?;
        Some(extract_bit(self.bytes[idx], bit))
    }

    fn write(&mut self, x: i16, y: i16, value: bool) {
        let (idx, bit) = Self::index(x, y).unwrap();
        self.bytes[idx] = set_bit(self.bytes[idx], bit, value);
    }


}

struct Rule {
    b: u16,
    s: u16,
}

fn pack_rule(indices: &[u16]) -> u16 {
    let mut ret: u16 = 0;

    for i in indices {
        ret = set_bit_u16(ret, *i, true);
    }

    ret
}

impl Rule {
    pub fn new(born: &[u16], survive: &[u16]) -> Self {
        Self {
            b: pack_rule(born),
            s: pack_rule(survive),
        }
    }

    pub fn exec(&self, neighbors: u8, center: bool) -> bool {
        if center {
            extract_bit_u16(self.s, neighbors as _)
        } else {
            extract_bit_u16(self.b, neighbors as _)
        }
    }
}

fn mask(bit: u8) -> u8 {
    1 << bit
}

fn mask_u16(bit: u16) -> u16 {
    1 << bit
}

fn extract_bit(byte: u8, bit: u8) -> bool {
    (byte & mask(bit)) != 0
}

fn extract_bit_u16(byte: u16, bit: u16) -> bool {
    (byte & mask_u16(bit)) != 0
}

fn set_bit(byte: u8, bit: u8, value: bool) -> u8 {
    (byte & !mask(bit)) | ((value as u8) << bit)
}

fn set_bit_u16(byte: u16, bit: u16, value: bool) -> u16 {
    (byte & !mask_u16(bit)) | ((value as u16) << bit)
}

impl Default for Rule {
    fn default() -> Self {
        Rule::new(&[3], &[2, 3])
    }
}
