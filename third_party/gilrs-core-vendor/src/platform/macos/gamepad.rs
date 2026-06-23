// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use super::io_kit::*;
use super::FfDevice;
use crate::{AxisInfo, Event, EventType, PlatformError, PowerInfo};
use uuid::Uuid;

use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use io_kit_sys::hid::base::{IOHIDDeviceRef, IOHIDValueRef};
use io_kit_sys::hid::usage_tables::{
    kHIDPage_GenericDesktop, kHIDUsage_GD_GamePad, kHIDUsage_GD_Joystick,
    kHIDUsage_GD_MultiAxisController,
};
use io_kit_sys::ret::IOReturn;
use vec_map::VecMap;

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::os::raw::c_void;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub struct Gilrs {
    gamepads: Vec<Gamepad>,
    device_infos: Arc<Mutex<Vec<DeviceInfo>>>,
    rx: Receiver<(Event, Option<IOHIDDevice>)>,
}

impl Gilrs {
    pub(crate) fn new() -> Result<Self, PlatformError> {
        let gamepads = Vec::new();
        let device_infos = Arc::new(Mutex::new(Vec::new()));

        let (tx, rx) = mpsc::channel();
        Self::spawn_thread(tx, device_infos.clone());

        Ok(Gilrs {
            gamepads,
            device_infos,
            rx,
        })
    }

    fn spawn_thread(
        tx: Sender<(Event, Option<IOHIDDevice>)>,
        device_infos: Arc<Mutex<Vec<DeviceInfo>>>,
    ) {
        thread::Builder::new()
            .name("gilrs".to_owned())
            .spawn(move || unsafe {
                let mut manager = match IOHIDManager::new() {
                    Some(manager) => manager,
                    None => {
                        error!("Failed to create IOHIDManager object");
                        return;
                    }
                };

                manager.schedule_with_run_loop(CFRunLoop::get_current(), kCFRunLoopDefaultMode);

                let context = &(tx.clone(), device_infos.clone()) as *const _ as *mut c_void;
                manager.register_device_matching_callback(device_matching_cb, context);

                let context = &(tx.clone(), device_infos.clone()) as *const _ as *mut c_void;
                manager.register_device_removal_callback(device_removal_cb, context);

                let context = &(tx, device_infos) as *const _ as *mut c_void;
                manager.register_input_value_callback(input_value_cb, context);

                CFRunLoop::run_current();

                manager.unschedule_from_run_loop(CFRunLoop::get_current(), kCFRunLoopDefaultMode);
            })
            .expect("failed to spawn thread");
    }

    pub(crate) fn next_event(&mut self) -> Option<Event> {
        let event = self.rx.try_recv().ok();
        self.handle_event(event)
    }

    pub(crate) fn next_event_blocking(&mut self, timeout: Option<Duration>) -> Option<Event> {
        let event = if let Some(timeout) = timeout {
            self.rx.recv_timeout(timeout).ok()
        } else {
            self.rx.recv().ok()
        };

        self.handle_event(event)
    }

    fn handle_event(&mut self, event: Option<(Event, Option<IOHIDDevice>)>) -> Option<Event> {
        match event {
            Some((event, Some(device))) => {
                if event.event == EventType::Connected {
                    if self.gamepads.get(event.id).is_some() {
                        self.gamepads[event.id].is_connected = true;
                    } else {
                        match Gamepad::open(device) {
                            Some(gamepad) => {
                                self.gamepads.push(gamepad);
                            }
                            None => {
                                error!("Failed to open gamepad: {:?}", event.id);
                                return None;
                            }
                        };
                    }
                }
                Some(event)
            }
            Some((event, None)) => {
                if event.event == EventType::Disconnected {
                    match self.gamepads.get_mut(event.id) {
                        Some(gamepad) => {
                            match self.device_infos.lock().unwrap().get_mut(event.id) {
                                Some(device_info) => device_info.is_connected = false,
                                None => {
                                    error!("Failed to find device_info: {:?}", event.id);
                                    return None;
                                }
                            };
                            gamepad.is_connected = false;
                        }
                        None => {
                            error!("Failed to find gamepad: {:?}", event.id);
                            return None;
                        }
                    }
                }
                Some(event)
            }
            None => None,
        }
    }

    pub fn gamepad(&self, id: usize) -> Option<&Gamepad> {
        self.gamepads.get(id)
    }

