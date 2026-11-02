use async_channel::Sender;
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::*,
    Category, Icon, MenuItem, Tray,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    RunSmartboost,
    TogglePowerSaver(bool),
    ApplyPreset(crate::power::PowerPreset),
    OpenMithrandir,
    QuitMithrandir,
}

pub struct MithrandirTray {
    pub tx: Sender<TrayAction>,
    pub power_saver_enabled: Arc<AtomicBool>,
}

static TRAY_HANDLE: std::sync::Mutex<Option<Handle<MithrandirTray>>> = std::sync::Mutex::new(None);

pub fn register_tray_handle(handle: Handle<MithrandirTray>) {
    if let Ok(mut lock) = TRAY_HANDLE.lock() {
        *lock = Some(handle);
    }
}

pub fn sync_tray_power_saver(active: bool) {
    if let Ok(lock) = TRAY_HANDLE.lock() {
        if let Some(ref handle) = *lock {
            handle.update(move |tray| {
                tray.power_saver_enabled.store(active, Ordering::SeqCst);
            });
        }
    }
}

impl MithrandirTray {
    pub fn new(tx: Sender<TrayAction>, power_saver_enabled: Arc<AtomicBool>) -> Self {
        Self {
            tx,
            power_saver_enabled,
        }
    }

    /// Spawns the system tray service on a background thread.
    /// Handles Flatpak sandbox D-Bus naming constraints gracefully.
    pub fn spawn_service(
        tx: Sender<TrayAction>,
        power_saver_enabled: Arc<AtomicBool>,
    ) -> Result<Handle<Self>, ksni::Error> {
        let tray = Self::new(tx, power_saver_enabled);
        let is_flatpak = std::path::Path::new("/.flatpak-info").exists();
        if is_flatpak {
            tray.disable_dbus_name(true).spawn()
        } else {
            tray.spawn()
        }
    }

    /// Generates a fallback 16x16 monochrome ARGB32 icon pixmap if system icon theme lookup fails.
    pub fn generate_fallback_pixmap() -> Vec<Icon> {
        let width = 16;
        let height = 16;
        let mut data = Vec::with_capacity(width * height * 4);

        // A brand blue circular emblem matching Mithrandir `#2b7bdd`
        for y in 0..height {
            for x in 0..width {
                let dx = (x as f32 - 7.5).abs();
                let dy = (y as f32 - 7.5).abs();
                let dist = (dx * dx + dy * dy).sqrt();

                if dist <= 6.5 {
                    // ARGB32 format in network byte order: Alpha, Red, Green, Blue (#2b7bdd)
                    data.push(255); // Alpha
                    data.push(43);  // Red (#2B)
                    data.push(123); // Green (#7B)
                    data.push(221); // Blue (#DD)
                } else {
                    data.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
        }

        vec![Icon {
            width: width as i32,
            height: height as i32,
            data,
        }]
    }

    /// Concurrency-safe Smartboost runner for tray actions.
    pub fn execute_tray_smartboost() -> Option<crate::backend::smart_boost::BoostResult> {
        static RUN_GUARD: AtomicBool = AtomicBool::new(false);

        if RUN_GUARD
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }

        let result = crate::backend::smart_boost::SmartBoostEngine::execute_boost(true, true);
        RUN_GUARD.store(false, Ordering::SeqCst);
        Some(result)
    }
}

impl Tray for MithrandirTray {
    fn id(&self) -> String {
        "mithrandir".to_string()
    }

    fn category(&self) -> Category {
        Category::SystemServices
    }

    fn title(&self) -> String {
        "Mithrandir".to_string()
    }

    fn icon_name(&self) -> String {
        "mithrandir-status-symbolic".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        Self::generate_fallback_pixmap()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.try_send(TrayAction::OpenMithrandir);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let tx_boost = self.tx.clone();
        let tx_power = self.tx.clone();
        let tx_bat = self.tx.clone();
        let tx_bal = self.tx.clone();
        let tx_low = self.tx.clone();
        let tx_game = self.tx.clone();
        let tx_open = self.tx.clone();
        let tx_quit = self.tx.clone();

        let is_power_active = self.power_saver_enabled.load(Ordering::SeqCst);
        let kinfo = crate::kernel::detect_kernel();
        let current_preset = crate::power::get_active_preset();

        let sched_info = if kinfo.scx_supported && kinfo.scx_active_scheduler != "none" {
            format!("Active: {}", kinfo.scx_active_scheduler)
        } else if kinfo.bore_supported && kinfo.bore_enabled {
            "Active: BORE".to_string()
        } else if kinfo.bore_supported && !kinfo.bore_enabled {
            "Active: EEVDF".to_string()
        } else {
            "Active: Default".to_string()
        };

        let kernel_submenu = SubMenu {
            label: "Kernel & Power".into(),
            icon_name: "power-profile-balanced-symbolic".into(),
            submenu: vec![
                StandardItem {
                    label: sched_info,
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                CheckmarkItem {
                    label: "Battery / Silent".into(),
                    checked: current_preset == crate::power::PowerPreset::Battery,
                    activate: Box::new(move |_| {
                        let _ = tx_bat.try_send(TrayAction::ApplyPreset(crate::power::PowerPreset::Battery));
                    }),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "Balanced".into(),
                    checked: current_preset == crate::power::PowerPreset::Balanced,
                    activate: Box::new(move |_| {
                        let _ = tx_bal.try_send(TrayAction::ApplyPreset(crate::power::PowerPreset::Balanced));
                    }),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "Low-Latency / Workstation".into(),
                    checked: current_preset == crate::power::PowerPreset::LowLatency,
                    activate: Box::new(move |_| {
                        let _ = tx_low.try_send(TrayAction::ApplyPreset(crate::power::PowerPreset::LowLatency));
                    }),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "Max Performance / Gaming".into(),
                    checked: current_preset == crate::power::PowerPreset::Gaming,
                    activate: Box::new(move |_| {
                        let _ = tx_game.try_send(TrayAction::ApplyPreset(crate::power::PowerPreset::Gaming));
                    }),
                    ..Default::default()
                }
                .into(),
            ],
            ..Default::default()
        };

        vec![
            StandardItem {
                label: "Run Smartboost".into(),
                icon_name: "media-playback-start-symbolic".into(),
                activate: Box::new(move |_| {
                    let _ = tx_boost.try_send(TrayAction::RunSmartboost);
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Battery Power Saver".into(),
                icon_name: "battery-symbolic".into(),
                checked: is_power_active,
                activate: Box::new(move |tray: &mut MithrandirTray| {
                    let current = tray.power_saver_enabled.load(Ordering::SeqCst);
                    let target = !current;
                    tray.power_saver_enabled.store(target, Ordering::SeqCst);
                    let _ = tx_power.try_send(TrayAction::TogglePowerSaver(target));
                }),
                ..Default::default()
            }
            .into(),
            kernel_submenu.into(),
            MenuItem::Separator,
            StandardItem {
                label: "Open Mithrandir".into(),
                icon_name: "utilities-system-monitor-symbolic".into(),
                activate: Box::new(move |_| {
                    let _ = tx_open.try_send(TrayAction::OpenMithrandir);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Mithrandir".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(move |_| {
                    let _ = tx_quit.try_send(TrayAction::QuitMithrandir);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
