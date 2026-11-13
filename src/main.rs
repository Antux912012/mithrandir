use mithrandir::{CleanerEngine, SmartBoostEngine, SystemMonitor};
#[cfg(feature = "gui")]
use mithrandir::BackgroundDaemon;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--apply-kernel-state") {
        let mut bore: Option<bool> = None;
        let mut governor: Option<String> = None;
        let mut epp: Option<String> = None;
        let mut boost: Option<bool> = None;
        let mut aspm: Option<String> = None;
        let persist = args.iter().any(|a| a == "--persist");

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--bore" if i + 1 < args.len() => {
                    bore = Some(args[i + 1] == "1");
                    i += 1;
                }
                "--governor" if i + 1 < args.len() => {
                    governor = Some(args[i + 1].clone());
                    i += 1;
                }
                "--epp" if i + 1 < args.len() => {
                    epp = Some(args[i + 1].clone());
                    i += 1;
                }
                "--boost" if i + 1 < args.len() => {
                    boost = Some(args[i + 1] == "1");
                    i += 1;
                }
                "--aspm" if i + 1 < args.len() => {
                    aspm = Some(args[i + 1].clone());
                    i += 1;
                }
                _ => {}
            }
            i += 1;
        }

        match mithrandir::apply_kernel_state_privileged(
            bore,
            governor.as_deref(),
            epp.as_deref(),
            boost,
            aspm.as_deref(),
            persist,
        ) {
            Ok(_) => {
                println!("✅ Mithrandir kernel & power state applied successfully.");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("❌ Error applying kernel & power state: {}", e);
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--kernel" || a == "--sched") {
        let kinfo = mithrandir::detect_kernel();
        println!("🧠 Kernel Release:      {}", kinfo.kernel_release);
        println!("🐧 CachyOS Kernel:      {}", if kinfo.is_cachyos { "Yes" } else { "No" });
        println!("⚡ BORE Supported:      {}", if kinfo.bore_supported { "Yes" } else { "No" });
        if kinfo.bore_supported {
            println!("   BORE Active:         {}", if kinfo.bore_enabled { "Enabled (1)" } else { "Disabled / EEVDF (0)" });
        }
        println!("🔋 Active Power Preset:  {}", mithrandir::get_active_preset().name());
        return;
    }

    if args.iter().any(|a| a == "--smart-boost") {
        println!("⚡ Executing Mithrandir Smart Boost (Rust Engine)...");
        let status = SmartBoostEngine::get_boost_status();
        println!("  Initial RAM: {:.1}% | Clutter: {}", status.ram_percent, status.temp_size_str);

        let res = SmartBoostEngine::execute_boost(true, true);
        println!("✨ Boost complete!");
        println!("  RAM Reclaimed: {:.1} MB", res.ram_reclaimed_mb);
        println!("  Disk Freed:    {:.1} MB", res.disk_freed_mb);
        println!("  Current RAM:   {:.1}%", res.current_ram_percent);
        return;
    }

    if args.iter().any(|a| a == "--monitor") {
        println!("📊 Mithrandir System Telemetry (Rust Engine)");
        let mem = SystemMonitor::get_memory_info();
        println!("  RAM:  {:.1}% ({:.1} MB / {:.1} MB)", mem.percent, mem.used_mb, mem.total_mb);

        let cpu = SystemMonitor::get_cpu_info();
        println!("  CPU:  {:.1}% ({} cores - {})", cpu.usage_percent, cpu.core_count, cpu.brand);

        let disks = SystemMonitor::get_disks_info();
        for d in disks {
            println!("  Disk ({}): {:.1}% ({:.1} GB / {:.1} GB)", d.mount_point, d.percent, d.used_gb, d.total_gb);
        }
        return;
    }

    if args.iter().any(|a| a == "--clean") {
        println!("🧹 Scanning cleanable categories...");
        let categories = CleanerEngine::get_categories();
        for cat in &categories {
            println!("  [{}]: {} ({:.1} MB, {} files)", cat.id, cat.name, cat.size_mb, cat.file_count);
        }
        return;
    }

    if args.iter().any(|a| a == "--ai-optimize" || a == "--triage") {
        println!("🧠 Running Mithrandir AI System Optimizer (Nemotron)...");
        let report = mithrandir::AIOptimizerEngine::run_heuristic_optimization();
        println!("  CPU:      {}", report.cpu_summary);
        println!("  Memory:   {}", report.memory_summary);
        println!("  Kernel:   {}", report.kernel_summary);
        println!("  Storage:  {}", report.storage_summary);
        println!("\n📋 Recommendations:");
        for rec in &report.recommendations {
            println!("  [{}] {}: {}", rec.category, rec.title, rec.description);
            if let Some(ref cmd) = rec.proposed_command {
                println!("    Fix: {}", cmd);
            }
        }
        return;
    }

    #[cfg(feature = "gui")]
    {
        use libadwaita as adw;
        use adw::prelude::*;

        let start_minimized = args
            .iter()
            .any(|a| a == "--background" || a == "--minimized" || a == "--keep-alive");

        // Start background optimization monitor thread (runs every 60s)
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                let cfg = mithrandir::AppSettings::get();
                if cfg.auto_boost_enabled {
                    BackgroundDaemon::run_auto_boost_check(
                        cfg.ram_threshold,
                        cfg.temp_threshold_mb,
                        cfg.cooldown_minutes,
                        cfg.desktop_notifications,
                    );
                }
            }
        });

        // Apply hardware acceleration settings (Vulkan / OpenGL / Cairo CPU fallback)
        let init_settings = mithrandir::AppSettings::get();
        mithrandir::GraphicsManager::apply_acceleration_settings(
            init_settings.hardware_acceleration,
            &init_settings.graphics_renderer,
        );

        let app = adw::Application::builder()
            .application_id("io.github.antux912012.Mithrandir")
            .build();

        // Query initial power saver state and create thread-safe shared flag
        let initial_power_saver = mithrandir::PowerManager::is_power_saver_active();
        let power_saver_arc = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(initial_power_saver));

        // Setup communication channel between tray thread and GTK4 main thread
        let (tx, rx) = async_channel::unbounded();

        // Spawn pure Rust StatusNotifierItem (SNI) + DBusMenu service (no GTK3 conflict)
        match mithrandir::tray::MithrandirTray::spawn_service(tx, power_saver_arc.clone()) {
            Ok(handle) => {
                mithrandir::tray::register_tray_handle(handle);
            }
            Err(e) => {
                eprintln!("Warning: Failed to register system tray icon: {}", e);
            }
        }

        // Hold guard keeps GApplication event loop active when all windows are hidden in background
        let hold_guard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let hg_init = hold_guard.clone();

        app.connect_startup(move |app| {
            *hg_init.borrow_mut() = Some(app.hold());

            // Register bundled Papirus symbolic icons so UI icons render on any desktop theme.
            const GRESOURCE_BYTES: &[u8] =
                include_bytes!(concat!(env!("OUT_DIR"), "/mithrandir.gresource"));
            let bytes = gtk4::glib::Bytes::from_static(GRESOURCE_BYTES);
            if let Ok(resource) = gtk4::gio::Resource::from_data(&bytes) {
                gtk4::gio::resources_register(&resource);
            }
            if let Some(display) = gtk4::gdk::Display::default() {
                let icon_theme = gtk4::IconTheme::for_display(&display);
                icon_theme.add_resource_path("/io/github/antux912012/Mithrandir/icons/papirus");
            }
        });

        let ps_arc_action = power_saver_arc.clone();

        app.connect_activate(move |app| {
            // Single-instance activation: restore existing window if already running
            if let Some(existing_win) = app.windows().first() {
                existing_win.set_visible(true);
                existing_win.present();
                return;
            }

            let win = mithrandir::ui::MithrandirWindow::build(app);

            let rx_channel = rx.clone();
            let app_weak = app.downgrade();
            let win_weak = win.downgrade();
            let ps_arc = ps_arc_action.clone();

            gtk4::glib::spawn_future_local(async move {
                while let Ok(action) = rx_channel.recv().await {
                    match action {
                        mithrandir::TrayAction::RunSmartboost => {
                            if let Some(res) = mithrandir::tray::MithrandirTray::execute_tray_smartboost() {
                                let body = format!(
                                    "Reclaimed {:.1} MB RAM and freed {:.1} MB disk cache.",
                                    res.ram_reclaimed_mb, res.disk_freed_mb
                                );
                                mithrandir::NotificationManager::send_notification("⚡ Smart Boost", &body);
                            }
                        }
                        mithrandir::TrayAction::ApplyPreset(preset) => {
                            match mithrandir::apply_preset(preset) {
                                Ok(_) => {
                                    let msg = format!("Applied Power Preset: {}", preset.name());
                                    mithrandir::NotificationManager::send_notification("⚡ Power Management", &msg);
                                    let is_battery = preset == mithrandir::PowerPreset::Battery;
                                    ps_arc.store(is_battery, std::sync::atomic::Ordering::SeqCst);
                                    mithrandir::tray::sync_tray_power_saver(is_battery);
                                    mithrandir::ui::DashboardPage::update_ui_power_saver(is_battery);
                                }
                                Err(e) => {
                                    let err_msg = format!("Failed to apply preset {}: {}", preset.name(), e);
                                    mithrandir::NotificationManager::send_notification("⚠️ Power Error", &err_msg);
                                }
                            }
                        }
                        mithrandir::TrayAction::TogglePowerSaver(enable) => {
                            let res = mithrandir::PowerManager::set_power_saver(enable);
                            match res {
                                Ok(_) => {
                                    let body = if enable {
                                        "Hardware Power Saver Enabled (Turbo Disabled, EPP Power, PCIe ASPM Powersupersave)"
                                    } else {
                                        "Hardware Power Saver Disabled (Performance Restored)"
                                    };
                                    mithrandir::NotificationManager::send_notification("⚡ Power Management", body);
                                    mithrandir::ui::DashboardPage::update_ui_power_saver(enable);
                                }
                                Err(e) => {
                                    eprintln!("Failed to toggle power saver: {}", e);
                                    ps_arc.store(!enable, std::sync::atomic::Ordering::SeqCst);
                                    mithrandir::tray::sync_tray_power_saver(!enable);
                                    mithrandir::ui::DashboardPage::update_ui_power_saver(!enable);
                                    let err_msg = format!("Failed to change power profile: {}", e);
                                    mithrandir::NotificationManager::send_notification("⚠️ Power Saver Error", &err_msg);
                                }
                            }
                        }
                        mithrandir::TrayAction::OpenMithrandir => {
                            if let Some(w) = win_weak.upgrade() {
                                w.set_visible(true);
                                w.present();
                            }
                        }
                        mithrandir::TrayAction::QuitMithrandir => {
                            if let Some(a) = app_weak.upgrade() {
                                a.quit();
                            }
                        }
                    }
                }
            });

            if !start_minimized {
                win.present();
            }
        });

        let filtered_args: Vec<String> = args
            .into_iter()
            .filter(|a| a != "--background" && a != "--minimized" && a != "--keep-alive")
            .collect();

        app.run_with_args(&filtered_args);
    }

    #[cfg(not(feature = "gui"))]
    {
        println!("Mithrandir v1.1.2 (GNU/Linux System Cleaner, Optimizer & Kernel Manager in Rust)");
        println!("Usage:");
        println!("  mithrandir                 Launch Libadwaita GUI");
        println!("  mithrandir --smart-boost   One-click RAM and cache boost");
        println!("  mithrandir --monitor       Real-time system telemetry");
        println!("  mithrandir --kernel        CachyOS scheduler & power profile telemetry");
        println!("  mithrandir --clean         Scan system clutter and cache");
        println!("  mithrandir --background    Launch hidden in background with panel tray icon");
        println!("  mithrandir --keep-alive    Run background daemon on login");
    }
}