    /// Returns index greater than index of last connected gamepad.
    pub fn last_gamepad_hint(&self) -> usize {
        self.gamepads.len()
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Gamepad {
    name: String,
    vendor: Option<u16>,
    product: Option<u16>,
    uuid: Uuid,
    entry_id: u64,
    location_id: u32,
    page: u32,
    usage: u32,
    axes_info: VecMap<AxisInfo>,
    axes: Vec<EvCode>,
    hats: Vec<EvCode>,
    buttons: Vec<EvCode>,
    is_connected: bool,
}

impl Gamepad {
    fn open(device: IOHIDDevice) -> Option<Gamepad> {
        let io_service = match device.get_service() {
            Some(io_service) => io_service,
            None => {
                error!("Failed to get device service");
                return None;
            }
        };

        let entry_id = match io_service.get_registry_entry_id() {
            Some(entry_id) => entry_id,
            None => {
                error!("Failed to get entry id of device");
                return None;
            }
        };

        let location_id = match device.get_location_id() {
            Some(location_id) => location_id,
            None => {
                error!("Failed to get location id of device");
                return None;
            }
        };

        let page = match device.get_page() {
            Some(page) => {
                if page == kHIDPage_GenericDesktop {
                    page
                } else {
                    error!("Failed to get valid device: {:?}", page);
                    return None;
                }
            }
            None => {
                error!("Failed to get page of device");
                return None;
            }
        };

        let usage = match device.get_usage() {
            Some(usage) => {
                if usage == kHIDUsage_GD_GamePad
                    || usage == kHIDUsage_GD_Joystick
                    || usage == kHIDUsage_GD_MultiAxisController
                {
                    usage
                } else {
                    error!("Failed to get valid device: {:?}", usage);
                    return None;
                }
            }
            None => {
                error!("Failed to get usage of device");
                return None;
            }
        };

        let name = device.get_name().unwrap_or_else(|| {
            warn!("Failed to get name of device");
            "Unknown".into()
        });

        let uuid = match Self::create_uuid(&device) {
            Some(uuid) => uuid,
            None => Uuid::nil(),
        };

        let mut gamepad = Gamepad {
            name,
            vendor: device.get_vendor_id(),
            product: device.get_product_id(),
            uuid,
            entry_id,
            location_id,
            page,
            usage,
            axes_info: VecMap::with_capacity(8),
            axes: Vec::with_capacity(8),
            hats: Vec::with_capacity(4),
            buttons: Vec::with_capacity(16),
            is_connected: true,
        };
        gamepad.collect_axes_and_buttons(&device.get_elements());

        Some(gamepad)
    }

    fn create_uuid(device: &IOHIDDevice) -> Option<Uuid> {
        // SDL always uses USB bus for UUID
        let bustype = u32::to_be(0x03);

        let vendor_id = match device.get_vendor_id() {
            Some(vendor_id) => vendor_id.to_be(),
            None => {
                warn!("Failed to get vendor id of device");
                0
            }
        };

        let product_id = match device.get_product_id() {
            Some(product_id) => product_id.to_be(),
            None => {
                warn!("Failed to get product id of device");
                0
            }
        };

        let version = match device.get_version() {
            Some(version) => version.to_be(),
            None => {
                warn!("Failed to get version of device");
                0
            }
        };

        if vendor_id == 0 && product_id == 0 && version == 0 {
            None
        } else {
            Some(Uuid::from_fields(
                bustype,
                vendor_id,
                0,
                &[
                    (product_id >> 8) as u8,
                    product_id as u8,
                    0,
                    0,
                    (version >> 8) as u8,
                    version as u8,
                    0,
                    0,
                ],
            ))
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn vendor_id(&self) -> Option<u16> {
        self.vendor
    }

    pub fn product_id(&self) -> Option<u16> {
        self.product
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub fn power_info(&self) -> PowerInfo {
        PowerInfo::Unknown
    }

    pub fn is_ff_supported(&self) -> bool {
        false
    }

    /// Creates Ffdevice corresponding to this gamepad.
    pub fn ff_device(&self) -> Option<FfDevice> {
        Some(FfDevice)
    }

    pub fn buttons(&self) -> &[EvCode] {
        &self.buttons
    }

    pub fn axes(&self) -> &[EvCode] {
        &self.axes
    }

    pub(crate) fn axis_info(&self, nec: EvCode) -> Option<&AxisInfo> {
        self.axes_info.get(nec.usage as usize)
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn collect_axes_and_buttons(&mut self, elements: &Vec<IOHIDElement>) {
        let mut cookies = Vec::new();

        self.collect_axes(&elements, &mut cookies);
        self.axes.sort_by_key(|axis| axis.usage);
        self.hats.sort_by_key(|axis| axis.usage);
        // Because "hat is axis" is gilrs thing, we want ensure that all hats are at the end of
        // axis vector, so SDL mappings still works.
        self.axes.extend(&self.hats);

        self.collect_buttons(&elements, &mut cookies);
        self.buttons.sort_by_key(|button| button.usage);
    }

    fn collect_axes(&mut self, elements: &Vec<IOHIDElement>, cookies: &mut Vec<u32>) {
        for element in elements {
            let type_ = element.get_type();
            let cookie = element.get_cookie();
            let page = element.get_page();
            let usage = element.get_usage();

            if IOHIDElement::is_collection_type(type_) {
                let children = element.get_children();
                self.collect_axes(&children, cookies);
            } else if IOHIDElement::is_axis(type_, page, usage) && !cookies.contains(&cookie) {
                cookies.push(cookie);
                self.axes_info.insert(
                    usage as usize,
                    AxisInfo {
                        min: element.get_logical_min() as _,
                        max: element.get_logical_max() as _,
                        deadzone: None,
                    },
                );
                self.axes.push(EvCode::new(page, usage));
            } else if IOHIDElement::is_hat(type_, page, usage) && !cookies.contains(&cookie) {
                cookies.push(cookie);
                self.axes_info.insert(
                    usage as usize,
                    AxisInfo {
                        min: -1,
                        max: 1,
                        deadzone: None,
                    },
                );
                self.hats.push(EvCode::new(page, usage));
                // All hat switches are translated into *two* axes
                self.axes_info.insert(
                    (usage + 1) as usize, // "+ 1" is assumed for usage of 2nd hat switch axis
                    AxisInfo {
                        min: -1,
                        max: 1,
                        deadzone: None,
                    },
                );
                self.hats.push(EvCode::new(page, usage + 1));
            }
        }
    }

    fn collect_buttons(&mut self, elements: &Vec<IOHIDElement>, cookies: &mut Vec<u32>) {
        for element in elements {
            let type_ = element.get_type();
            let cookie = element.get_cookie();
            let page = element.get_page();
            let usage = element.get_usage();

            if IOHIDElement::is_collection_type(type_) {
                let children = element.get_children();
                self.collect_buttons(&children, cookies);
            } else if IOHIDElement::is_button(type_, page, usage) && !cookies.contains(&cookie) {
                cookies.push(cookie);
                self.buttons.push(EvCode::new(page, usage));
            }
        }
    }
}

#[derive(Debug)]
struct DeviceInfo {
    entry_id: u64,
    location_id: u32,
    is_connected: bool,
}

const SONY_VENDOR_ID: u16 = 0x054c;
const DUALSENSE_PRODUCT_ID: u16 = 0x0ce6;
const DUALSENSE_BT_REPORT_ID: u32 = 0x31;
const DUALSENSE_BT_REPORT_LEN: usize = 78;
const DUALSENSE_BT_REPORT_BUFFER_LEN: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DualSenseBtState {
    lx: u8,
    ly: u8,
    rx: u8,
    ry: u8,
    l2: u8,
    r2: u8,
    buttons: u32,
}

struct SonyBtReportContext {
    tx: Sender<(Event, Option<IOHIDDevice>)>,
    id: usize,
    previous: Option<DualSenseBtState>,
    report: [u8; DUALSENSE_BT_REPORT_BUFFER_LEN],
}
#[cfg(feature = "serde-serialize")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EvCode {
    page: u32,
    usage: u32,
}

impl EvCode {
    fn new(page: u32, usage: u32) -> Self {
        EvCode { page, usage }
    }

    pub fn into_u32(self) -> u32 {
        self.page << 16 | self.usage
    }
}

impl From<IOHIDElement> for crate::EvCode {
    fn from(e: IOHIDElement) -> Self {
        crate::EvCode(EvCode {
            page: e.get_page(),
            usage: e.get_usage(),
        })
    }
}

impl Display for EvCode {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self.page {
            PAGE_GENERIC_DESKTOP => f.write_str("GENERIC_DESKTOP")?,
            PAGE_BUTTON => f.write_str("BUTTON")?,
            page => f.write_fmt(format_args!("PAGE_{}", page))?,
        }
        f.write_fmt(format_args!("({})", self.usage))
    }
}

pub mod native_ev_codes {
    use super::*;

    pub const AXIS_LSTICKX: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_LSTICKX,
    };
    pub const AXIS_LSTICKY: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_LSTICKY,
    };
    pub const AXIS_LEFTZ: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_LEFTZ,
    };
    pub const AXIS_RSTICKX: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_RSTICKX,
    };
    pub const AXIS_RSTICKY: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_RSTICKY,
    };
    pub const AXIS_RIGHTZ: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_RIGHTZ,
    };
    pub const AXIS_DPADX: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_DPADX,
    };
    pub const AXIS_DPADY: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_DPADY,
    };
    pub const AXIS_RT: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_RT,
    };
    pub const AXIS_LT: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_LT,
    };
    pub const AXIS_RT2: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_RT2,
    };
    pub const AXIS_LT2: EvCode = EvCode {
        page: super::PAGE_GENERIC_DESKTOP,
        usage: super::USAGE_AXIS_LT2,
    };

    pub const BTN_SOUTH: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_SOUTH,
    };
    pub const BTN_EAST: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_EAST,
    };
    pub const BTN_C: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_C,
    };
    pub const BTN_NORTH: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_NORTH,
    };
    pub const BTN_WEST: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_WEST,
    };
    pub const BTN_Z: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_Z,
    };
    pub const BTN_LT: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_LT,
    };
    pub const BTN_RT: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_RT,
    };
    pub const BTN_LT2: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_LT2,
    };
    pub const BTN_RT2: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_RT2,
    };
    pub const BTN_SELECT: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_SELECT,
    };
    pub const BTN_START: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_START,
    };
    pub const BTN_MODE: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_MODE,
    };
    pub const BTN_LTHUMB: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_LTHUMB,
    };
    pub const BTN_RTHUMB: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_RTHUMB,
    };

    pub const BTN_DPAD_UP: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_DPAD_UP,
    };
    pub const BTN_DPAD_DOWN: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_DPAD_DOWN,
    };
    pub const BTN_DPAD_LEFT: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_DPAD_LEFT,
    };
    pub const BTN_DPAD_RIGHT: EvCode = EvCode {
        page: super::PAGE_BUTTON,
        usage: super::USAGE_BTN_DPAD_RIGHT,
    };
}

