#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIO0, PIO1, USB};
use embassy_rp::pio::{self, Pio};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::usb::{self, Driver};
use embassy_time::{Duration, Ticker, Timer};
use smart_leds::RGB8;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    PIO1_IRQ_0 => pio::InterruptHandler<PIO1>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

const NUM_LEDS: usize = 8;
const STATE_MACHINE: usize = 0;
type Ws2812Strip<'a> = PioWs2812<'a, PIO0, STATE_MACHINE, NUM_LEDS>;

/// Input a value 0 to 255 to get a color value
/// The colours are a transition r - g - b - back to r.
fn colour_wheel(mut wheel_pos: u8) -> RGB8 {
    wheel_pos = 255 - wheel_pos;
    if wheel_pos < 85 {
        return (255 - wheel_pos * 3, 0, wheel_pos * 3).into();
    }
    if wheel_pos < 170 {
        wheel_pos -= 85;
        return (0, wheel_pos * 3, 255 - wheel_pos * 3).into();
    }
    wheel_pos -= 170;
    (wheel_pos * 3, 255 - wheel_pos * 3, 0).into()
}

fn set_brightness(c: RGB8, scale: f32) -> RGB8 {
    RGB8::new(
        (c.r as f32 * scale.clamp(0.0, 1.0)) as u8,
        (c.g as f32 * scale.clamp(0.0, 1.0)) as u8,
        (c.b as f32 * scale.clamp(0.0, 1.0)) as u8,
    )
}

fn wrap(index: usize, offset: i32, max: usize) -> usize {
    let pos = index as i32 + offset;
    if pos < 0 {
        (max as i32 + pos) as usize
    } else if pos < max as i32 {
        pos as usize
    } else {
        pos as usize % max
    }
}

type Ws2812PioDevice = embassy_rp::peripherals::PIO0;
type Ws2812DMADevice = embassy_rp::peripherals::DMA_CH0;
type Ws2812Pin = embassy_rp::peripherals::PIN_14;

#[embassy_executor::task]
async fn chase(pio: Ws2812PioDevice, dma: Ws2812DMADevice, pin: Ws2812Pin, delay: u64) {
    let Pio {
        mut common, sm0, ..
    } = Pio::new(pio, Irqs);

    let program = PioWs2812Program::new(&mut common);
    let mut ws2812: Ws2812Strip = PioWs2812::new(&mut common, sm0, dma, pin, &program);

    let mut index = 0;
    let mut data = [RGB8::default(); NUM_LEDS];
    let mut ticker = Ticker::every(Duration::from_millis(delay));
    let mut c = 0_u8;
    loop {
        let colour = set_brightness(colour_wheel(c), 0.5);
        log::debug!("CHASE COLOUR --> {:?}", colour);
        data[index] = colour;
        for offset in 1..NUM_LEDS {
            let i = wrap(index, -(offset as i32), NUM_LEDS);
            data[i] = set_brightness(data[i], 0.5);
        }
        ws2812.write(&data).await;
        index = wrap(index, 1, NUM_LEDS);
        ticker.next().await;
        c = c + 4;
    }
}

type InternalWs2812PioDevice = embassy_rp::peripherals::PIO1;
type InternalWs2812DMADevice = embassy_rp::peripherals::DMA_CH1;
type InternalWs2812Pin = embassy_rp::peripherals::PIN_16;

#[embassy_executor::task]
async fn wheel(
    pio: InternalWs2812PioDevice,
    dma: InternalWs2812DMADevice,
    pin: InternalWs2812Pin,
    brightness: f32,
) {
    let Pio {
        mut common, sm0, ..
    } = Pio::new(pio, Irqs);

    let program = PioWs2812Program::new(&mut common);
    let mut ws2812 = PioWs2812::new(&mut common, sm0, dma, pin, &program);

    const N: usize = 1;
    let mut data = [RGB8::default(); N];
    let mut ticker = Ticker::every(Duration::from_millis(10));
    loop {
        for j in 0..(256 * 5) {
            for i in 0..N {
                data[i] = set_brightness(
                    colour_wheel((((i * 256) as u16 / N as u16 + j as u16) & 255) as u8),
                    brightness,
                );
            }
            ws2812.write(&data).await;
            ticker.next().await;
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    log::info!("Start");
    let p = embassy_rp::init(Default::default());

    // USB Logger
    let driver = Driver::new(p.USB, Irqs);
    spawner.must_spawn(logger_task(driver));

    // WS2812 tasks
    spawner.must_spawn(chase(p.PIO0, p.DMA_CH0, p.PIN_14, 100));
    spawner.must_spawn(wheel(p.PIO1, p.DMA_CH1, p.PIN_16, 0.5));

    let mut c = 0;
    loop {
        Timer::after_millis(1000).await;
        log::info!("Tick: {}", c);
        c += 1;
    }
}
