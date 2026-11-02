use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Image, Label, ListBox, ListBoxRow, Orientation, ScrolledWindow, Stack,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    AboutDialog, ApplicationWindow, Breakpoint, BreakpointCondition, BreakpointConditionLengthType,
    HeaderBar, LengthUnit, OverlaySplitView, ToastOverlay, ToolbarView, WindowTitle,
};
use std::rc::Rc;
use crate::ui::autostart::AutostartPage;
use crate::ui::btrfs::BtrfsPage;
use crate::ui::cleaner::CleanerPage;
use crate::ui::dashboard::DashboardPage;
use crate::ui::grub::GrubPage;
use crate::ui::kernel_power::KernelPowerPage;
use crate::ui::preferences::PreferencesDialog;
use crate::ui::processes::ProcessesPage;
use crate::ui::services::ServicesPage;
use crate::ui::theme::ThemeManager;
use crate::ui::uninstaller::UninstallerPage;

pub struct MithrandirWindow;

impl MithrandirWindow {
    pub fn build(app: &adw::Application) -> ApplicationWindow {
        ThemeManager::apply_theme();

        let window = ApplicationWindow::builder()
            .application(app)
            .title("Mithrandir")
            .default_width(880)
            .default_height(640)
            .build();

        // Responsive min-size standard for mobile Linux devices (Phosh / Librem 5 / PinePhone)
        window.set_size_request(360, 480);

        let toast_overlay = ToastOverlay::new();
        let toolbar_view = ToolbarView::new();
        let header_bar = HeaderBar::new();

        let title_widget = WindowTitle::new("Mithrandir", "Dashboard");
        header_bar.set_title_widget(Some(&title_widget));

        // Hamburger icon to toggle sidebar
        let menu_btn = Button::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Toggle Navigation Menu")
            .build();
        header_bar.pack_start(&menu_btn);

        // Header Right: Preferences & Info Buttons
        let pref_btn = Button::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Preferences")
            .build();
        header_bar.pack_end(&pref_btn);

        let info_btn = Button::builder()
            .icon_name("dialog-information-symbolic")
            .tooltip_text("About Mithrandir")
            .build();
        header_bar.pack_end(&info_btn);

        toolbar_view.add_top_bar(&header_bar);

        // Stack for all functional views
        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .build();

        // Instantiate subpages
        let page_dashboard = DashboardPage::build(Some(toast_overlay.clone()));
        let page_kernel_power = KernelPowerPage::build(Some(toast_overlay.clone()));
        let page_cleaner = CleanerPage::build();
        let page_processes = ProcessesPage::build();
        let page_services = ServicesPage::build();
        let page_autostart = AutostartPage::build();
        let page_uninstaller = UninstallerPage::build();
        let page_btrfs = BtrfsPage::build();
        let page_grub = GrubPage::build();

        stack.add_titled(&page_dashboard, Some("dashboard"), "Dashboard");
        stack.add_titled(&page_kernel_power, Some("kernel_power"), "Kernel & Power");
        stack.add_titled(&page_cleaner, Some("cleaner"), "Cleaner");
        stack.add_titled(&page_processes, Some("processes"), "Processes");
        stack.add_titled(&page_services, Some("services"), "Services");
        stack.add_titled(&page_autostart, Some("autostart"), "Startup Apps");
        stack.add_titled(&page_uninstaller, Some("uninstaller"), "Uninstaller");
        stack.add_titled(&page_btrfs, Some("btrfs"), "BTRFS Snapshots");
        stack.add_titled(&page_grub, Some("grub"), "GRUB Bootloader");

        let pages = [
            ("dashboard", "Dashboard", "utilities-system-monitor-symbolic"),
            ("kernel_power", "Kernel & Power", "speedometer-symbolic"),
            ("cleaner", "Cleaner", "edit-clear-all-symbolic"),
            ("processes", "Processes", "view-list-bullet-symbolic"),
            ("services", "Services", "emblem-system-symbolic"),
            ("autostart", "Startup Apps", "system-run-symbolic"),
            ("uninstaller", "Uninstaller", "system-software-install-symbolic"),
            ("btrfs", "BTRFS Snapshots", "drive-harddisk-symbolic"),
            ("grub", "GRUB Bootloader", "preferences-system-symbolic"),
        ];

        // Sidebar Navigation
        let sidebar_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .width_request(220)
            .vexpand(true)
            .build();
        sidebar_box.add_css_class("sidebar");

        let sidebar_scroller = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let list_box = ListBox::builder().build();
        list_box.add_css_class("navigation-sidebar");

        let stack_rc = Rc::new(stack.clone());
        let title_rc = Rc::new(title_widget.clone());

        for (id, label_str, icon_name) in pages {
            let row = ListBoxRow::new();
            let row_box = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(12)
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(12)
                .margin_end(12)
                .build();

            let icon = Image::from_icon_name(icon_name);
            let lbl = Label::builder()
                .label(label_str)
                .hexpand(true)
                .xalign(0.0)
                .build();

            row_box.append(&icon);
            row_box.append(&lbl);
            row.set_child(Some(&row_box));
            list_box.append(&row);

            let stack_clone = stack_rc.clone();
            let title_clone = title_rc.clone();
            let id_str = id.to_string();
            let title_str = label_str.to_string();

            row.connect_activate(move |_| {
                stack_clone.set_visible_child_name(&id_str);
                title_clone.set_subtitle(&title_str);
            });
        }

        sidebar_scroller.set_child(Some(&list_box));
        sidebar_box.append(&sidebar_scroller);

        // Adaptive Overlay Split View (GNOME HIG)
        let split_view = OverlaySplitView::builder()
            .sidebar(&sidebar_box)
            .content(&stack)
            .min_sidebar_width(220.0)
            .max_sidebar_width(260.0)
            .collapsed(false)
            .show_sidebar(true)
            .build();

        let split_view_rc = Rc::new(split_view.clone());
        let stack_for_nav = stack_rc.clone();
        let title_for_nav = title_rc.clone();
        let split_view_for_nav = split_view_rc.clone();

        list_box.connect_row_selected(move |_, row_opt| {
            if let Some(row) = row_opt {
                let idx = row.index();
                if idx >= 0 && (idx as usize) < pages.len() {
                    let (id, title_str, _) = pages[idx as usize];
                    stack_for_nav.set_visible_child_name(id);
                    title_for_nav.set_subtitle(title_str);

                    // If mobile / collapsed, hide sidebar after navigation
                    if split_view_for_nav.is_collapsed() {
                        split_view_for_nav.set_show_sidebar(false);
                    }
                }
            }
        });

        // Breakpoint for dynamic adaptive responsive layout (mobile / narrow screen)
        let breakpoint = Breakpoint::new(BreakpointCondition::new_length(
            BreakpointConditionLengthType::MaxWidth,
            720.0,
            LengthUnit::Sp,
        ));
        breakpoint.add_setter(&split_view, "collapsed", Some(&true.into()));
        window.add_breakpoint(breakpoint);

        // Connect hamburger button to toggle sidebar
        let split_view_menu = split_view_rc.clone();
        menu_btn.connect_clicked(move |_| {
            split_view_menu.set_show_sidebar(!split_view_menu.shows_sidebar());
        });

        toolbar_view.set_content(Some(&split_view));
        toast_overlay.set_child(Some(&toolbar_view));
        window.set_content(Some(&toast_overlay));

        // Connect Preferences
        let win_weak = window.downgrade();
        pref_btn.connect_clicked(move |_| {
            if let Some(win) = win_weak.upgrade() {
                let dlg = PreferencesDialog::build(&win);
                dlg.present();
            }
        });

        // Connect About dialog
        let win_weak_about = window.downgrade();
        info_btn.connect_clicked(move |_| {
            if let Some(win) = win_weak_about.upgrade() {
                let about = AboutDialog::builder()
                    .application_name("Mithrandir")
                    .application_icon("io.github.antonio.Mithrandir")
                    .developer_name("Antonio")
                    .version("1.0.0")
                    .comments("Native Linux System Optimization, Cache Management, CachyOS Scheduler Controller & Hybrid GPU Power Utility in Rust (GTK4 & Libadwaita)")
                    .website("https://github.com/Antux912012/mithrandir")
                    .license_type(gtk4::License::Gpl30Only)
                    .build();
                about.present(Some(&win));
            }
        });

        // Handle window close request: hide to background
        window.set_hide_on_close(true);
        window.connect_close_request(move |w| {
            w.set_visible(false);
            gtk4::glib::Propagation::Stop
        });

        window
    }
}
