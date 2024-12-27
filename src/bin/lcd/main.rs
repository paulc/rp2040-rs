#![no_std]
#![no_main]

use display_interface_spi::SPIInterface;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{AnyPin, Level, Output, Pin};
use embassy_rp::peripherals::USB;
use embassy_rp::spi;
use embassy_rp::usb::{self, Driver};
use embassy_time::Timer;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*};
use embedded_hal_bus::spi::ExclusiveDevice;
use mipidsi::TestImage;
use mipidsi::{models::ST7735s, Builder};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

pub type DisplaySpi = embassy_rp::peripherals::SPI0;
pub type DisplaySpiSck = embassy_rp::peripherals::PIN_2;
pub type DisplaySpiMosi = embassy_rp::peripherals::PIN_3;
pub type DisplaySpiRxDma = embassy_rp::peripherals::DMA_CH5;

pub struct DisplayPins {
    pub sck: DisplaySpiSck,
    pub mosi: DisplaySpiMosi,
    pub dc: AnyPin,
    pub cs: AnyPin,
    pub reset: AnyPin,
    pub backlight: AnyPin,
}

#[embassy_executor::task]
pub async fn display(pins: DisplayPins, spi: DisplaySpi, rxdma: DisplaySpiRxDma) {
    let mut config = spi::Config::default();
    config.frequency = 64_000_000;

    let mut delay = embassy_time::Delay;

    let spi_bus = spi::Spi::new_txonly(spi, pins.sck, pins.mosi, rxdma, config);

    let lcd_dc = Output::new(pins.dc, Level::Low);
    let lcd_cs = Output::new(pins.cs, Level::High);
    let lcd_reset = Output::new(pins.reset, Level::Low);
    let mut lcd_backlight = Output::new(pins.backlight, Level::High);

    let spi_device = ExclusiveDevice::new(spi_bus, lcd_cs, delay.clone()).unwrap();
    let display_interface = SPIInterface::new(spi_device, lcd_dc);

    let mut display = Builder::new(ST7735s, display_interface)
        .reset_pin(lcd_reset)
        .init(&mut delay)
        .unwrap();

    log::info!("Starting Display");
    lcd_backlight.set_high();
    TestImage::new().draw(&mut display).unwrap();
    Timer::after_millis(2000).await;

    loop {
        for c in [Rgb565::RED, Rgb565::BLUE, Rgb565::GREEN] {
            log::info!("Display Clear....");
            display.clear(c).ok();
            log::info!("Display Done....");
            Timer::after_millis(1000).await;
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

    // Display task
    let display_pins = DisplayPins {
        sck: p.PIN_2,
        mosi: p.PIN_3,
        dc: p.PIN_8.degrade(),
        cs: p.PIN_1.degrade(),
        reset: p.PIN_6.degrade(),
        backlight: p.PIN_7.degrade(),
    };
    spawner.must_spawn(display(display_pins, p.SPI0, p.DMA_CH5));

    let mut c = 0;
    loop {
        Timer::after_millis(1000).await;
        log::info!("Tick: {}", c);
        c += 1;
    }
}
