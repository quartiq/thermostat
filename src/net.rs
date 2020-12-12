//! As there is only one peripheral, supporting data structures are
//! declared once and globally.

use core::cell::RefCell;
use cortex_m::interrupt::{CriticalSection, Mutex};
use stm32f4xx_hal::{
    rcc::Clocks,
    stm32::{interrupt, Peripherals, ETHERNET_MAC, ETHERNET_DMA},
};
use smoltcp::wire::{EthernetAddress, Ipv4Address, Ipv4Cidr};
use smoltcp::iface::{
    EthernetInterfaceBuilder, EthernetInterface,
    NeighborCache, Routes,
};
use stm32_eth::{Eth, RingEntry, PhyAddress, RxDescriptor, TxDescriptor};
use crate::command_parser::Ipv4Config;
use crate::pins::EthernetPins;

/// Not on the stack so that stack can be placed in CCMRAM (which the
/// ethernet peripheral cannot access)
static mut RX_RING: Option<[RingEntry<RxDescriptor>; 8]> = None;
/// Not on the stack so that stack can be placed in CCMRAM (which the
/// ethernet peripheral cannot access)
static mut TX_RING: Option<[RingEntry<TxDescriptor>; 2]> = None;

/// Interrupt pending flag: set by the `ETH` interrupt handler, should
/// be cleared before polling the interface.
static NET_PENDING: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

/// Run callback `f` with ethernet driver and TCP/IP stack
pub fn run<F>(
    clocks: Clocks,
    ethernet_mac: ETHERNET_MAC, ethernet_dma: ETHERNET_DMA,
    eth_pins: EthernetPins,
    ethernet_addr: EthernetAddress,
    ipv4_config: Ipv4Config,
    f: F
) where
    F: FnOnce(EthernetInterface<&mut stm32_eth::Eth<'static, 'static>>),
{
    let rx_ring = unsafe {
        RX_RING.get_or_insert(Default::default())
    };
    let tx_ring = unsafe {
        TX_RING.get_or_insert(Default::default())
    };
    // Ethernet driver
    let mut eth_dev = Eth::new(
        ethernet_mac, ethernet_dma,
        &mut rx_ring[..], &mut tx_ring[..],
        PhyAddress::_0,
        clocks,
        eth_pins,
    ).unwrap();
    eth_dev.enable_interrupt();

    // IP stack
    let (ipv4_cidr, gateway) = split_ipv4_config(ipv4_config);
    let mut ip_addrs = [ipv4_cidr.into()];
    let mut neighbor_storage = [None; 16];
    let neighbor_cache = NeighborCache::new(&mut neighbor_storage[..]);
    let mut routes_storage = [None; 1];
    let mut routes = Routes::new(&mut routes_storage[..]);
    gateway.map(|gateway| routes.add_default_ipv4_route(gateway).unwrap());
    let iface = EthernetInterfaceBuilder::new(&mut eth_dev)
        .ethernet_addr(ethernet_addr)
        .ip_addrs(&mut ip_addrs[..])
        .neighbor_cache(neighbor_cache)
        .routes(routes)
        .finalize();

    f(iface);
}

/// Potentially wake up from `wfi()`, set the interrupt pending flag,
/// clear interrupt flags.
#[interrupt]
fn ETH() {
    cortex_m::interrupt::free(|cs| {
        *NET_PENDING.borrow(cs)
            .borrow_mut() = true;
    });

    let p = unsafe { Peripherals::steal() };
    stm32_eth::eth_interrupt_handler(&p.ETHERNET_DMA);
}

/// Has an interrupt occurred since last call to `clear_pending()`?
pub fn is_pending(cs: &CriticalSection) -> bool {
    *NET_PENDING.borrow(cs)
        .borrow()
}

/// Clear the interrupt pending flag before polling the interface for
/// data.
pub fn clear_pending(cs: &CriticalSection) {
    *NET_PENDING.borrow(cs)
        .borrow_mut() = false;
}

/// utility for destructuring into smoltcp types
pub fn split_ipv4_config(config: Ipv4Config) -> (Ipv4Cidr, Option<Ipv4Address>) {
    let cidr = Ipv4Cidr::new(Ipv4Address(config.address), config.mask_len);
    let gateway = config.gateway.map(Ipv4Address);
    (cidr, gateway)
}
