#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{AnyPin, Level, Output, Pin};
use embassy_rp::peripherals::USB;
use embassy_rp::spi;
use embassy_rp::usb::{self, Driver};
use embassy_time::Timer;
use embedded_hal::spi::SpiDevice;
use embedded_hal_bus::spi::ExclusiveDevice;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

pub type Spi = embassy_rp::peripherals::SPI0;
pub type SpiSck = embassy_rp::peripherals::PIN_2;
pub type SpiMosi = embassy_rp::peripherals::PIN_3;
pub type SpiRxDma = embassy_rp::peripherals::DMA_CH5;

pub struct SpiPins {
    pub sck: SpiSck,
    pub mosi: SpiMosi,
    pub cs: AnyPin,
}

#[embassy_executor::task]
pub async fn spi_task(pins: SpiPins, spi: Spi, rxdma: SpiRxDma) {
    let mut config = spi::Config::default();
    config.frequency = 2_000_000;
    let delay = embassy_time::Delay;

    let spi_bus = spi::Spi::new_txonly(spi, pins.sck, pins.mosi, rxdma, config);
    let cs = Output::new(pins.cs, Level::High);
    let mut spi_device = ExclusiveDevice::new(spi_bus, cs, delay.clone()).unwrap();
    let mut buf = [0_u8; 64];
    for i in 0..64 {
        buf[i] = i as u8;
    }
    loop {
        spi_device.write(&buf).unwrap();
        Timer::after_millis(1).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    log::info!("Start");
    let p = embassy_rp::init(Default::default());

    // USB Logger
    let driver = Driver::new(p.USB, Irqs);
    spawner.must_spawn(logger_task(driver));

    // Display task
    let spi_pins = SpiPins {
        sck: p.PIN_2,
        mosi: p.PIN_3,
        cs: p.PIN_1.degrade(),
    };
    spawner.must_spawn(spi_task(spi_pins, p.SPI0, p.DMA_CH5));

    let mut c = 0;
    loop {
        Timer::after_millis(1000).await;
        log::info!("Tick: {}", c);
        c += 1;
    }
}