fn register_sony_bt_report_callback(
    device: &IOHIDDevice,
    id: usize,
    tx: &Sender<(Event, Option<IOHIDDevice>)>,
) {
    let vendor = device.get_vendor_id();
    let product = device.get_product_id();
    let transport = device.get_transport_key();
    if vendor != Some(SONY_VENDOR_ID)
        || product != Some(DUALSENSE_PRODUCT_ID)
        || transport.as_deref() != Some("Bluetooth")
    {
        return;
    }

    let mut context = Box::new(SonyBtReportContext {
        tx: tx.clone(),
        id,
        previous: None,
        report: [0; DUALSENSE_BT_REPORT_BUFFER_LEN],
    });
    let report = context.report.as_mut_ptr();
    let report_length = context.report.len() as _;
    let context = Box::into_raw(context) as *mut c_void;
    device.register_input_report_callback(report, report_length, sony_bt_input_report_cb, context);
}

unsafe extern "C" fn sony_bt_input_report_cb(
    context: *mut c_void,
    result: IOReturn,
    _sender: *mut c_void,
    _type: u32,
    report_id: u32,
    report: *mut u8,
    report_len: isize,
) {
    if result != 0 || context.is_null() || report.is_null() || report_len <= 0 {
        return;
    }
    let context = &mut *(context as *mut SonyBtReportContext);
    let report = std::slice::from_raw_parts(report as *const u8, report_len as usize);
    let Some(next) = parse_dualsense_bt_report(report_id, report) else {
        return;
    };

    let Some(previous) = context.previous.replace(next) else {
        return;
    };

    send_axis_if_changed(
        &context.tx,
        context.id,
        previous.lx,
        next.lx,
        USAGE_AXIS_LSTICKX,
    );
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous.ly,
        next.ly,
        USAGE_AXIS_LSTICKY,
    );
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous.rx,
        next.rx,
        USAGE_AXIS_RT2,
    );
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous.ry,
        next.ry,
        USAGE_AXIS_LT2,
    );
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous.l2,
        next.l2,
        USAGE_AXIS_RSTICKX,
    );
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous.r2,
        next.r2,
        USAGE_AXIS_RSTICKY,
    );

    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_SOUTH,
        USAGE_BTN_SOUTH,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_EAST,
        USAGE_BTN_EAST,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_NORTH,
        USAGE_BTN_NORTH,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_WEST,
        USAGE_BTN_WEST,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_LT,
        USAGE_BTN_LT,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_RT,
        USAGE_BTN_RT,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_LT2,
        USAGE_BTN_LT2,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_RT2,
        USAGE_BTN_RT2,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_SELECT,
        USAGE_BTN_SELECT,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_START,
        USAGE_BTN_START,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_LTHUMB,
        USAGE_BTN_LTHUMB,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_RTHUMB,
        USAGE_BTN_RTHUMB,
    );
    send_button_if_changed(
        &context.tx,
        context.id,
        previous.buttons,
        next.buttons,
        DUALSENSE_BUTTON_MODE,
        USAGE_BTN_MODE,
    );
    let (previous_dpad_x, previous_dpad_y) = dualsense_dpad_axis_values(previous.buttons);
    let (next_dpad_x, next_dpad_y) = dualsense_dpad_axis_values(next.buttons);
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous_dpad_x,
        next_dpad_x,
        USAGE_AXIS_DPADX,
    );
    send_axis_if_changed(
        &context.tx,
        context.id,
        previous_dpad_y,
        next_dpad_y,
        USAGE_AXIS_DPADY,
    );
}

