#![no_std]
#![no_main]

use rp2040_hal as hal;

use defmt_rtt as _;
use embedded_hal::{digital::v2::OutputPin, spi};
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
    ll::{blocking::vdm::W5500, Registers, Sn},
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

    let mut w5500 = {
        let mut chip_select = pins.gpio13.into_push_pull_output();
        chip_select.set_high().unwrap();
        let mut reset = pins.gpio11.into_push_pull_output();
        w5500_hl::ll::reset(&mut reset, &mut delay).unwrap();
        let _mosi = pins.gpio15.into_mode::<FunctionSpi>();
        let _sckl = pins.gpio14.into_mode::<FunctionSpi>();
        let _miso = pins.gpio12.into_mode::<FunctionSpi>();
        let spi_eth = Spi::<_, _, 8>::new(pac.SPI1).init(
            &mut pac.RESETS,
            clocks.peripheral_clock.freq(),
            10_000_000.Hz(),
            &spi::MODE_0,
        );
        W5500::new(spi_eth, chip_select)
    };
    assert_eq!(w5500.version().unwrap(), 0x04);

    const UDP_PORT: u16 = 1337;
    w5500.set_gar(&Ipv4Addr::new(192, 168, 0, 1)).unwrap();
    w5500
        .set_shar(&Eui48Addr::new(0x5E, 0x4C, 0x69, 0x67, 0x00, 0x03))
        .unwrap();
    w5500.set_sipr(&Ipv4Addr::new(192, 168, 0, 3)).unwrap();
    w5500.set_subr(&Ipv4Addr::new(255, 255, 255, 0)).unwrap();
    w5500.udp_bind(Sn::Sn0, UDP_PORT).unwrap();

    let mut request_buffer = [0u8; 2048];
    loop {
        let (len, sender) = block!(w5500.udp_recv_from(Sn::Sn0, &mut request_buffer)).unwrap();
        let request = &request_buffer[0..len.into()];
        defmt::info!(
            "Received from {}:{}\nData: {}\n",
            sender.ip().octets,
            sender.port(),
            request,
        );
        w5500.udp_send_to(Sn::Sn0, request, &sender).unwrap();
    }
}
