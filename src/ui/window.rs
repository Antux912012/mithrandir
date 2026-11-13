use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, Stack,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    AboutDialog, ApplicationWindow, Breakpoint, BreakpointCondition, BreakpointConditionLengthType,
    HeaderBar, LengthUnit, NavigationPage, NavigationSplitView, ToastOverlay, ToolbarView,
    WindowTitle,
};
use std::rc::Rc;
use crate::backend::settings::AppSettings;
use crate::ui::ai::AIPage;
use crate::ui::autostart::AutostartPage;
use crate::ui::btrfs::BtrfsPage;
use crate::ui::cleaner::CleanerPage;
use crate::ui::dashboard::DashboardPage;
use crate::ui::grub::GrubPage;
use crate::ui::hardware::HardwarePage;
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
            .default_width(980)
            .default_height(680)
            .build();

        // Responsive min-size standard for mobile Linux devices (Phosh / Librem 5 / PinePhone)
        window.set_size_request(360, 480);

        let toast_overlay = ToastOverlay::new();
        let split_view = NavigationSplitView::builder()
            .min_sidebar_width(220.0)
            .max_sidebar_width(270.0)
            .sidebar_width_fraction(0.26)
            .build();

        // ---------------------------------------------------------
        // Content Pane (Right): HeaderBar + Stack
        // ---------------------------------------------------------
        let content_toolbar = ToolbarView::new();
        let content_header = HeaderBar::new();
        let content_title = WindowTitle::new("Overview", "System health, sensors & quick boost");
        content_header.set_title_widget(Some(&content_title));

        let sidebar_toggle_btn = Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Toggle Sidebar")
            .build();
        sidebar_toggle_btn.add_css_class("flat");
        content_header.pack_start(&sidebar_toggle_btn);

        let pref_btn = Button::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Preferences")
            .build();
        content_header.pack_end(&pref_btn);

        let info_btn = Button::builder()
            .icon_name("dialog-information-symbolic")
            .tooltip_text("About Mithrandir")
            .build();
        content_header.pack_end(&info_btn);

        content_toolbar.add_top_bar(&content_header);

        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .build();

        let page_dashboard = DashboardPage::build(Some(toast_overlay.clone()));
        let page_cleaner = CleanerPage::build();
        let page_ai = AIPage::build(Some(toast_overlay.clone()));
        let page_kernel_power = KernelPowerPage::build(Some(toast_overlay.clone()));
        let page_hardware = HardwarePage::build(Some(toast_overlay.clone()));
        let page_processes = ProcessesPage::build();
        let page_services = ServicesPage::build();
        let page_autostart = AutostartPage::build();
        let page_uninstaller = UninstallerPage::build();
        let page_btrfs = BtrfsPage::build();
        let page_grub = GrubPage::build(Some(toast_overlay.clone()));

        stack.add_titled(&page_dashboard, Some("dashboard"), "Overview");
        stack.add_titled(&page_cleaner, Some("cleaner"), "Smart Cleaner");
        stack.add_titled(&page_ai, Some("ai"), "AI Assistant");
        stack.add_titled(&page_kernel_power, Some("kernel_power"), "Kernel & Power");
        stack.add_titled(&page_hardware, Some("hardware"), "Hardware");
        stack.add_titled(&page_processes, Some("processes"), "Processes");
        stack.add_titled(&page_services, Some("services"), "Services");
        stack.add_titled(&page_autostart, Some("autostart"), "Startup Apps");
        stack.add_titled(&page_uninstaller, Some("uninstaller"), "Uninstaller");
        stack.add_titled(&page_btrfs, Some("btrfs"), "BTRFS Snapshots");
        stack.add_titled(&page_grub, Some("grub"), "GRUB Bootloader");

        content_toolbar.set_content(Some(&stack));
        let content_nav_page = NavigationPage::builder()
            .title("Overview")
            .child(&content_toolbar)
            .build();
        split_view.set_content(Some(&content_nav_page));

        // ---------------------------------------------------------
        // Sidebar Pane (Left): Grouped Navigation List
        // ---------------------------------------------------------
        let sidebar_toolbar = ToolbarView::new();
        let sidebar_header = HeaderBar::new();
        let sidebar_title = WindowTitle::new("Mithrandir", "");
        sidebar_header.set_title_widget(Some(&sidebar_title));
        sidebar_toolbar.add_top_bar(&sidebar_header);

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

        // Grouped information architecture: Overview & Care, Performance & Hardware, System & Recovery
        struct NavSpec {
            id: &'static str,
            label: &'static str,
            subtitle: &'static str,
            icon: &'static str,
            section: Option<&'static str>,
        }

        let pages = [
            NavSpec {
                id: "dashboard",
                label: "Overview",
                subtitle: "System health, sensors & quick boost",
                icon: "utilities-system-monitor-symbolic",
                section: Some("OVERVIEW & CARE"),
            },
            NavSpec {
                id: "cleaner",
                label: "Smart Cleaner",
                subtitle: "Reclaim caches, logs & temporary clutter",
                icon: "edit-clear-all-symbolic",
                section: None,
            },
            NavSpec {
                id: "ai",
                label: "AI Assistant",
                subtitle: "One-click triage & natural-language tuning",
                icon: "starred-symbolic",
                section: None,
            },
            NavSpec {
                id: "kernel_power",
                label: "Kernel & Power",
                subtitle: "BORE scheduler, CPU governors & ASPM",
                icon: "speedometer-symbolic",
                section: Some("PERFORMANCE & HARDWARE"),
            },
            NavSpec {
                id: "hardware",
                label: "Hardware",
                subtitle: "Battery health, NVMe, RAM, display & PCI",
                icon: "computer-symbolic",
                section: None,
            },
            NavSpec {
                id: "processes",
                label: "Processes",
                subtitle: "Inspect & manage running processes",
                icon: "view-list-bullet-symbolic",
                section: None,
            },
            NavSpec {
                id: "services",
                label: "Services",
                subtitle: "Manage systemd units",
                icon: "emblem-system-symbolic",
                section: Some("SYSTEM & RECOVERY"),
            },
            NavSpec {
                id: "autostart",
                label: "Startup Apps",
                subtitle: "Control login applications",
                icon: "system-run-symbolic",
                section: None,
            },
            NavSpec {
                id: "uninstaller",
                label: "Uninstaller",
                subtitle: "Remove native, Flatpak, Snap & Pip apps",
                icon: "system-software-install-symbolic",
                section: None,
            },
            NavSpec {
                id: "btrfs",
                label: "BTRFS Snapshots",
                subtitle: "Create & restore filesystem snapshots",
                icon: "drive-harddisk-symbolic",
                section: None,
            },
            NavSpec {
                id: "grub",
                label: "GRUB Bootloader",
                subtitle: "Kernel boot parameters & timeout",
                icon: "preferences-system-symbolic",
                section: None,
            },
        ];

        // Map section headers by row widget name
        let mut section_map: std::collections::HashMap<&'static str, &'static str> =
            std::collections::HashMap::new();
        for spec in &pages {
            if let Some(sec) = spec.section {
                section_map.insert(spec.id, sec);
            }
        }
        let section_map_rc = Rc::new(section_map);
        list_box.set_header_func(move |row, _before| {
            let name = row.widget_name();
            if let Some(sec_title) = section_map_rc.get(name.as_str()) {
                let hdr = Label::builder()
                    .label(*sec_title)
                    .xalign(0.0)
                    .build();
                hdr.add_css_class("sidebar-section-header");
                row.set_header(Some(&hdr));
            } else {
                row.set_header(None::<&gtk4::Widget>);
            }
        });

        let initial_settings = AppSettings::get();
        let mut nav_items: Vec<(ListBoxRow, &'static str, &'static str, &'static str)> = Vec::new();

        for spec in &pages {
            let row = ListBoxRow::new();
            row.set_widget_name(spec.id);
            let row_box = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(12)
                .margin_top(7)
                .margin_bottom(7)
                .margin_start(8)
                .margin_end(8)
                .build();

            let icon = Image::from_icon_name(spec.icon);
            icon.set_pixel_size(18);

            let lbl = Label::builder()
                .label(spec.label)
                .hexpand(true)
                .xalign(0.0)
                .build();

            row_box.append(&icon);
            row_box.append(&lbl);
            row.set_child(Some(&row_box));

            let is_disabled = initial_settings.disabled_tabs.iter().any(|t| t == spec.id);
            row.set_visible(!is_disabled);

            list_box.append(&row);
            nav_items.push((row, spec.id, spec.label, spec.subtitle));
        }

        let nav_items_rc = Rc::new(nav_items);
        sidebar_scroller.set_child(Some(&list_box));
        sidebar_toolbar.set_content(Some(&sidebar_scroller));

        let sidebar_nav_page = NavigationPage::builder()
            .title("Mithrandir")
            .child(&sidebar_toolbar)
            .build();
        split_view.set_sidebar(Some(&sidebar_nav_page));

        // Navigation row selection: lookup by widget_name
        {
            let stack_for_nav = stack.clone();
            let title_for_nav = content_title.clone();
            let nav_page_for_nav = content_nav_page.clone();
            let split_for_nav = split_view.clone();
            let nav_items_lookup = nav_items_rc.clone();

            list_box.connect_row_selected(move |_, row_opt| {
                if let Some(row) = row_opt {
                    let row_id = row.widget_name();
                    if let Some((_, id, label, subtitle)) =
                        nav_items_lookup.iter().find(|(_, id, _, _)| *id == row_id.as_str())
                    {
                        stack_for_nav.set_visible_child_name(id);
                        title_for_nav.set_title(label);
                        title_for_nav.set_subtitle(subtitle);
                        nav_page_for_nav.set_title(label);
                        if split_for_nav.is_collapsed() {
                            split_for_nav.set_show_content(true);
                        }
                    }
                }
            });
        }

        // Select first visible item at startup
        for (row, id, label, subtitle) in nav_items_rc.iter() {
            if !initial_settings.disabled_tabs.iter().any(|t| t == id) {
                list_box.select_row(Some(row));
                stack.set_visible_child_name(id);
                content_title.set_title(label);
                content_title.set_subtitle(subtitle);
                break;
            }
        }

        // Sidebar collapse/expand toggle on desktop & mobile
        {
            let split_for_toggle = split_view.clone();
            sidebar_toggle_btn.connect_clicked(move |_| {
                if split_for_toggle.is_collapsed() {
                    split_for_toggle.set_show_content(false);
                } else {
                    split_for_toggle.set_collapsed(true);
                    split_for_toggle.set_show_content(true);
                }
            });
        }

        // Adaptive breakpoint for narrow/mobile viewports (<= 720sp)
        let breakpoint = Breakpoint::new(BreakpointCondition::new_length(
            BreakpointConditionLengthType::MaxWidth,
            720.0,
            LengthUnit::Sp,
        ));
        {
            let split_bp_apply = split_view.clone();
            breakpoint.connect_apply(move |_| {
                split_bp_apply.set_collapsed(true);
            });
            let split_bp_unapply = split_view.clone();
            breakpoint.connect_unapply(move |_| {
                split_bp_unapply.set_collapsed(false);
            });
        }
        window.add_breakpoint(breakpoint);

        toast_overlay.set_child(Some(&split_view));
        window.set_content(Some(&toast_overlay));

        // Connect Preferences
        let win_weak = window.downgrade();
        let nav_items_for_pref = nav_items_rc.clone();
        let list_box_for_pref = list_box.clone();
        let stack_for_pref = stack.clone();
        pref_btn.connect_clicked(move |_| {
            if let Some(win) = win_weak.upgrade() {
                let nav_ref = nav_items_for_pref.clone();
                let lb_ref = list_box_for_pref.clone();
                let st_ref = stack_for_pref.clone();
                let dlg = PreferencesDialog::build(&win, move || {
                    let cfg = AppSettings::get();
                    let mut first_visible = None;
                    for (row, id, _, _) in nav_ref.iter() {
                        let is_disabled = cfg.disabled_tabs.iter().any(|t| t == id);
                        row.set_visible(!is_disabled);
                        if !is_disabled && first_visible.is_none() {
                            first_visible = Some((row.clone(), *id));
                        }
                    }
                    if let Some(selected) = lb_ref.selected_row() {
                        if !selected.is_visible() {
                            if let Some((first_row, first_id)) = first_visible {
                                lb_ref.select_row(Some(&first_row));
                                st_ref.set_visible_child_name(first_id);
                            }
                        }
                    }
                });
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
                    .developer_name("Antonio De Gaetano")
                    .version("1.2.0")
                    .comments("Native Linux System Optimization, Cache Management, CachyOS Scheduler Controller & Hardware Power Manager in Rust (GTK4 & Libadwaita)")
                    .website("https://github.com/Antux912012/mithrandir")
                    .license_type(gtk4::License::Gpl30Only)
                    .build();
                about.present(Some(&win));
            }
        });

        // Handle window close request: respect keep_in_background preference
        window.connect_close_request(move |w| {
            if AppSettings::get().keep_in_background {
                w.set_visible(false);
                gtk4::glib::Propagation::Stop
            } else {
                if let Some(app) = w.application() {
                    app.quit();
                }
                gtk4::glib::Propagation::Proceed
            }
        });

        if let Ok(capture_dir) = std::env::var("MITHRANDIR_CAPTURE_DIR") {
            use gtk4::gdk::prelude::TextureExt;
            use gtk4::gsk::prelude::GskRendererExt;
            use std::cell::Cell;
            use std::path::PathBuf;
            use std::time::Duration;

            let out_dir = PathBuf::from(capture_dir);
            let _ = std::fs::create_dir_all(&out_dir);

            let pages: Vec<(&'static str, &'static str)> = vec![
                ("dashboard", "01-dashboard.png"),
                ("kernel_power", "02-kernel-power.png"),
                ("cleaner", "03-cleaner.png"),
                ("uninstaller", "04-uninstaller.png"),
                ("grub", "05-grub.png"),
                ("ai", "06-ai-optimizer.png"),
            ];

            let step = Rc::new(Cell::new(0usize));
            let win_cap = window.clone();
            let lb_cap = list_box.clone();
            let nav_cap = nav_items_rc.clone();

            gtk4::glib::timeout_add_local(Duration::from_millis(900), move || {
                let idx = step.get();

                if idx < pages.len() {
                    let (tab_id, file_name) = pages[idx];
                    if let Some((row, _, _, _)) = nav_cap.iter().find(|(_, id, _, _)| *id == tab_id) {
                        lb_cap.select_row(Some(row));
                    }
                    let w_ref = win_cap.clone().upcast::<gtk4::Widget>();
                    let fname = file_name.to_string();
                    let out_dir_inner = out_dir.clone();
                    gtk4::glib::timeout_add_local_once(Duration::from_millis(600), move || {
                        let paintable = gtk4::WidgetPaintable::new(Some(&w_ref));
                        let snap = gtk4::Snapshot::new();
                        let width = w_ref.width().max(1) as f64;
                        let height = w_ref.height().max(1) as f64;
                        gtk4::gdk::prelude::PaintableExt::snapshot(&paintable, &snap, width, height);
                        if let Some(node) = snap.to_node() {
                            if let Some(renderer) = w_ref.native().and_then(|n| n.renderer()) {
                                let tex = renderer.render_texture(&node, None);
                                let _ = tex.save_to_png(out_dir_inner.join(fname));
                            }
                        }
                    });
                    step.set(idx + 1);
                    gtk4::glib::ControlFlow::Continue
                } else {
                    let dlg = PreferencesDialog::build(&win_cap, || {});
                    dlg.present();
                    let dlg_w = dlg.clone().upcast::<gtk4::Widget>();
                    let out_dir_pref = out_dir.clone();
                    let win_quit = win_cap.clone();
                    gtk4::glib::timeout_add_local_once(Duration::from_millis(400), move || {
                        let paintable = gtk4::WidgetPaintable::new(Some(&dlg_w));
                        let snap = gtk4::Snapshot::new();
                        let width = dlg_w.width().max(1) as f64;
                        let height = dlg_w.height().max(1) as f64;
                        gtk4::gdk::prelude::PaintableExt::snapshot(&paintable, &snap, width, height);
                        if let Some(node) = snap.to_node() {
                            if let Some(renderer) = dlg_w.native().and_then(|n| n.renderer()) {
                                let tex = renderer.render_texture(&node, None);
                                let _ = tex.save_to_png(out_dir_pref.join("07-preferences.png"));
                            }
                        }
                        dlg.close();
                        if let Some(app) = win_quit.application() {
                            app.quit();
                        }
                    });
                    gtk4::glib::ControlFlow::Break
                }
            });
        }

        window
    }
}