fn send_axis_if_changed<T>(
    tx: &Sender<(Event, Option<IOHIDDevice>)>,
    id: usize,
    previous: T,
    next: T,
    usage: u32,
) where
    T: Copy + Eq + Into<i32>,
{
    if previous == next {
        return;
    }
    let _ = tx.send((
        Event::new(
            id,
            EventType::AxisValueChanged(
                next.into(),
                crate::EvCode(EvCode {
                    page: PAGE_GENERIC_DESKTOP,
                    usage,
                }),
            ),
        ),
        None,
    ));
}

fn send_button_if_changed(
    tx: &Sender<(Event, Option<IOHIDDevice>)>,
    id: usize,
    previous: u32,
    next: u32,
    mask: u32,
    usage: u32,
) {
    let previous_pressed = previous & mask != 0;
    let next_pressed = next & mask != 0;
    if previous_pressed == next_pressed {
        return;
    }
    let event = if next_pressed {
        EventType::ButtonPressed(crate::EvCode(EvCode {
            page: PAGE_BUTTON,
            usage,
        }))
    } else {
        EventType::ButtonReleased(crate::EvCode(EvCode {
            page: PAGE_BUTTON,
            usage,
        }))
    };
    let _ = tx.send((Event::new(id, event), None));
}

const DUALSENSE_BUTTON_SOUTH: u32 = 1 << 0;
const DUALSENSE_BUTTON_EAST: u32 = 1 << 1;
const DUALSENSE_BUTTON_NORTH: u32 = 1 << 2;
const DUALSENSE_BUTTON_WEST: u32 = 1 << 3;
const DUALSENSE_BUTTON_LT: u32 = 1 << 4;
const DUALSENSE_BUTTON_RT: u32 = 1 << 5;
const DUALSENSE_BUTTON_LT2: u32 = 1 << 6;
const DUALSENSE_BUTTON_RT2: u32 = 1 << 7;
const DUALSENSE_BUTTON_SELECT: u32 = 1 << 8;
const DUALSENSE_BUTTON_START: u32 = 1 << 9;
const DUALSENSE_BUTTON_LTHUMB: u32 = 1 << 10;
const DUALSENSE_BUTTON_RTHUMB: u32 = 1 << 11;
const DUALSENSE_BUTTON_MODE: u32 = 1 << 12;
const DUALSENSE_BUTTON_DPAD_UP: u32 = 1 << 13;
const DUALSENSE_BUTTON_DPAD_DOWN: u32 = 1 << 14;
const DUALSENSE_BUTTON_DPAD_LEFT: u32 = 1 << 15;
const DUALSENSE_BUTTON_DPAD_RIGHT: u32 = 1 << 16;

