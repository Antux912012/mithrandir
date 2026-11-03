pub mod battery_view;
pub mod display_view;
pub mod helpers;
pub mod memory_view;
pub mod network_view;
pub mod overview_view;
pub mod storage_view;

use gtk4::prelude::*;
use libadwaita as adw;
use std::rc::Rc;

use crate::hardware::{
    battery::BatteryInfo,
    display::DisplayInfo,
    memory::MemoryInfo,
    network::NetworkCardInfo,
    storage::DiskInfo,
    system::SystemInfo,
};

use self::{
    battery_view::build_battery_view,
    display_view::build_display_view,
    memory_view::build_memory_view,
    network_view::build_network_view,
    overview_view::build_overview_view,
    storage_view::build_storage_view,
};

pub struct HardwarePage;

impl HardwarePage {
    pub fn build(toast_overlay: Option<adw::ToastOverlay>) -> gtk4::Widget {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        let stack = adw::ViewStack::builder()
            .hexpand(true)
            .vexpand(true)
            .build();

        let switcher = adw::ViewSwitcher::builder()
            .stack(&stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();

        let refresh_btn = gtk4::Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Rescan Hardware")
            .css_classes(["flat"])
            .valign(gtk4::Align::Center)
            .build();

        let top_box = gtk4::CenterBox::builder()
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(16)
            .margin_end(16)
            .center_widget(&switcher)
            .end_widget(&refresh_btn)
            .build();

        container.append(&top_box);

        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        container.append(&sep);

        container.append(&stack);

        let toast_overlay_rc = Rc::new(toast_overlay);

        // Initial population
        populate_hardware_stack(&stack, toast_overlay_rc.as_ref().as_ref());

        // Connect refresh button
        let stack_weak = stack.downgrade();
        let toast_overlay_for_refresh = toast_overlay_rc.clone();

        refresh_btn.connect_clicked(move |_| {
            if let Some(stack) = stack_weak.upgrade() {
                let current_page = stack.visible_child_name();
                populate_hardware_stack(&stack, toast_overlay_for_refresh.as_ref().as_ref());

                if let Some(name) = current_page {
                    stack.set_visible_child_name(&name);
                }

                if let Some(overlay) = toast_overlay_for_refresh.as_ref() {
                    let toast = adw::Toast::new("Hardware telemetry re-scanned");
                    toast.set_timeout(2);
                    overlay.add_toast(toast);
                }
            }
        });

        container.upcast()
    }
}

fn populate_hardware_stack(stack: &adw::ViewStack, overlay: Option<&adw::ToastOverlay>) {
    // Remove existing pages if refreshing
    while let Some(child) = stack.first_child() {
        stack.remove(&child);
    }

    // Hardware Data Collection
    let sys_info = SystemInfo::collect();
    let dev_model = sys_info.full_device_name();
    let batteries = BatteryInfo::collect_all(&dev_model);
    let mem_info = MemoryInfo::collect(&dev_model, &sys_info.cpu_model);
    let disks = DiskInfo::collect_all(&dev_model);
    let displays = DisplayInfo::collect_all(&dev_model);
    let network_adapters = NetworkCardInfo::collect_all(&dev_model);

    // Build Pages
    let overview_page = build_overview_view(&sys_info, &batteries, &mem_info, &disks, &displays, overlay);
    let battery_page = build_battery_view(&batteries, overlay);
    let storage_page = build_storage_view(&disks, overlay);
    let memory_page = build_memory_view(&mem_info, overlay);
    let display_page = build_display_view(&displays, overlay);
    let network_page = build_network_view(&network_adapters, overlay);

    // Add to stack with icons
    let p_overview = stack.add_titled(&overview_page, Some("overview"), "Overview");
    p_overview.set_icon_name(Some("computer-symbolic"));

    let p_bat = stack.add_titled(&battery_page, Some("battery"), "Battery");
    p_bat.set_icon_name(Some("battery-good-symbolic"));

    let p_storage = stack.add_titled(&storage_page, Some("storage"), "Storage");
    p_storage.set_icon_name(Some("drive-harddisk-symbolic"));

    let p_mem = stack.add_titled(&memory_page, Some("memory"), "Memory");
    p_mem.set_icon_name(Some("media-flash-symbolic"));

    let p_disp = stack.add_titled(&display_page, Some("display"), "Display");
    p_disp.set_icon_name(Some("video-display-symbolic"));

    let p_net = stack.add_titled(&network_page, Some("network"), "Network");
    p_net.set_icon_name(Some("network-wireless-symbolic"));
}
