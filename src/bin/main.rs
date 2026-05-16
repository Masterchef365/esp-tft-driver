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
    gpio::{Level, Output, OutputConfig},
};

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

    let mut spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_khz(100))
            .with_mode(Mode::_0),
    ).unwrap();

    let mut delay = esp_hal::delay::Delay::new();

    let config = OutputConfig::default();

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

    loop {
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}