fn dualsense_dpad_axis_values(buttons: u32) -> (i32, i32) {
    let left = buttons & DUALSENSE_BUTTON_DPAD_LEFT != 0;
    let right = buttons & DUALSENSE_BUTTON_DPAD_RIGHT != 0;
    let up = buttons & DUALSENSE_BUTTON_DPAD_UP != 0;
    let down = buttons & DUALSENSE_BUTTON_DPAD_DOWN != 0;

    let x = match (left, right) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };
    let y = match (up, down) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };

    (x, y)
}

fn parse_dualsense_bt_report(report_id: u32, report: &[u8]) -> Option<DualSenseBtState> {
    if report_id != DUALSENSE_BT_REPORT_ID || report.len() < DUALSENSE_BT_REPORT_LEN {
        return None;
    }
    let offset = if report.first().copied() == Some(DUALSENSE_BT_REPORT_ID as u8) {
        1
    } else {
        0
    };
    if report.len() < offset + 11 {
        return None;
    }

    let face_and_dpad = report[offset + 8];
    let shoulders = report[offset + 9];
    let system = report[offset + 10];
    let dpad = face_and_dpad & 0x0f;
    let mut buttons = 0;
    if face_and_dpad & 0x20 != 0 {
        buttons |= DUALSENSE_BUTTON_SOUTH;
    }
    if face_and_dpad & 0x40 != 0 {
        buttons |= DUALSENSE_BUTTON_EAST;
    }
    if face_and_dpad & 0x80 != 0 {
        buttons |= DUALSENSE_BUTTON_NORTH;
    }
    if face_and_dpad & 0x10 != 0 {
        buttons |= DUALSENSE_BUTTON_WEST;
    }
    if shoulders & 0x01 != 0 {
        buttons |= DUALSENSE_BUTTON_LT;
    }
    if shoulders & 0x02 != 0 {
        buttons |= DUALSENSE_BUTTON_RT;
    }
    if shoulders & 0x04 != 0 {
        buttons |= DUALSENSE_BUTTON_LT2;
    }
    if shoulders & 0x08 != 0 {
        buttons |= DUALSENSE_BUTTON_RT2;
    }
    if shoulders & 0x10 != 0 {
        buttons |= DUALSENSE_BUTTON_SELECT;
    }
    if shoulders & 0x20 != 0 {
        buttons |= DUALSENSE_BUTTON_START;
    }
    if shoulders & 0x40 != 0 {
        buttons |= DUALSENSE_BUTTON_LTHUMB;
    }
    if shoulders & 0x80 != 0 {
        buttons |= DUALSENSE_BUTTON_RTHUMB;
    }
    if system & 0x01 != 0 {
        buttons |= DUALSENSE_BUTTON_MODE;
    }

    match dpad {
        0 => buttons |= DUALSENSE_BUTTON_DPAD_UP,
        1 => buttons |= DUALSENSE_BUTTON_DPAD_UP | DUALSENSE_BUTTON_DPAD_RIGHT,
        2 => buttons |= DUALSENSE_BUTTON_DPAD_RIGHT,
        3 => buttons |= DUALSENSE_BUTTON_DPAD_DOWN | DUALSENSE_BUTTON_DPAD_RIGHT,
        4 => buttons |= DUALSENSE_BUTTON_DPAD_DOWN,
        5 => buttons |= DUALSENSE_BUTTON_DPAD_DOWN | DUALSENSE_BUTTON_DPAD_LEFT,
        6 => buttons |= DUALSENSE_BUTTON_DPAD_LEFT,
        7 => buttons |= DUALSENSE_BUTTON_DPAD_UP | DUALSENSE_BUTTON_DPAD_LEFT,
        _ => {}
    }

    Some(DualSenseBtState {
        lx: report[offset + 1],
        ly: report[offset + 2],
        rx: report[offset + 3],
        ry: report[offset + 4],
        l2: report[offset + 5],
        r2: report[offset + 6],
        buttons,
    })
}

