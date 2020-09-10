use core::{fmt::{self, Write}, mem::MaybeUninit};
use cortex_m::interrupt::free;
use stm32f4xx_hal::{
    otg_fs::{USB, UsbBus as Bus},
    stm32::{interrupt, Interrupt, NVIC},
};
use usb_device::{
    class_prelude::{UsbBusAllocator},
    prelude::{UsbError, UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_serial::SerialPort;
use log::{Record, Level, Log, Metadata};

static mut EP_MEMORY: [u32; 1024] = [0; 1024];

static mut BUS: MaybeUninit<UsbBusAllocator<Bus<USB>>> = MaybeUninit::uninit();
// static mut SERIAL_DEV: Option<(SerialPort<'static, Bus<USB>>, UsbDevice<'static, Bus<USB>>)> = None;
static mut STATE: Option<State> = None;

pub struct State {
    serial: SerialPort<'static, Bus<USB>>,
    dev: UsbDevice<'static, Bus<USB>>,
}

impl State {
    pub fn setup(usb: USB) {
        unsafe { BUS.write(Bus::new(usb, &mut EP_MEMORY)) };

        let bus = unsafe { BUS.assume_init_ref() };
        let serial = SerialPort::new(bus);
        let dev = UsbDeviceBuilder::new(bus, UsbVidPid(0x16c0, 0x27dd))
            .manufacturer("M-Labs")
            .product("thermostat")
            .device_release(0x20)
            .self_powered(true)
            .device_class(usbd_serial::USB_CLASS_CDC)
            .build();

        free(|_| {
            unsafe { STATE = Some(State { serial, dev }); }
        });

        unsafe {
            NVIC::unmask(Interrupt::OTG_FS);
        }
    }

    pub fn get() -> Option<&'static mut Self> {
        unsafe { STATE.as_mut() }
    }

    pub fn poll() {
        if let Some(ref mut s) = Self::get() {
            if s.dev.poll(&mut [&mut s.serial]) {
                // discard any input
                let mut buf = [0u8; 64];
                let _ = s.serial.read(&mut buf);
            }
        }
    }
}

#[interrupt]
fn OTG_FS() {
    free(|_| {
        State::poll();
    });
}

pub struct Logger;

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut output = SerialOutput;
            let _ = writeln!(&mut output, "{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {
        if let Some(ref mut state) = State::get() {
            let _ = free(|_| state.serial.flush());
        }
    }
}

pub struct SerialOutput;

impl Write for SerialOutput {
    fn write_str(&mut self, s: &str) -> core::result::Result<(), core::fmt::Error> {
        if let Some(ref mut state) = State::get() {
            for chunk in s.as_bytes().chunks(16) {
                free(|_| state.serial.write(chunk))
                    .map_err(|_| fmt::Error)?;
            }
        }
        Ok(())
    }
}
