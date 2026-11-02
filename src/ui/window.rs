use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, Separator, Stack,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    AboutDialog, ApplicationWindow, Breakpoint, BreakpointCondition, BreakpointConditionLengthType,
    HeaderBar, LengthUnit, ToastOverlay, ToolbarView, WindowTitle,
};
use std::cell::Cell;
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
    pub const EXPANDED_WIDTH: i32 = 220;
    pub const COLLAPSED_WIDTH: i32 = 56;

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

        // Top Left Toggle Box aligned strictly with the sidebar icon column (56px width)
        let toggle_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .width_request(Self::COLLAPSED_WIDTH)
            .halign(Align::Center)
            .valign(Align::Center)
            .hexpand(false)
            .build();

        let menu_btn = Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Collapse Sidebar")
            .halign(Align::Center)
            .valign(Align::Center)
            .build();
        menu_btn.add_css_class("flat");
        menu_btn.add_css_class("sidebar-toggle-btn");
        toggle_box.append(&menu_btn);
        header_bar.pack_start(&toggle_box);

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

        // Stack for functional pages
        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .build();

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

        // Sidebar Container: STRICTLY hexpand = false so it never steals width from content
        let sidebar_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .hexpand(false)
            .vexpand(true)
            .build();
        sidebar_box.set_size_request(Self::EXPANDED_WIDTH, -1);
        sidebar_box.add_css_class("sidebar");

        let sidebar_scroller = ScrolledWindow::builder()
            .hexpand(false)
            .vexpand(true)
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .build();

        let list_box = ListBox::builder()
            .selection_mode(gtk4::SelectionMode::Single)
            .build();
        list_box.add_css_class("navigation-sidebar");

        let mut nav_items = Vec::new();

        for (id, label_str, icon_name) in pages {
            let row = ListBoxRow::new();
            let row_box = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(10)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(8)
                .margin_end(8)
                .build();

            let icon = Image::from_icon_name(icon_name);
            icon.set_pixel_size(20);

            let lbl = Label::builder()
                .label(label_str)
                .hexpand(true)
                .xalign(0.0)
                .build();

            row_box.append(&icon);
            row_box.append(&lbl);
            row.set_child(Some(&row_box));
            list_box.append(&row);

            nav_items.push((icon, lbl, row.clone(), label_str.to_string(), id));
        }

        sidebar_scroller.set_child(Some(&list_box));
        sidebar_box.append(&sidebar_scroller);

        // Sidebar expand / collapse controller (Zen / Firefox style)
        let is_expanded = Rc::new(Cell::new(true));
        let nav_items_rc = Rc::new(nav_items);

        let apply_sidebar_state = {
            let sidebar_box = sidebar_box.clone();
            let nav_items = nav_items_rc.clone();
            let menu_btn = menu_btn.clone();
            let is_expanded = is_expanded.clone();
            Rc::new(move |expanded: bool| {
                is_expanded.set(expanded);
                if expanded {
                    sidebar_box.set_size_request(Self::EXPANDED_WIDTH, -1);
                    menu_btn.set_tooltip_text(Some("Collapse Sidebar"));
                    for (icon, lbl, row, _title, _) in nav_items.iter() {
                        lbl.set_visible(true);
                        icon.set_hexpand(false);
                        icon.set_halign(Align::Start);
                        row.set_tooltip_text(None);
                    }
                } else {
                    sidebar_box.set_size_request(Self::COLLAPSED_WIDTH, -1);
                    menu_btn.set_tooltip_text(Some("Expand Sidebar"));
                    for (icon, lbl, row, title, _) in nav_items.iter() {
                        lbl.set_visible(false);
                        icon.set_hexpand(true);
                        icon.set_halign(Align::Center);
                        row.set_tooltip_text(Some(title));
                    }
                }
            })
        };

        let apply_toggle = apply_sidebar_state.clone();
        let is_expanded_btn = is_expanded.clone();
        menu_btn.connect_clicked(move |_| {
            apply_toggle(!is_expanded_btn.get());
        });

        // Breakpoint: adapt automatically on smaller/mobile viewports (<= 720sp)
        let breakpoint = Breakpoint::new(BreakpointCondition::new_length(
            BreakpointConditionLengthType::MaxWidth,
            720.0,
            LengthUnit::Sp,
        ));
        let apply_bp_apply = apply_sidebar_state.clone();
        breakpoint.connect_apply(move |_| {
            apply_bp_apply(false);
        });
        let apply_bp_unapply = apply_sidebar_state.clone();
        breakpoint.connect_unapply(move |_| {
            apply_bp_unapply(true);
        });
        window.add_breakpoint(breakpoint);

        // Navigation row selection: single source of truth
        let stack_for_nav = stack.clone();
        let title_for_nav = title_widget.clone();

        list_box.connect_row_selected(move |_, row_opt| {
            if let Some(row) = row_opt {
                let idx = row.index();
                if idx >= 0 && (idx as usize) < pages.len() {
                    let (id, title_str, _) = pages[idx as usize];
                    stack_for_nav.set_visible_child_name(id);
                    title_for_nav.set_subtitle(title_str);
                }
            }
        });

        // Highlight first item ("Dashboard") at startup
        if let Some(first_row) = list_box.row_at_index(0) {
            list_box.select_row(Some(&first_row));
        }

        // Main Horizontal View: [Sidebar (fixed width)] | [Separator] | [Stack (hexpand true)]
        let main_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .hexpand(true)
            .vexpand(true)
            .build();

        main_box.append(&sidebar_box);

        let sep = Separator::new(Orientation::Vertical);
        main_box.append(&sep);

        main_box.append(&stack);

        toolbar_view.set_content(Some(&main_box));
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
                    .application_icon("io.github.antux912012.Mithrandir")
                    .developer_name("Antux912012")
                    .version("1.0.1")
                    .comments("Native Linux System Optimization, Cache Management, CachyOS Scheduler Controller & Hardware Power Manager in Rust (GTK4 & Libadwaita)")
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