extern "C" fn device_matching_cb(
    context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
    value: IOHIDDeviceRef,
) {
    let (tx, device_infos): &(
        Sender<(Event, Option<IOHIDDevice>)>,
        Arc<Mutex<Vec<DeviceInfo>>>,
    ) = unsafe { &*(context as *mut _) };
    let device = match IOHIDDevice::new(value) {
        Some(device) => device,
        None => {
            error!("Failed to get device");
            return;
        }
    };

    let io_service = match device.get_service() {
        Some(io_service) => io_service,
        None => {
            error!("Failed to get device service");
            return;
        }
    };

    let entry_id = match io_service.get_registry_entry_id() {
        Some(entry_id) => entry_id,
        None => {
            error!("Failed to get entry id of device");
            return;
        }
    };

    let mut device_infos = device_infos.lock().unwrap();
    let id = match device_infos
        .iter()
        .position(|info| info.entry_id == entry_id && info.is_connected)
    {
        Some(id) => {
            info!("Device is already registered: {:?}", entry_id);
            id
        }
        None => {
            let location_id = match device.get_location_id() {
                Some(location_id) => location_id,
                None => {
                    error!("Failed to get location id of device");
                    return;
                }
            };

            device_infos.push(DeviceInfo {
                entry_id: entry_id,
                location_id: location_id,
                is_connected: true,
            });

            device_infos.len() - 1
        }
    };
    register_sony_bt_report_callback(&device, id, tx);
    let _ = tx.send((Event::new(id, EventType::Connected), Some(device)));
}

extern "C" fn device_removal_cb(
    context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
    value: IOHIDDeviceRef,
) {
    let (tx, device_infos): &(
        Sender<(Event, Option<IOHIDDevice>)>,
        Arc<Mutex<Vec<DeviceInfo>>>,
    ) = unsafe { &*(context as *mut _) };

    let device = match IOHIDDevice::new(value) {
        Some(device) => device,
        None => {
            error!("Failed to get device");
            return;
        }
    };

    let location_id = match device.get_location_id() {
        Some(location_id) => location_id,
        None => {
            error!("Failed to get location id of device");
            return;
        }
    };

    let device_infos = device_infos.lock().unwrap();
    let id = match device_infos
        .iter()
        .position(|info| info.location_id == location_id && info.is_connected)
    {
        Some(id) => id,
        None => {
            warn!("Failed to find device: {:?}", location_id);
            return;
        }
    };

    let _ = tx.send((Event::new(id, EventType::Disconnected), None));
}

