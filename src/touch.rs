#![no_std]

const ATTENUATION: Attenuation = Attenuation::_11dB;

type ADC = ADC2<'static>;
type YP = GPIO15<'static>;
type XM = GPIO4<'static>;
type YM = GPIO22<'static>;
type XP = GPIO2<'static>;

const TS_MINX: usize = 116*2;
const TS_MAXX: usize = 890*2;
const TS_MINY: usize = 83*2;
const TS_MAXY: usize = 913*2;

use esp_hal::Blocking;
use esp_hal::{
    analog::adc::*,
    gpio::{Input, Level, Output},
};

use nb::block;

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};

use esp_hal::gpio::RtcPin;



// ======================================================
// Pin aliases
// Change these to match your board wiring
// ======================================================

use esp_hal::peripherals::*;

// ======================================================
// Touchscreen constants
// ======================================================

const NUMSAMPLES: usize = 2;
const COMP: u16 = 8;
const RXPLATE: f32 = 300.0;
const PRESSURE_MIN: u16 = 10;

// ======================================================
// Touch point structure
// ======================================================

#[derive(Debug, Clone, Copy)]
pub struct TSPoint {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

// ======================================================
// Averaged ADC read helper
// ======================================================

fn average_adc_read<T: AdcChannel>(
    adc: &mut Adc<'_, ADC, Blocking>,
    pin: &mut AdcPin<T, ADC>,
) -> u16 {
    const N: usize = 4;

    let mut sum: u32 = 0;
    let mut min = u16::MAX;
    let mut max = 0u16;

    for _ in 0..N {
        let v: u16 = block!(adc.read_oneshot(pin)).unwrap();

        if v < min {
            min = v;
        }

        if v > max {
            max = v;
        }

        sum += v as u32;
    }

    ((sum - min as u32 - max as u32) / (N as u32 - 2)) as u16
}

// ======================================================
// Main touchscreen read function
// ======================================================

pub fn get_touch_point() -> TSPoint {
    let mut point = TSPoint { x: 0, y: 0, z: 0 };

    let mut valid = true;

    // ==================================================
    // READ X
    //
    // XP = HIGH
    // XM = LOW
    // read YP
    // ==================================================

    let rtcio = esp_hal::peripherals::RTC_IO::regs();

    let mut x_samples = [0u16; NUMSAMPLES];

    {
        let mut adc_config = AdcConfig::new();

        let mut yp = adc_config.enable_pin(unsafe { YP::steal() }, ATTENUATION);

        let mut adc = Adc::new(unsafe { ADC::steal() }, adc_config);

        let _ym = Input::new(unsafe { YM::steal() }, Default::default());

        let _xp = Output::new(unsafe { XP::steal() }, Level::High, Default::default());

        let _xm = Output::new(unsafe { XM::steal() }, Level::Low, Default::default());

        esp_hal::delay::Delay::new().delay_micros(2_000);

        for i in 0..NUMSAMPLES {
            x_samples[i] = average_adc_read(&mut adc, &mut yp);
        }
    }

    rtcio.touch_pad3().modify(|_,w| {
        w.mux_sel().clear_bit();
        w
    });

    if x_samples[0].abs_diff(x_samples[1]) > COMP {
        valid = false;
    }

    point.x = ((x_samples[0] as u32 + x_samples[1] as u32) / 2) as u16;

    // ==================================================
    // READ Y
    //
    // YP = HIGH
    // YM = LOW
    // read XM
    // ==================================================

    let mut y_samples = [0u16; NUMSAMPLES];

    {
        let mut adc_config = AdcConfig::new();

        let mut xm = adc_config.enable_pin(unsafe { XM::steal() }, ATTENUATION);

        let mut adc = Adc::new(unsafe { ADC::steal() }, adc_config);

        let _xp = Input::new(unsafe { XP::steal() }, Default::default());

        let _yp = Output::new(unsafe { YP::steal() }, Level::High, Default::default());

        let _ym = Output::new(unsafe { YM::steal() }, Level::Low, Default::default());

        esp_hal::delay::Delay::new().delay_micros(2_000);

        for i in 0..NUMSAMPLES {
            y_samples[i] = average_adc_read(&mut adc, &mut xm);
        }
    }

    rtcio.touch_pad0().modify(|_,w| {
        w.mux_sel().clear_bit();
        w
    });


    if y_samples[0].abs_diff(y_samples[1]) > COMP {
        valid = false;
    }

    point.y = ((y_samples[0] as u32 + y_samples[1] as u32) / 2) as u16;

    // ==================================================
    // READ PRESSURE (Z)
    //
    // XP = LOW
    // YM = HIGH
    // read XM and YP
    // ==================================================

    {
        let _xp = Output::new(unsafe { XP::steal() }, Level::Low, Default::default());

        let _ym = Output::new(unsafe { YM::steal() }, Level::High, Default::default());

        let _yp = Input::new(unsafe { YP::steal() }, Default::default());

        let mut adc_config = AdcConfig::new();

        let mut xm = adc_config.enable_pin(unsafe { XM::steal() }, ATTENUATION);

        let mut yp = adc_config.enable_pin(unsafe { YP::steal() }, ATTENUATION);

        let mut adc = Adc::new(unsafe { ADC::steal() }, adc_config);

        esp_hal::delay::Delay::new().delay_micros(2_000);

        let z1: u16 = block!(adc.read_oneshot(&mut xm)).unwrap();

        let z2: u16 = block!(adc.read_oneshot(&mut yp)).unwrap();

        if z1 != 0 {
            let mut rtouch = z2 as f32;

            rtouch /= z1 as f32;
            rtouch -= 1.0;
            rtouch *= 4095.0 - point.x as f32;
            rtouch *= RXPLATE;
            rtouch /= 4095.0;

            point.z = rtouch as u16;
        }
    }

    rtcio.touch_pad0().modify(|_,w| {
        w.mux_sel().clear_bit();
        w
    });

    rtcio.touch_pad3().modify(|_,w| {
        w.mux_sel().clear_bit();
        w
    });

    if !valid {
        point.z = 0;
    }

    point
}

// ======================================================
// Touch helper
// ======================================================

impl TSPoint {
    pub fn to_pixel_point(&self) -> Option<(usize, usize)> {
        (self.z > PRESSURE_MIN).then(|| {
            (
                map(self.x, TS_MINX, TS_MAXX, 0, 240),
                map(self.y, TS_MINY, TS_MAXY, 0, 320)
            )
        })
    }
}

fn map(value: u16, min: usize, max: usize, map_min: usize, map_max: usize) -> usize {
    ((value as usize - min) * (map_max - map_min) / (max - min)) + map_min
}
