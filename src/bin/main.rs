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

use alloc::format;
use egui::Widget;
use egui_euc::Algebra565;
use esp_backtrace as _;
use esp_hal::analog::adc::*;
use esp_hal::Blocking;

use egui_esp32::touch::*;
use alloc::vec::Vec;

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

    let [w, h] = [320, 240];

    display.clear(Rgb565::BLUE).unwrap();

    let mut toucher = TouchController::new();

    let mut i = 0;
    loop {
        let mut raw_input = egui::RawInput::default();

        toucher.next(&mut raw_input.events);

        let tile_size = 240;

        let color = gui.update(
            raw_input.clone(),
            [w, h],
            tile_size,
            |ctx| {
                if i == 0 {
                    ctx.fonts(|fonts| {
                        let font_impl =
                            fonts.lock().fonts.font(&Default::default()).fonts[0].clone();
                        *font_impl.glyph_info_cache.write() = egui::epaint::load_glyphs();
                    });
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some(pos) = ui.ctx().pointer_hover_pos() {
                        esp_println::println!("{pos:?}");
                        let rect = egui::Rect::from_center_size(pos, egui::Vec2::splat(25.0));
                        ui.painter().rect_filled(
                            rect,
                            0.0,
                            egui::Color32::MAGENTA,
                        );
                    }

                    let rt = egui::RichText::new(format!("I {i}"))
                        .color(egui::Color32::WHITE)
                        .font(Default::default());
                    let button = egui::Button::new(rt).fill(egui::Color32::RED);
                    if ui.add_sized(egui::Vec2::new(100.0, 50.0), button).clicked() {
                        i += 1;
                    }
                });
            },
            |x, y, ex, ey, buf| {
                display.draw_raw_iter(
                    x as _,
                    y as _,
                    (ex - 1) as _,
                    (ey - 1) as _,
                    buf.raw().iter().map(|c| c.bits),
                );
            },
        );
    }
}

struct TouchController {
    last_touch_point: Option<(usize, usize)>,
    touch_id: u64,
}

impl TouchController {
    pub fn new() -> Self {
        Self {
            last_touch_point: None,
            touch_id: 0,
        }
    }

    pub fn next(&mut self, events: &mut Vec<egui::Event>) {
        let point = get_touch_point().to_pixel_point();
        let mut phase = egui::TouchPhase::Move;

        let mut ret_point = point;

        if let Some(last) = self.last_touch_point {
            if point.is_none() {
                ret_point = Some(last);
                phase = egui::TouchPhase::End;
            }
        } else {
            phase = egui::TouchPhase::Start;
            self.touch_id += 1;
        }

        self.last_touch_point = point;

        if let Some((x, y)) = ret_point {
            let pos = egui::Pos2::new(x as _, y as _);
            match phase {
                egui::TouchPhase::Start => {
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    });
                },
                egui::TouchPhase::End => {
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    });
                    events.push(egui::Event::PointerGone);
                }
                _ => {
                    events.push(egui::Event::PointerMoved(pos));
                },

            }
        }

        /*
        ret_point.map(|(x, y)| egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(self.touch_id),
            phase,
            pos: egui::Pos2::new(x as _, y as _),
            force: None,
        })
        */
    }
}