extern "C" fn input_value_cb(
    context: *mut c_void,
    _result: IOReturn,
    sender: *mut c_void,
    value: IOHIDValueRef,
) {
    let (tx, device_infos): &(
        Sender<(Event, Option<IOHIDDevice>)>,
        Arc<Mutex<Vec<DeviceInfo>>>,
    ) = unsafe { &*(context as *mut _) };

    let device = match IOHIDDevice::new(sender as _) {
        Some(device) => device,
        None => {
            error!("Failed to get device");
            return;
        }
    };

    let io_service = match device.get_service() {
        Some(io_service) => io_service,
        None => {
            error!("Failed to get device service");
            return;
        }
    };

    let entry_id = match io_service.get_registry_entry_id() {
        Some(entry_id) => entry_id,
        None => {
            error!("Failed to get entry id of device");
            return;
        }
    };

    let device_infos = device_infos.lock().unwrap();
    let id = match device_infos
        .iter()
        .position(|info| info.entry_id == entry_id && info.is_connected)
    {
        Some(id) => id,
        None => {
            warn!("Failed to find device: {:?}", entry_id);
            return;
        }
    };

    let value = match IOHIDValue::new(value) {
        Some(value) => value,
        None => {
            error!("Failed to get value");
            return;
        }
    };

    let element = match value.get_element() {
        Some(element) => element,
        None => {
            error!("Failed to get element of value");
            return;
        }
    };

    let type_ = element.get_type();
    let page = element.get_page();
    let usage = element.get_usage();

    if IOHIDElement::is_axis(type_, page, usage) {
        let event = Event::new(
            id,
            EventType::AxisValueChanged(
                value.get_value() as i32,
                crate::EvCode(EvCode {
                    page: page,
                    usage: usage,
                }),
            ),
        );
        let _ = tx.send((event, None));
    } else if IOHIDElement::is_button(type_, page, usage) {
        if value.get_value() == 0 {
            let event = Event::new(
                id,
                EventType::ButtonReleased(crate::EvCode(EvCode {
                    page: page,
                    usage: usage,
                })),
            );
            let _ = tx.send((event, None));
        } else {
            let event = Event::new(
                id,
                EventType::ButtonPressed(crate::EvCode(EvCode {
                    page: page,
                    usage: usage,
                })),
            );
            let _ = tx.send((event, None));
        }
    } else if IOHIDElement::is_hat(type_, page, usage) {
        // Hat switch values are reported with a range of usually 8 numbers (sometimes 4). The logic
        // below uses the reported min/max values of that range to map that onto a range of 0-7 for
        // the directions (and any other value indicates the center position). Lucky for us, they
        // always start with "up" as the lowest number and proceed clockwise. See similar handling
        // here https://github.com/spurious/SDL-mirror/blob/094b2f68dd7fc9af167f905e10625e103a131459/src/joystick/darwin/SDL_sysjoystick.c#L976-L1028
        //
        //          up
        //       7  0  1
        //        \ | /
        // left 6 - ? - 2 right       (After mapping)
        //        / | \
        //       5  4  3
        //         down
        let range = element.get_logical_max() - element.get_logical_min() + 1;
        let shifted_value = value.get_value() - element.get_logical_min();
        let dpad_value = match range {
            4 => shifted_value * 2, // 4-position hat switch - scale it up to 8
            8 => shifted_value,     // 8-position hat switch - no adjustment necessary
            _ => -1, // Neither 4 nor 8 positions, we don't know what to do - default to centered
        };
        // At this point, the value should be normalized to the 0-7 directional values (or center
        // for any other value). The dpad is a hat switch on macOS, but on other platforms dpads are
        // either buttons or a pair of axes that get converted to button events by the
        // `axis_dpad_to_button` filter.  We will emulate axes here and let that filter do the
        // button conversion, because it is safer and easier than making separate logic for button
        // conversion that may diverge in subtle ways from the axis conversion logic.  The most
        // practical outcome of this conversion is that there are extra "released" axis events for
        // the unused axis. For example, pressing just "up" will also give you a "released" event
        // for either the left or right button, even if it wasn't pressed before pressing "up".
        let x_axis_value = match dpad_value {
            5 | 6 | 7 => -1, // left
            1 | 2 | 3 => 1,  // right
            _ => 0,
        };
        // Since we're emulating an inverted macOS gamepad axis, down is positive and up is negative
        let y_axis_value = match dpad_value {
            3 | 4 | 5 => 1,  // down
            0 | 1 | 7 => -1, // up
            _ => 0,
        };

        let x_axis_event = Event::new(
            id,
            EventType::AxisValueChanged(
                x_axis_value,
                crate::EvCode(EvCode {
                    page,
                    usage: USAGE_AXIS_DPADX,
                }),
            ),
        );
        let y_axis_event = Event::new(
            id,
            EventType::AxisValueChanged(
                y_axis_value,
                crate::EvCode(EvCode {
                    page,
                    usage: USAGE_AXIS_DPADY,
                }),
            ),
        );

        let _ = tx.send((x_axis_event, None));
        let _ = tx.send((y_axis_event, None));
    }
}

