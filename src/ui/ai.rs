use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Entry, Frame, Image, Orientation, ProgressBar,
    ScrolledWindow, TextView, WrapMode,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, Clamp, ComboRow, ExpanderRow, PasswordEntryRow, PreferencesGroup, Toast, ToastOverlay};
use std::rc::Rc;
use std::sync::mpsc;
use crate::ai::{
    AIClient, ChatMessage, LocalLLMManager, SafetyGuard, get_recommended_nemotron_models,
    AIOptimizerEngine,
};
use crate::backend::host_bridge::HostBridge;

pub struct AIPage;

impl AIPage {
    pub fn build(toast_overlay: Option<ToastOverlay>) -> ScrolledWindow {
        let scroller = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .kinetic_scrolling(true)
            .build();

        let root_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(20)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();

        let toast_rc = Rc::new(toast_overlay);

        // ---------------------------------------------------------
        // 1. One-Click AI Optimizer Triage Group
        // ---------------------------------------------------------
        let optimizer_group = PreferencesGroup::builder()
            .title("AI System Optimizer")
            .description("Real-time GNU/Linux tuning powered by NVIDIA Nemotron 3.5 Bolt E4B with DFlash speculative decoding")
            .build();

        let triage_action_row = ActionRow::builder()
            .title("System Health &amp; Performance Triage")
            .subtitle("Analyze kernel schedulers, memory pressure, storage caches, and systemd services")
            .build();
        triage_action_row.add_prefix(&Image::from_icon_name("starred-symbolic"));

        let run_triage_btn = Button::builder()
            .label("Run AI Optimization Triage")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();
        triage_action_row.add_suffix(&run_triage_btn);
        optimizer_group.add(&triage_action_row);

        // Triage progress bar
        let triage_progress = ProgressBar::builder()
            .visible(false)
            .pulse_step(0.1)
            .margin_start(12)
            .margin_end(12)
            .build();
        optimizer_group.add(&ActionRow::builder().child(&triage_progress).build());

        // Dynamic Recommendations Container
        let recs_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .margin_top(4)
            .margin_bottom(8)
            .build();
        optimizer_group.add(&ActionRow::builder().child(&recs_box).build());

        root_box.append(&optimizer_group);

        // ---------------------------------------------------------
        // 2. NVIDIA Nemotron Local Engine & DFlash GGUF Group
        // ---------------------------------------------------------
        let engine_group = PreferencesGroup::builder()
            .title("NVIDIA Nemotron 3.5 Bolt E4B Engine")
            .description("Compact 8B (4B effective) SLM with DFlash draft model for ~2x faster local inference")
            .build();

        let status_row = ActionRow::builder()
            .title("Engine Status")
            .subtitle("Probing local engine and models...")
            .build();
        status_row.add_prefix(&Image::from_icon_name("cpu-symbolic"));

        let start_engine_btn = Button::builder()
            .label("Start Engine")
            .valign(Align::Center)
            .build();

        let stop_engine_btn = Button::builder()
            .label("Stop")
            .valign(Align::Center)
            .build();

        status_row.add_suffix(&start_engine_btn);
        status_row.add_suffix(&stop_engine_btn);
        engine_group.add(&status_row);

        // Models Expander
        let models_expander = ExpanderRow::builder()
            .title("Nemotron GGUF Model Weights")
            .subtitle("Download DFlash draft model (737MB) and Base model (4.5GB)")
            .build();

        // Download Console inside expander
        let console_progress = ProgressBar::builder()
            .visible(false)
            .pulse_step(0.1)
            .build();

        let console_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(WrapMode::Word)
            .monospace(true)
            .height_request(100)
            .build();

        let console_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();
        console_box.append(&console_progress);
        console_box.append(&ScrolledWindow::builder().child(&console_view).vexpand(true).build());

        for model in get_recommended_nemotron_models() {
            let m_row = ActionRow::builder()
                .title(&model.name)
                .subtitle(format!("{} | {} | {}", model.size, model.ram_required, model.description))
                .build();

            let dl_btn = Button::builder()
                .label(if model.is_draft_model { "Download DFlash" } else { "Download Base" })
                .css_classes(vec!["pill"])
                .valign(Align::Center)
                .build();

            let m_id = model.id.clone();
            let cv_c = console_view.clone();
            let cp_c = console_progress.clone();
            let me_c = models_expander.clone();
            let sr_c = status_row.clone();

            dl_btn.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                me_c.set_expanded(true);
                cp_c.set_visible(true);

                let (tx, rx) = mpsc::channel();
                let m_id_t = m_id.clone();

                std::thread::spawn(move || {
                    let tx_log = tx.clone();
                    let res = LocalLLMManager::download_gguf_model(
                        &m_id_t,
                        move |line| {
                            let _ = tx_log.send((false, line));
                        },
                        None,
                    );
                    let done_msg = match res {
                        Ok(msg) => msg,
                        Err(e) => format!("Download error: {}", e),
                    };
                    let _ = tx.send((true, done_msg));
                });

                let btn_clone = btn.clone();
                let cv_clone = cv_c.clone();
                let cp_clone = cp_c.clone();
                let sr_clone = sr_c.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
                    while let Ok((done, line)) = rx.try_recv() {
                        let buf = cv_clone.buffer();
                        let mut end = buf.end_iter();
                        buf.insert(&mut end, &format!("{}\n", line));

                        if done {
                            btn_clone.set_sensitive(true);
                            cp_clone.set_visible(false);
                            let st = LocalLLMManager::check_llamacpp_status();
                            sr_clone.set_subtitle(&format!(
                                "Running: {} | Models: {} (DFlash: {}) | Disk: {:.1} MB",
                                st.running, st.models.len(), st.has_nemotron_dflash, st.disk_usage_mb
                            ));
                            return glib::ControlFlow::Break;
                        }
                    }
                    glib::ControlFlow::Continue
                });
            });

            m_row.add_suffix(&dl_btn);
            models_expander.add_row(&m_row);
        }

        models_expander.add_row(&ActionRow::builder().child(&console_box).build());
        engine_group.add(&models_expander);
        root_box.append(&engine_group);

        // ---------------------------------------------------------
        // 3. Provider & Inference Endpoint Configuration
        // ---------------------------------------------------------
        let provider_group = PreferencesGroup::builder()
            .title("Inference Provider &amp; Model Selection")
            .description("Select between local Nemotron (llama.cpp/LM Studio/Ollama) or cloud endpoints")
            .build();

        let provider_model = gtk4::StringList::new(&[
            "Built-in Linux Assistant (Offline — Instant Zero-Download)",
            "Mithrandir In-App Nemotron (DFlash + llama.cpp — localhost:18080)",
            "LM Studio (Local Offline — localhost:1234)",
            "Ollama (Local Offline — localhost:11434)",
            "NVIDIA NIM Cloud API (Nemotron 3.5)",
            "Google Gemini (Google AI Studio Cloud)",
            "OpenAI / Custom Compatible Endpoint",
        ]);

        let provider_row = ComboRow::builder()
            .title("Active AI Provider")
            .subtitle("Backend inference source")
            .model(&provider_model)
            .selected(0)
            .build();
        provider_group.add(&provider_row);

        let url_row = ActionRow::builder()
            .title("Endpoint URL")
            .subtitle("Base URL of the active LLM API")
            .build();
        let url_entry = Entry::builder()
            .text("builtin://linux-mini")
            .hexpand(true)
            .valign(Align::Center)
            .build();
        url_row.add_suffix(&url_entry);
        provider_group.add(&url_row);

        let key_row = PasswordEntryRow::builder()
            .title("API Key (Optional for local offline providers)")
            .build();
        provider_group.add(&key_row);

        let model_row = ActionRow::builder()
            .title("Model Identifier")
            .subtitle("Model name passed in API requests")
            .build();
        let model_entry = Entry::builder()
            .text("builtin-linux-mini")
            .hexpand(true)
            .valign(Align::Center)
            .build();
        model_row.add_suffix(&model_entry);
        provider_group.add(&model_row);

        root_box.append(&provider_group);

        // ---------------------------------------------------------
        // 4. Interactive Linux Assistant & Custom Prompt
        // ---------------------------------------------------------
        let task_group = PreferencesGroup::builder()
            .title("Interactive Linux Assistant")
            .description("Ask any GNU/Linux optimization, command, or administration question")
            .build();

        let prompt_row = ActionRow::builder()
            .title("Command or Optimization Prompt")
            .build();

        let prompt_entry = Entry::builder()
            .placeholder_text("e.g. 'How to optimize BORE scheduler?' or 'Find files over 100MB'")
            .hexpand(true)
            .valign(Align::Center)
            .build();

        let ask_btn = Button::builder()
            .label("Ask AI")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();

        prompt_row.add_suffix(&prompt_entry);
        prompt_row.add_suffix(&ask_btn);
        task_group.add(&prompt_row);

        // Quick Optimization Chips
        let chips_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .margin_top(4)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();

        let chips = [
            ("Tune BORE Scheduler", "How do I optimize the CachyOS BORE scheduler for gaming and desktop responsiveness?"),
            ("Reclaim Standby RAM", "Analyze memory buffers and show safe command to drop caches"),
            ("Triage Failed Services", "Check failed systemd services and how to restart or reset them"),
            ("Find Large Files", "Find files exceeding 100MB in my home directory"),
            ("Check Listening Ports", "Display active listening network sockets and ports"),
        ];

        for (label, query) in chips {
            let chip_btn = Button::builder()
                .label(label)
                .css_classes(vec!["pill"])
                .build();

            let pe_c = prompt_entry.clone();
            let q_str = query.to_string();
            chip_btn.connect_clicked(move |_| {
                pe_c.set_text(&q_str);
            });
            chips_box.append(&chip_btn);
        }
        task_group.add(&ActionRow::builder().child(&chips_box).build());

        // Response View
        let response_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(WrapMode::Word)
            .monospace(true)
            .height_request(140)
            .build();
        response_view.buffer().set_text(
            "Ask a question above or click 'Run AI Optimization Triage' to analyze system diagnostics.",
        );

        let frame = Frame::builder()
            .child(&response_view)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        task_group.add(&ActionRow::builder().child(&frame).build());

        // Proposed Fix Action Row
        let proposed_row = ActionRow::builder()
            .title("Proposed Terminal Fix")
            .subtitle("No command proposed yet")
            .build();

        let exec_fix_btn = Button::builder()
            .label("Execute Fix")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .sensitive(false)
            .build();

        proposed_row.add_suffix(&exec_fix_btn);
        task_group.add(&proposed_row);

        root_box.append(&task_group);

        let clamp = Clamp::builder()
            .maximum_size(900)
            .child(&root_box)
            .build();
        scroller.set_child(Some(&clamp));

        // ---------------------------------------------------------
        // Signal Connectors & Logic
        // ---------------------------------------------------------

        // 1. Initial engine status check
        let update_engine_status = {
            let sr = status_row.clone();
            move || {
                let st = LocalLLMManager::check_llamacpp_status();
                sr.set_subtitle(&format!(
                    "Running: {} | Port: {} | Models: {} (DFlash: {}) | Disk: {:.1} MB",
                    if st.running { "Active on localhost:18080" } else { "Stopped" },
                    st.port,
                    st.models.len(),
                    if st.has_nemotron_dflash { "Installed" } else { "Missing" },
                    st.disk_usage_mb
                ));
            }
        };
        update_engine_status();

        // Start engine button
        {
            let u_status = update_engine_status.clone();
            let toast_rc_start = toast_rc.clone();
            start_engine_btn.connect_clicked(move |_| {
                match LocalLLMManager::start_llama_server(None) {
                    Ok(_) => {
                        u_status();
                        if let Some(ref o) = *toast_rc_start {
                            o.add_toast(Toast::new("Nemotron engine started on port 18080"));
                        }
                    }
                    Err(e) => {
                        if let Some(ref o) = *toast_rc_start {
                            o.add_toast(Toast::new(&format!("Engine start failed: {}", e)));
                        }
                    }
                }
            });
        }

        // Stop engine button
        {
            let u_status = update_engine_status.clone();
            let toast_rc_stop = toast_rc.clone();
            stop_engine_btn.connect_clicked(move |_| {
                let _ = LocalLLMManager::stop_llama_server();
                u_status();
                if let Some(ref o) = *toast_rc_stop {
                    o.add_toast(Toast::new("Nemotron engine stopped"));
                }
            });
        }

        // Provider sync
        {
            let ue = url_entry.clone();
            let me = model_entry.clone();
            provider_row.connect_selected_notify(move |combo| {
                match combo.selected() {
                    0 => {
                        ue.set_text("builtin://linux-mini");
                        me.set_text("builtin-linux-mini");
                    }
                    1 => {
                        ue.set_text("http://127.0.0.1:18080/v1");
                        me.set_text("Nemotron-3.5-Bolt-E4B-DFlash-BF16.gguf");
                    }
                    2 => {
                        ue.set_text("http://localhost:1234/v1");
                        me.set_text("nemotron-3.5-bolt-e4b");
                    }
                    3 => {
                        ue.set_text("http://localhost:11434/v1");
                        me.set_text("nemotron");
                    }
                    4 => {
                        ue.set_text("https://integrate.api.nvidia.com/v1");
                        me.set_text("nvidia/nemotron-3.5-bolt-e4b");
                    }
                    5 => {
                        ue.set_text("https://generativelanguage.googleapis.com/v1beta");
                        me.set_text("gemini-1.5-flash");
                    }
                    _ => {
                        ue.set_text("http://localhost:8000/v1");
                        me.set_text("default");
                    }
                }
            });
        }

        // Active proposed command storage
        let active_cmd_rc: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));

        // Connect Ask AI button
        {
            let pe = prompt_entry.clone();
            let ue = url_entry.clone();
            let ke = key_row.clone();
            let me = model_entry.clone();
            let rv = response_view.clone();
            let pr = proposed_row.clone();
            let eb = exec_fix_btn.clone();
            let ab = ask_btn.clone();
            let cmd_rc = active_cmd_rc.clone();

            ask_btn.connect_clicked(move |_| {
                let prompt = pe.text().to_string();
                if prompt.trim().is_empty() {
                    return;
                }

                ab.set_sensitive(false);
                ab.set_label("Thinking...");
                eb.set_sensitive(false);
                pr.set_subtitle("Thinking...");

                let url = ue.text().to_string();
                let key = ke.text().to_string();
                let model = me.text().to_string();

                let (tx, rx) = mpsc::channel();

                std::thread::spawn(move || {
                    let client = AIClient::new(&url, &key, &model);
                    let messages = vec![
                        ChatMessage {
                            role: "system".to_string(),
                            content: "You are Mithrandir's NVIDIA Nemotron 3.5 AI Linux Optimizer. Propose concise, safe terminal solutions. If a command fix applies, format it as PROPOSED_COMMAND: <cmd>.".to_string(),
                        },
                        ChatMessage {
                            role: "user".to_string(),
                            content: prompt,
                        },
                    ];
                    let res = client.chat(&messages);
                    let _ = tx.send(res);
                });

                let ab_c = ab.clone();
                let rv_c = rv.clone();
                let pr_c = pr.clone();
                let eb_c = eb.clone();
                let cmd_rc_c = cmd_rc.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
                    if let Ok(res) = rx.try_recv() {
                        ab_c.set_sensitive(true);
                        ab_c.set_label("Ask AI");

                        match res {
                            Ok(chat_resp) => {
                                rv_c.buffer().set_text(&chat_resp.content);
                                if let Some(ref cmd) = chat_resp.proposed_command {
                                    pr_c.set_subtitle(cmd);
                                    eb_c.set_sensitive(true);
                                    *cmd_rc_c.borrow_mut() = Some(cmd.clone());
                                } else {
                                    pr_c.set_subtitle("No command proposed");
                                    eb_c.set_sensitive(false);
                                    *cmd_rc_c.borrow_mut() = None;
                                }
                            }
                            Err(e) => {
                                rv_c.buffer().set_text(&format!("Error: {}", e));
                                pr_c.set_subtitle("Query failed");
                                eb_c.set_sensitive(false);
                                *cmd_rc_c.borrow_mut() = None;
                            }
                        }
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        // Connect Execute Fix button
        {
            let cmd_rc = active_cmd_rc.clone();
            let toast_rc_exec = toast_rc.clone();
            let eb = exec_fix_btn.clone();

            exec_fix_btn.connect_clicked(move |_| {
                if let Some(cmd) = cmd_rc.borrow().clone() {
                    let (safe, reason) = SafetyGuard::is_command_safe(&cmd);
                    if !safe {
                        if let Some(ref o) = *toast_rc_exec {
                            o.add_toast(Toast::new(&format!("Execution blocked: {}", reason)));
                        }
                        return;
                    }

                    eb.set_sensitive(false);
                    eb.set_label("Executing...");

                    let eb_clone = eb.clone();
                    let toast_clone = toast_rc_exec.clone();
                    let (tx, rx) = mpsc::channel();

                    std::thread::spawn(move || {
                        let res = HostBridge::run_command("sh", &["-c", &cmd]);
                        let _ = tx.send(res);
                    });

                    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                        if let Ok(res) = rx.try_recv() {
                            eb_clone.set_sensitive(true);
                            eb_clone.set_label("Execute Fix");
                            match res {
                                Ok((0, _, _)) => {
                                    if let Some(ref o) = *toast_clone {
                                        o.add_toast(Toast::new("Command executed successfully!"));
                                    }
                                }
                                Ok((code, _, err)) => {
                                    if let Some(ref o) = *toast_clone {
                                        o.add_toast(Toast::new(&format!("Command failed (code {}): {}", code, err.trim())));
                                    }
                                }
                                Err(e) => {
                                    if let Some(ref o) = *toast_clone {
                                        o.add_toast(Toast::new(&format!("Execution error: {}", e)));
                                    }
                                }
                            }
                            return glib::ControlFlow::Break;
                        }
                        glib::ControlFlow::Continue
                    });
                }
            });
        }

        // Connect Run AI Optimization Triage button
        {
            let run_btn = run_triage_btn.clone();
            let tp = triage_progress.clone();
            let rb = recs_box.clone();
            let ue = url_entry.clone();
            let ke = key_row.clone();
            let me = model_entry.clone();
            let toast_rc_triage = toast_rc.clone();

            run_triage_btn.connect_clicked(move |_| {
                run_btn.set_sensitive(false);
                run_btn.set_label("Analyzing System...");
                tp.set_visible(true);

                let url = ue.text().to_string();
                let key = ke.text().to_string();
                let model = me.text().to_string();

                let (tx, rx) = mpsc::channel();

                std::thread::spawn(move || {
                    let client = AIClient::new(&url, &key, &model);
                    let report = AIOptimizerEngine::run_ai_optimization(&client);
                    let _ = tx.send(report);
                });

                let run_btn_c = run_btn.clone();
                let tp_c = tp.clone();
                let rb_c = rb.clone();
                let toast_c = toast_rc_triage.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
                    if let Ok(res) = rx.try_recv() {
                        run_btn_c.set_sensitive(true);
                        run_btn_c.set_label("Run AI Optimization Triage");
                        tp_c.set_visible(false);

                        // Clear previous recommendations
                        while let Some(child) = rb_c.first_child() {
                            rb_c.remove(&child);
                        }

                        match res {
                            Ok(report) => {
                                if let Some(ref o) = *toast_c {
                                    o.add_toast(Toast::new("AI Optimization Triage completed!"));
                                }

                                // Header summary row
                                let summary_row = ActionRow::builder()
                                    .title("System Health &amp; Telemetry Status")
                                    .subtitle(format!(
                                        "CPU: {} | RAM: {} | Kernel: {}",
                                        report.cpu_summary, report.memory_summary, report.kernel_summary
                                    ))
                                    .build();
                                summary_row.add_prefix(&Image::from_icon_name("utilities-system-monitor-symbolic"));
                                rb_c.append(&summary_row);

                                // AI Insights Text
                                let insight_row = ActionRow::builder()
                                    .title("Nemotron AI Tuning Insights")
                                    .subtitle(&report.ai_insights)
                                    .build();
                                insight_row.add_prefix(&Image::from_icon_name("starred-symbolic"));
                                rb_c.append(&insight_row);

                                // Recommendation Cards
                                for rec in report.recommendations {
                                    let r_row = ActionRow::builder()
                                        .title(&rec.title)
                                        .subtitle(format!("[{}] {}", rec.category, rec.description))
                                        .build();

                                    let icon_name = match rec.severity.as_str() {
                                        "warning" => "dialog-warning-symbolic",
                                        "recommendation" => "emblem-favorite-symbolic",
                                        _ => "dialog-information-symbolic",
                                    };
                                    r_row.add_prefix(&Image::from_icon_name(icon_name));

                                    if let Some(cmd) = rec.proposed_command {
                                        let exec_btn = Button::builder()
                                            .label("Apply Fix")
                                            .css_classes(vec!["suggested-action", "pill"])
                                            .valign(Align::Center)
                                            .build();

                                        let toast_fix = toast_c.clone();
                                        let cmd_str = cmd.clone();
                                        exec_btn.connect_clicked(move |b| {
                                            b.set_sensitive(false);
                                            let cmd_run = cmd_str.clone();
                                            let toast_run = toast_fix.clone();
                                            let b_run = b.clone();
                                            let (tx, rx) = mpsc::channel();

                                            std::thread::spawn(move || {
                                                let r = HostBridge::run_command("sh", &["-c", &cmd_run]);
                                                let _ = tx.send(r);
                                            });

                                            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                                                if let Ok(r) = rx.try_recv() {
                                                    b_run.set_sensitive(true);
                                                    if let Ok((code, _, err)) = r {
                                                        if code == 0 {
                                                            if let Some(ref o) = *toast_run {
                                                                o.add_toast(Toast::new("Optimization applied successfully!"));
                                                            }
                                                        } else {
                                                            if let Some(ref o) = *toast_run {
                                                                o.add_toast(Toast::new(&format!("Fix failed ({}): {}", code, err.trim())));
                                                            }
                                                        }
                                                    }
                                                    return glib::ControlFlow::Break;
                                                }
                                                glib::ControlFlow::Continue
                                            });
                                        });

                                        r_row.add_suffix(&exec_btn);
                                    }

                                    rb_c.append(&r_row);
                                }
                            }
                            Err(e) => {
                                if let Some(ref o) = *toast_c {
                                    o.add_toast(Toast::new(&format!("Optimization triage failed: {}", e)));
                                }
                            }
                        }
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        scroller
    }
}
