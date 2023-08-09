#![no_std]
#![no_main]

use rp2040_hal as hal;

use defmt_rtt as _;
use embedded_hal::{
    digital::v2::{InputPin, OutputPin},
    spi,
};
use fugit::RateExtU32;
use hal::{
    clocks::{init_clocks_and_plls, Clock},
    entry,
    gpio::FunctionSpi,
    pac,
    sio::Sio,
    spi::Spi,
    watchdog::Watchdog,
};
use panic_probe as _;
use w5500_hl::{
    block,
    ll::{eh0::vdm::W5500, Registers, Sn, SocketInterrupt},
    net::{Eui48Addr, Ipv4Addr},
    Udp,
};

#[link_section = ".boot_loader"]
#[used]
pub static BOOT_LOADER: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    const XTAL_FREQ_HZ: u32 = 12_000_000;
    let clocks = init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    let mut int_pin = pins.gpio21.into_floating_input();

    let mut w5500 = {
        let mut chip_select = pins.gpio17.into_push_pull_output();
        chip_select.set_high().unwrap();
        let mut reset = pins.gpio20.into_push_pull_output();
        w5500_hl::ll::eh0::reset(&mut reset, &mut delay).unwrap();
        let _mosi = pins.gpio19.into_mode::<FunctionSpi>();
        let _sckl = pins.gpio18.into_mode::<FunctionSpi>();
        let _miso = pins.gpio16.into_mode::<FunctionSpi>();
        let spi_eth = Spi::<_, _, 8>::new(pac.SPI0).init(
            &mut pac.RESETS,
            clocks.peripheral_clock.freq(),
            10_000_000.Hz(),
            &spi::MODE_0,
        );
        W5500::new(spi_eth, chip_select)
    };
    assert_eq!(w5500.version().unwrap(), 0x04);

    const SN: Sn = Sn::Sn0;
    const UDP_PORT: u16 = 1337;
    w5500.set_gar(&Ipv4Addr::new(10, 0, 0, 1)).unwrap();
    w5500
        .set_shar(&Eui48Addr::new(0x5E, 0x4C, 0x69, 0x67, 0x00, 0x03))
        .unwrap();
    w5500.set_sipr(&Ipv4Addr::new(10, 0, 0, 67)).unwrap();
    w5500.set_subr(&Ipv4Addr::new(255, 255, 255, 0)).unwrap();
    w5500.udp_bind(SN, UDP_PORT).unwrap();
    w5500.set_simr(SN.bitmask()).unwrap();

    let mut request_buffer = [0u8; 2048];
    loop {
        // int_pin.is_low().unwrap()
        if let Ok((len, sender)) = w5500.udp_recv_from(SN, &mut request_buffer) {
            let request = &request_buffer[0..len.into()];
            defmt::info!(
                "Received from {}:{}\nData: {}\n",
                sender.ip().octets(),
                sender.port(),
                request,
            );
        } else {
            delay.delay_ms(1000);
        }
    }
}