#[cfg(test)]
mod sony_bt_tests {
    use super::*;

    fn base_report() -> [u8; DUALSENSE_BT_REPORT_LEN] {
        let mut report = [0u8; DUALSENSE_BT_REPORT_LEN];
        report[0] = DUALSENSE_BT_REPORT_ID as u8;
        report[2] = 0x80;
        report[3] = 0x81;
        report[4] = 0x82;
        report[5] = 0x83;
        report[6] = 0x12;
        report[7] = 0x34;
        report[9] = 0x08;
        report
    }

    #[test]
    fn dualsense_bt_report_decodes_axes() {
        let report = base_report();
        let state = parse_dualsense_bt_report(DUALSENSE_BT_REPORT_ID, &report).unwrap();
        assert_eq!(state.lx, 0x80);
        assert_eq!(state.ly, 0x81);
        assert_eq!(state.rx, 0x82);
        assert_eq!(state.ry, 0x83);
        assert_eq!(state.l2, 0x12);
        assert_eq!(state.r2, 0x34);
        assert_eq!(state.buttons, 0);
    }

    #[test]
    fn dualsense_bt_report_decodes_face_shoulders_system_and_dpad() {
        let mut report = base_report();
        report[9] = 0x20 | 0x02;
        report[10] = 0x01 | 0x20;
        report[11] = 0x01;
        let state = parse_dualsense_bt_report(DUALSENSE_BT_REPORT_ID, &report).unwrap();
        assert!(state.buttons & DUALSENSE_BUTTON_SOUTH != 0);
        assert!(state.buttons & DUALSENSE_BUTTON_DPAD_RIGHT != 0);
        assert!(state.buttons & DUALSENSE_BUTTON_LT != 0);
        assert!(state.buttons & DUALSENSE_BUTTON_START != 0);
        assert!(state.buttons & DUALSENSE_BUTTON_MODE != 0);
        assert_eq!(state.buttons & DUALSENSE_BUTTON_DPAD_UP, 0);
    }

    #[test]
    fn dualsense_bt_report_decodes_dpad_without_r3_or_guide_leaks() {
        let mut up_report = base_report();
        up_report[9] = 0x00;
        let up = parse_dualsense_bt_report(DUALSENSE_BT_REPORT_ID, &up_report).unwrap();
        assert!(up.buttons & DUALSENSE_BUTTON_DPAD_UP != 0);
        assert_eq!(up.buttons & DUALSENSE_BUTTON_RTHUMB, 0);
        assert_eq!(up.buttons & DUALSENSE_BUTTON_MODE, 0);
        assert_eq!(dualsense_dpad_axis_values(up.buttons), (0, -1));

        let mut down_report = base_report();
        down_report[9] = 0x04;
        let down = parse_dualsense_bt_report(DUALSENSE_BT_REPORT_ID, &down_report).unwrap();
        assert!(down.buttons & DUALSENSE_BUTTON_DPAD_DOWN != 0);
        assert_eq!(down.buttons & DUALSENSE_BUTTON_RTHUMB, 0);
        assert_eq!(down.buttons & DUALSENSE_BUTTON_MODE, 0);
        assert_eq!(dualsense_dpad_axis_values(down.buttons), (0, 1));
    }

    #[test]
    fn dualsense_bt_dpad_axis_values_cover_cardinals_diagonals_and_neutral() {
        assert_eq!(dualsense_dpad_axis_values(0), (0, 0));
        assert_eq!(
            dualsense_dpad_axis_values(DUALSENSE_BUTTON_DPAD_UP),
            (0, -1)
        );
        assert_eq!(
            dualsense_dpad_axis_values(DUALSENSE_BUTTON_DPAD_DOWN),
            (0, 1)
        );
        assert_eq!(
            dualsense_dpad_axis_values(DUALSENSE_BUTTON_DPAD_LEFT),
            (-1, 0)
        );
        assert_eq!(
            dualsense_dpad_axis_values(DUALSENSE_BUTTON_DPAD_RIGHT),
            (1, 0)
        );
        assert_eq!(
            dualsense_dpad_axis_values(DUALSENSE_BUTTON_DPAD_UP | DUALSENSE_BUTTON_DPAD_RIGHT),
            (1, -1)
        );
        assert_eq!(
            dualsense_dpad_axis_values(DUALSENSE_BUTTON_DPAD_DOWN | DUALSENSE_BUTTON_DPAD_LEFT),
            (-1, 1)
        );
    }
}
