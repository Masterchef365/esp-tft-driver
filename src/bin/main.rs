#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

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

use euc::{Buffer2d, Empty, Pipeline, TriangleList};

use egui_euc::Algebra565;
use esp_backtrace as _;
use egui::Widget;
use esp_hal::analog::adc::*;
use alloc::format;

/*
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("{info}");
    esp_println::println!("{}", esp_alloc::HEAP.stats());
    loop {}
}
*/

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
            .with_frequency(Rate::from_khz(40000))
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
        Orientation::LandscapeFlipped,
        ili9341::DisplaySize240x320,
    )
    .unwrap();

    display.clear(Rgb565::RED).unwrap();

    let mut gui = egui_euc::SoftwareGui::new();

    esp_alloc::heap_allocator!(size: 170 * 1024);

    let mut raw_input = egui::RawInput::default();

    /*
    gui.egui_ctx.run(raw_input.clone(), |ctx| {
        
    });
    */

    let [w, h] = [320, 240];
    //let [w, h] = [2*320/3, 2*240/3];
    let mut color = Buffer2d::fill([w, h], Algebra565::BLACK);

    display.clear(Rgb565::BLUE).unwrap();



    let mut adc2_config = AdcConfig::new();
    let mut xm = adc2_config.enable_pin(peripherals.GPIO4, Attenuation::_0dB);
    let mut ym = adc2_config.enable_pin(peripherals.GPIO15, Attenuation::_0dB);
    let mut adc2 = Adc::new(peripherals.ADC2, adc2_config);

    let mut xp = Output::new(peripherals.GPIO2, Level::Low, config);
    let mut yp = Output::new(peripherals.GPIO22, Level::High, config);

    xp.set_low();
    yp.set_high();


    let mut i = 0;
    loop {
        /*
        let pixels_per_point = 0.05;

        for (_, vp) in raw_input.viewports.iter_mut() {
            vp.native_pixels_per_point = Some(pixels_per_point);
        }
        */

        gui.update(
            raw_input.clone(),
            [w, h],
            |ctx| {
                if i == 0 {
                    ctx.fonts(|fonts| {
                        let font_impl = fonts.lock().fonts.font(&Default::default()).fonts[0].clone();
                        *font_impl.glyph_info_cache.write() = egui::epaint::load_glyphs();
                    });
                }

                let xm_value: u16 = nb::block!(adc2.read_oneshot(&mut xm)).unwrap();

                let ym_value: u16 = nb::block!(adc2.read_oneshot(&mut ym)).unwrap();

                egui::CentralPanel::default().show(ctx, |ui| {
                    let rt = egui::RichText::new(format!("XM: {xm_value}")).color(egui::Color32::WHITE).font(Default::default());
                    let button = egui::Button::new(rt).fill(egui::Color32::RED);
                    if button.ui(ui).clicked() {
                    }

                    let rt = egui::RichText::new(format!("YM: {ym_value}")).color(egui::Color32::WHITE).font(Default::default());
                    let button = egui::Button::new(rt).fill(egui::Color32::RED);
                    if button.ui(ui).clicked() {
                    }

                    let rt = egui::RichText::new(format!("I: {i}")).color(egui::Color32::WHITE).font(Default::default());
                    let button = egui::Button::new(rt).fill(egui::Color32::RED);
                    if button.ui(ui).clicked() {
                    }
                });
            },
            &mut color,
        );

        //if i % 100 == 0 {
            esp_println::println!("LOOP {i}:\n{}", esp_alloc::HEAP.stats());
        //}
        i += 1;

        display.draw_raw_iter(0, 0, w as _, h as _, color.raw().iter().map(|c| c.bits));
        /*
        display.draw_raw_iter(
            0,
            0,
            (w * 2) as _,
            (h * 2) as _,
            color
                .raw()
                .chunks(w)
                .map(|chunk| chunk.iter().chain(chunk).map(|c| [c.bits; 2]).flatten())
                .flatten(),
        );
        */

        //let delay_start = Instant::now();
        //while delay_start.elapsed() < Duration::from_millis(500) {}
    }
}
